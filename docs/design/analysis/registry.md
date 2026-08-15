# `analysis::registry` 設計書

対象: [analysis/mod.md](mod.md)の対応表における `registry.rs`。

## 1. `StrategyConfig`

各戦略のパラメータ（重み・しきい値）をハードコードせず、`StrategyRegistry`構築時に
外部注入できるようにする
（[Issue #6での決定](https://github.com/MinamiyamaKotaro/extmd/issues/6#issuecomment-5301777202)）。

```rust
pub struct StrategyConfig {
    /// `GridPaperStrategy::detect_overflow`が使うはみ出し判定の感度
    /// （CLI `--overflow-threshold`に対応、要件定義書5.1）。
    pub overflow_threshold: f64,
    /// `GridPaperStrategy::affinity`の重み（strategies/grid_paper.md 1章）。
    pub grid_paper_weights: strategies::grid_paper::Weights,
    /// `TabularStrategy::affinity`の重み（strategies/tabular.md 1章）。
    pub tabular_weights: strategies::tabular::Weights,
    /// `select_auto`で最上位2戦略の差がこの値未満の場合、`grid-paper`にフォールバックする（3章）。
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
```

`overflow_threshold`のみ他フィールドと役割が異なる点に注意: `metrics::compute_sheet_metrics`
が使う既定しきい値（[heuristics.md 2章](heuristics.md#2-is_overflow_candidate)）とは別に、
`GridPaperStrategy`自身の`detect_overflow`で使う戦略固有のしきい値を指す。
CLIの`--overflow-threshold`（要件定義書5.1）はこちらに反映する。

## 2. `StrategyRegistry`

```rust
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
                Box::new(GridPaperStrategy::new(config.overflow_threshold, config.grid_paper_weights)),
                Box::new(TabularStrategy::new(config.tabular_weights)),
            ],
            fallback_margin: config.affinity_fallback_margin,
        }
    }

    /// CLIで明示指定された場合（`--strategy grid-paper`等）。
    pub fn get(&self, id: &str) -> Option<&dyn AnalysisStrategy> {
        self.strategies.iter().map(AsRef::as_ref).find(|s| s.id() == id)
    }

    /// `--strategy auto`（デフォルト）の場合、affinityが最大の戦略を選ぶ。
    /// `SheetMetrics`はここで一度だけ計算し、全戦略のaffinityに配る
    /// （metrics.md 3-4章の可視性制限により、この関数以外からは呼べない）。
    pub fn select_auto(&self, sheet: &Sheet) -> &dyn AnalysisStrategy {
        let metrics = metrics::compute_sheet_metrics(sheet);

        let mut scored: Vec<(&dyn AnalysisStrategy, f32)> = self.strategies
            .iter()
            .map(AsRef::as_ref)
            .map(|s| (s, s.affinity(sheet, &metrics)))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));

        // 3章: 僅差なら「上位2戦略の中で」grid-paperを優先する。
        // `.take(2)`を挟まないと、戦略が3つ以上ある場合に3位以下の低スコアな
        // grid-paperが誤って選ばれてしまう
        // （[PR #7のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/7#issuecomment-5301859980)での指摘を反映）。
        if scored.len() >= 2 && (scored[0].1 - scored[1].1) < self.fallback_margin {
            if let Some(gp) = scored.iter().take(2).find(|(s, _)| s.id() == "grid-paper") {
                return gp.0;
            }
        }

        // `with_config`は常に1つ以上の戦略を登録するため到達しないが、将来の動的登録
        // （strategies/mod.md 3章）に備え、境界外アクセスではなく明示的なpanicにする
        // （同レビューコメントでの指摘を反映）。
        scored.first().map(|(s, _)| *s).expect("registry must not be empty")
    }
}
```

登録される戦略一覧は`with_defaults`/`with_config`内で固定する。v1では
[strategies/mod.md 2章](strategies/mod.md#2-v1スコープ-grid-paper--tabular-の2戦略のみ)の通り
`grid-paper`/`tabular`の2つのみ登録する。

## 3. 僅差時のフォールバック（`affinity_fallback_margin`）

`select_auto`で最大スコアの戦略を選ぶ際、最上位2戦略の差が小さい場合は、要件上の主要
ユースケースである`grid-paper`を優先してフォールバックする。閾値の初期値は`0.05`
（1章）とするが、実データでの検証を経てチューニングする（5章）。

## 4. `SheetMetrics`計算契約の強制

- `compute_sheet_metrics`は`pub(in crate::analysis)`（[metrics.md 3-4章](metrics.md)）のため、
  `analysis`外部からの呼び出しはコンパイルエラーになる。`analysis`内での呼び出しを
  `select_auto`だけに限定する可視性レベルの強制手段はないが、`select_auto`の
  docstringに「1シートに対して1回のみ`SheetMetrics`を計算し、登録された全戦略に配る」
  契約を明記することで、`analysis`内の他ファイル（`strategies/*`等）からの誤用を防ぐ。

## 5. CLIとの境界

`--strategy`/`--overflow-threshold`/`--no-overflow-merge`（要件定義書5.1）から
`StrategyConfig`を組み立てる処理は`analysis`層の外（`cli.rs`/`lib.rs`）の責務とする。
v1ではCLI引数からの上書きのみをサポートし、TOML等の設定ファイルは対象外とする
（[Issue #6での決定](https://github.com/MinamiyamaKotaro/extmd/issues/6#issuecomment-5301777202)）。
`--strategy`で戦略を明示指定した場合は`select_auto`を呼ばず`get(id)`を使うため、
`SheetMetrics`自体を計算しない（無駄な走査を避ける）。

## 6. 未確定事項

- `cli.rs`から`StrategyConfig`を組み立てる際の具体的なフィールド対応・デフォルト値の
  妥当性検証は`cli.rs`の設計フェーズで詰める
- `affinity_fallback_margin`（0.05）自体のチューニングは実データ検証が必要
