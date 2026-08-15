//! `StrategyRegistry`、`StrategyConfig`、`select_auto`（docs/design/analysis/registry.md）。

use crate::domain;

use super::metrics;
use super::strategies::{self, GridPaperStrategy, TabularStrategy};
use super::strategy::AnalysisStrategy;

/// 各戦略のパラメータ（重み・しきい値）をハードコードせず、`StrategyRegistry`構築時に
/// 外部注入できるようにする。
pub struct StrategyConfig {
    /// `GridPaperStrategy::detect_overflow`が使うはみ出し判定の感度
    /// （CLI `--overflow-threshold`に対応、要件定義書5.1）。
    pub overflow_threshold: f64,
    /// `GridPaperStrategy::affinity`の重み。
    pub grid_paper_weights: strategies::grid_paper::Weights,
    /// `TabularStrategy::affinity`の重み。
    pub tabular_weights: strategies::tabular::Weights,
    /// `select_auto`で最上位2戦略の差がこの値未満の場合、`grid-paper`にフォールバックする。
    pub affinity_fallback_margin: f32,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            overflow_threshold: 1.0, // metricsでの保守的な既定値と同じ（heuristics.md 2章）
            grid_paper_weights: strategies::grid_paper::Weights::default(),
            tabular_weights: strategies::tabular::Weights::default(),
            affinity_fallback_margin: 0.05,
        }
    }
}

pub struct StrategyRegistry {
    strategies: Vec<Box<dyn AnalysisStrategy>>,
    fallback_margin: f32,
}

impl StrategyRegistry {
    pub fn with_defaults() -> Self {
        Self::with_config(StrategyConfig::default())
    }

    pub fn with_config(config: StrategyConfig) -> Self {
        Self {
            strategies: vec![
                Box::new(GridPaperStrategy::new(
                    config.overflow_threshold,
                    config.grid_paper_weights,
                )),
                Box::new(TabularStrategy::new(config.tabular_weights)),
            ],
            fallback_margin: config.affinity_fallback_margin,
        }
    }

    /// CLIで明示指定された場合（`--strategy grid-paper`等）。
    pub fn get(&self, id: &str) -> Option<&dyn AnalysisStrategy> {
        self.strategies
            .iter()
            .map(AsRef::as_ref)
            .find(|s| s.id() == id)
    }

    /// `--strategy auto`（デフォルト）の場合、affinityが最大の戦略を選ぶ。
    /// `SheetMetrics`はここで一度だけ計算し、登録された全戦略の`affinity`に配る
    /// （`analysis`内の他ファイルからはこの関数を経由せずに`SheetMetrics`を計算しないこと）。
    pub fn select_auto(&self, sheet: &domain::Sheet) -> &dyn AnalysisStrategy {
        let metrics = metrics::compute_sheet_metrics(sheet);

        let mut scored: Vec<(&dyn AnalysisStrategy, f32)> = self
            .strategies
            .iter()
            .map(AsRef::as_ref)
            .map(|s| (s, s.affinity(sheet, &metrics)))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));

        // 僅差なら「上位2戦略の中で」grid-paperを優先する。`.take(2)`を挟まないと、
        // 戦略が3つ以上ある場合に3位以下の低スコアなgrid-paperが誤って選ばれてしまう。
        if scored.len() >= 2 && (scored[0].1 - scored[1].1) < self.fallback_margin {
            if let Some(gp) = scored.iter().take(2).find(|(s, _)| s.id() == "grid-paper") {
                return gp.0;
            }
        }

        // `with_config`は常に1つ以上の戦略を登録するため到達しないが、将来の動的登録に
        // 備え、境界外アクセスではなく明示的なpanicにする。
        scored
            .first()
            .map(|(s, _)| *s)
            .expect("registry must not be empty")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_cell(s: &str, column_width: f64) -> domain::Cell {
        domain::Cell {
            value: domain::CellValue::String(s.into()),
            column_width,
            wrap_text: false,
            alignment: domain::Alignment::default(),
            font: domain::FontInfo {
                size_pt: 11.0,
                bold: false,
            },
            number_format: None,
        }
    }

    #[test]
    fn get_finds_registered_strategy_by_id() {
        let registry = StrategyRegistry::with_defaults();
        assert!(registry.get("grid-paper").is_some());
        assert!(registry.get("tabular").is_some());
        assert!(registry.get("unknown").is_none());
    }

    fn dense_regular_sheet() -> domain::Sheet {
        let cells = vec![
            text_cell("a", 8.0),
            text_cell("b", 8.0),
            text_cell("c", 8.0),
            text_cell("d", 8.0),
            text_cell("e", 8.0),
            text_cell("f", 8.0),
        ];
        domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(3, 2, cells),
            merges: vec![],
        }
    }

    #[test]
    fn select_auto_picks_tabular_for_dense_regular_sheet() {
        let registry = StrategyRegistry::with_defaults();
        let selected = registry.select_auto(&dense_regular_sheet());
        assert_eq!(selected.id(), "tabular");
    }

    /// `affinity_fallback_margin`の判定（`scored[0].1 - scored[1].1 < self.fallback_margin`）
    /// の`<`境界そのものを、実データのfixtureに頼らず決定的に検証する。`dense_regular_sheet`は
    /// 常にtabularがgrid-paperを上回るため、両者のaffinity差`diff`を実際に計算し、
    /// `affinity_fallback_margin`をその境界値ちょうど・直上・直下に設定して3点で確認する
    /// （境界値分析）。
    fn tabular_grid_paper_affinity_diff() -> f32 {
        let sheet = dense_regular_sheet();
        let metrics = metrics::compute_sheet_metrics(&sheet);
        let grid_paper = GridPaperStrategy::new(1.0, strategies::grid_paper::Weights::default());
        let tabular = TabularStrategy::new(strategies::tabular::Weights::default());
        let diff = tabular.affinity(&sheet, &metrics) - grid_paper.affinity(&sheet, &metrics);
        assert!(
            diff > 0.0,
            "tabular must score higher on this sheet for the boundary tests below to be meaningful"
        );
        diff
    }

    #[test]
    fn select_auto_does_not_fall_back_when_margin_equals_the_affinity_diff_exactly() {
        // `<`（`<=`ではない）ため、margin == diffちょうどはフォールバックの対象外。
        let diff = tabular_grid_paper_affinity_diff();
        let registry = StrategyRegistry::with_config(StrategyConfig {
            affinity_fallback_margin: diff,
            ..StrategyConfig::default()
        });
        assert_eq!(registry.select_auto(&dense_regular_sheet()).id(), "tabular");
    }

    #[test]
    fn select_auto_falls_back_when_margin_is_just_above_the_affinity_diff() {
        let diff = tabular_grid_paper_affinity_diff();
        let registry = StrategyRegistry::with_config(StrategyConfig {
            affinity_fallback_margin: diff + 0.001,
            ..StrategyConfig::default()
        });
        assert_eq!(
            registry.select_auto(&dense_regular_sheet()).id(),
            "grid-paper"
        );
    }

    #[test]
    fn select_auto_does_not_fall_back_when_margin_is_just_below_the_affinity_diff() {
        let diff = tabular_grid_paper_affinity_diff();
        let registry = StrategyRegistry::with_config(StrategyConfig {
            affinity_fallback_margin: diff - 0.001,
            ..StrategyConfig::default()
        });
        assert_eq!(registry.select_auto(&dense_regular_sheet()).id(), "tabular");
    }

    #[test]
    fn select_auto_falls_back_to_grid_paper_on_empty_sheet() {
        let registry = StrategyRegistry::with_defaults();
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(0, 1, vec![]),
            merges: vec![],
        };
        // 全指標が0.0の僅差状態のため、フォールバックでgrid-paperが選ばれる。
        assert_eq!(registry.select_auto(&sheet).id(), "grid-paper");
    }
}
