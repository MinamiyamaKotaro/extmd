//! `GridPaperStrategy`（docs/design/analysis/strategies/grid_paper.md）。方眼紙的な文書向け、
//! デフォルトの解析戦略。

use crate::analysis::heuristics;
use crate::analysis::metrics::SheetMetrics;
use crate::analysis::strategy::{AnalysisStrategy, OverflowContext, OverflowDecision};
use crate::domain;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    pub narrow_columns: f32,
    pub uniformity: f32,
    pub overflow_signal: f32,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            narrow_columns: 0.3,
            uniformity: 0.3,
            overflow_signal: 0.4,
        }
    }
}

pub struct GridPaperStrategy {
    overflow_threshold: f64,
    weights: Weights,
}

impl GridPaperStrategy {
    /// `registry::StrategyRegistry::with_config`からのみ構築させる
    /// （`analysis`外から個別に構築させない）。
    pub(in crate::analysis) fn new(overflow_threshold: f64, weights: Weights) -> Self {
        Self {
            overflow_threshold,
            weights,
        }
    }
}

impl AnalysisStrategy for GridPaperStrategy {
    fn id(&self) -> &'static str {
        "grid-paper"
    }

    fn affinity(&self, _sheet: &domain::Sheet, m: &SheetMetrics) -> f32 {
        let narrow_columns = heuristics::normalize_inverse(m.avg_column_width, 2.0, 12.0);
        let uniformity =
            1.0 - heuristics::normalize(m.column_width_stddev, 0.0, m.avg_column_width.max(1.0));
        let overflow_signal = m.overflow_candidate_rate;

        self.weights.narrow_columns * narrow_columns
            + self.weights.uniformity * uniformity
            + self.weights.overflow_signal * overflow_signal
    }

    fn detect_overflow(&self, ctx: &OverflowContext) -> OverflowDecision {
        if !heuristics::is_overflow_candidate(
            ctx.source,
            ctx.following_empty_cells.first(),
            self.overflow_threshold,
        ) {
            return OverflowDecision::NoMerge;
        }

        let estimated_width = heuristics::estimate_render_width(ctx.source);
        let mut consumed = ctx.source.column_width;
        let mut count = 0;

        for cell in ctx.following_empty_cells {
            if consumed >= estimated_width {
                break;
            }
            consumed += cell.column_width;
            count += 1;
        }

        OverflowDecision::MergeCells { count }
    }

    fn classify_row(&self, row: &domain::ResolvedRow) -> domain::RowKind {
        // 要件定義書5.3.4: 1行が単一の大きなテキストブロックで構成される場合はFlow。
        // これがないと、はみ出し・結合の有無によらない単独セルの短い見出し・段落を
        // 誤ってテーブル扱いしてしまう。
        if row.blocks.len() == 1 {
            return domain::RowKind::Flow;
        }

        // 複数ブロックの行は、少数(3以下)の大きなブロック(span() > 1)で構成される
        // 場合はFlow、そうでなければTableRowとする。
        let flow_like = row.blocks.iter().filter(|b| b.span() > 1).count();
        if row.blocks.len() <= 3 && flow_like > 0 {
            domain::RowKind::Flow
        } else {
            domain::RowKind::TableRow
        }
    }

    fn heading_level(&self, block: &domain::Block) -> Option<u8> {
        let size = block.font.size_pt;

        // 14pt以上は太字不問で見出しとみなす。14pt未満は通常の強調表示と区別するため
        // 太字を必須条件とする。
        if size < 14.0 && !block.font.bold {
            return None;
        }

        match size {
            s if s >= 18.0 => Some(1),
            s if s >= 16.0 => Some(2),
            s if s >= 14.0 => Some(3),
            s if s >= 12.0 => Some(4),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(text: &str, column_width: f64, wrap_text: bool) -> domain::Cell {
        domain::Cell {
            value: domain::CellValue::String(text.into()),
            column_width,
            wrap_text,
            alignment: domain::Alignment::default(),
            font: domain::FontInfo {
                size_pt: 11.0,
                bold: false,
            },
            number_format: None,
        }
    }

    fn empty_cell(column_width: f64) -> domain::Cell {
        domain::Cell {
            value: domain::CellValue::Empty,
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

    fn block(col_start: usize, col_end: usize) -> domain::Block {
        domain::Block {
            row: 0,
            col_start,
            col_end,
            text: String::new(),
            font: domain::FontInfo {
                size_pt: 11.0,
                bold: false,
            },
            source: domain::BlockSource::Single,
            heading_level: None,
        }
    }

    #[test]
    fn id_is_grid_paper() {
        let s = GridPaperStrategy::new(1.0, Weights::default());
        assert_eq!(s.id(), "grid-paper");
    }

    #[test]
    fn detect_overflow_no_merge_when_not_a_candidate() {
        let s = GridPaperStrategy::new(1.0, Weights::default());
        let source = cell("short", 20.0, false);
        let ctx = OverflowContext {
            source: &source,
            following_empty_cells: &[],
        };
        assert_eq!(s.detect_overflow(&ctx), OverflowDecision::NoMerge);
    }

    #[test]
    fn detect_overflow_merges_until_width_consumed() {
        let s = GridPaperStrategy::new(1.0, Weights::default());
        let source = cell("long text here", 4.0, false);
        let following = vec![empty_cell(4.0), empty_cell(4.0), empty_cell(4.0)];
        let ctx = OverflowContext {
            source: &source,
            following_empty_cells: &following,
        };
        match s.detect_overflow(&ctx) {
            OverflowDecision::MergeCells { count } => assert!((1..=3).contains(&count)),
            OverflowDecision::NoMerge => panic!("expected merge"),
        }
    }

    #[test]
    fn classify_row_single_block_is_flow() {
        let s = GridPaperStrategy::new(1.0, Weights::default());
        let blocks = vec![block(0, 0)];
        let row = domain::ResolvedRow { blocks: &blocks };
        assert_eq!(s.classify_row(&row), domain::RowKind::Flow);
    }

    #[test]
    fn classify_row_many_narrow_blocks_is_table_row() {
        let s = GridPaperStrategy::new(1.0, Weights::default());
        let blocks = vec![block(0, 0), block(1, 1), block(2, 2), block(3, 3)];
        let row = domain::ResolvedRow { blocks: &blocks };
        assert_eq!(s.classify_row(&row), domain::RowKind::TableRow);
    }

    #[test]
    fn classify_row_few_wide_blocks_is_flow() {
        let s = GridPaperStrategy::new(1.0, Weights::default());
        let blocks = vec![block(0, 3), block(4, 4)];
        let row = domain::ResolvedRow { blocks: &blocks };
        assert_eq!(s.classify_row(&row), domain::RowKind::Flow);
    }

    #[test]
    fn heading_level_large_font_ignores_bold() {
        let s = GridPaperStrategy::new(1.0, Weights::default());
        let mut b = block(0, 0);
        b.font.size_pt = 18.0;
        assert_eq!(s.heading_level(&b), Some(1));
    }

    #[test]
    fn heading_level_small_font_requires_bold() {
        let s = GridPaperStrategy::new(1.0, Weights::default());
        let mut b = block(0, 0);
        b.font.size_pt = 12.0;
        assert_eq!(s.heading_level(&b), None);
        b.font.bold = true;
        assert_eq!(s.heading_level(&b), Some(4));
    }

    #[test]
    fn heading_level_normal_bold_text_is_not_a_heading() {
        let s = GridPaperStrategy::new(1.0, Weights::default());
        let mut b = block(0, 0);
        b.font.size_pt = 10.0;
        b.font.bold = true;
        assert_eq!(s.heading_level(&b), None);
    }
}
