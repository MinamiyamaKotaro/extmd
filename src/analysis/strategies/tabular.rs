//! `TabularStrategy`（docs/design/analysis/strategies/tabular.md）。通常の集計表向け、
//! はみ出し結合を行わずセルをそのままテーブルセルとして扱う。

use crate::analysis::metrics::SheetMetrics;
use crate::analysis::strategy::{AnalysisStrategy, OverflowContext, OverflowDecision};
use crate::domain;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    pub fill_density: f32,
    pub row_structural_regularity: f32,
    pub low_overflow: f32,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            fill_density: 0.4,
            row_structural_regularity: 0.4,
            low_overflow: 0.2,
        }
    }
}

pub struct TabularStrategy {
    weights: Weights,
}

impl TabularStrategy {
    /// `registry::StrategyRegistry::with_config`からのみ構築させる。
    pub(in crate::analysis) fn new(weights: Weights) -> Self {
        Self { weights }
    }
}

impl AnalysisStrategy for TabularStrategy {
    fn id(&self) -> &'static str {
        "tabular"
    }

    fn affinity(&self, _sheet: &domain::Sheet, m: &SheetMetrics) -> f32 {
        let low_overflow = 1.0 - m.overflow_candidate_rate;
        self.weights.fill_density * m.fill_density
            + self.weights.row_structural_regularity * m.row_structural_regularity
            + self.weights.low_overflow * low_overflow
    }

    fn detect_overflow(&self, _ctx: &OverflowContext) -> OverflowDecision {
        OverflowDecision::NoMerge
    }

    fn classify_row(&self, _row: &domain::ResolvedRow) -> domain::RowKind {
        domain::RowKind::TableRow
    }

    fn heading_level(&self, _block: &domain::Block) -> Option<u8> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell() -> domain::Cell {
        domain::Cell {
            value: domain::CellValue::String("x".into()),
            column_width: 8.0,
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
    fn id_is_tabular() {
        let s = TabularStrategy::new(Weights::default());
        assert_eq!(s.id(), "tabular");
    }

    #[test]
    fn detect_overflow_always_no_merge() {
        let s = TabularStrategy::new(Weights::default());
        let source = cell();
        let ctx = OverflowContext {
            source: &source,
            following_empty_cells: &[],
        };
        assert_eq!(s.detect_overflow(&ctx), OverflowDecision::NoMerge);
    }

    #[test]
    fn classify_row_always_table_row() {
        let s = TabularStrategy::new(Weights::default());
        let blocks = vec![];
        let row = domain::ResolvedRow { blocks: &blocks };
        assert_eq!(s.classify_row(&row), domain::RowKind::TableRow);
    }

    #[test]
    fn heading_level_always_none() {
        let s = TabularStrategy::new(Weights::default());
        let b = domain::Block {
            row: 0,
            col_start: 0,
            col_end: 0,
            text: String::new(),
            font: domain::FontInfo {
                size_pt: 24.0,
                bold: true,
            },
            source: domain::BlockSource::Single,
            heading_level: None,
        };
        assert_eq!(s.heading_level(&b), None);
    }
}
