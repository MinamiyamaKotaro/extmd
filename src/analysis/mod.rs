//! 設計方針・モジュール構成・`analyze`公開API・Analyzerのオーケストレーション
//! （docs/design/analysis/mod.md）。
//!
//! `analysis`は`domain`にのみ依存する（`reader`/`renderer`には依存しない）。内部モジュール
//! （`registry`/`metrics`/`heuristics`/`strategy`/`strategies`）は`analysis`外部には公開しない。

mod heuristics;
mod metrics;
mod registry;
mod strategies;
mod strategy;

use crate::domain;

pub use metrics::SheetMetrics;
pub use registry::{StrategyConfig, StrategyRegistry};
pub use strategy::AnalysisStrategy;

use strategy::{OverflowContext, OverflowDecision};

/// 1シートを、指定された戦略で`domain::Document`に変換する。戦略の選択（CLI指定 or
/// 自動判定）は呼び出し側（`lib.rs`）の責務とし、`analyze`は既に決定済みの`strategy`を
/// 受け取るだけとする。
pub fn analyze(sheet: &domain::Sheet, strategy: &dyn AnalysisStrategy) -> domain::Document {
    let rows = (0..sheet.cells.rows())
        .map(|row_idx| analyze_row(sheet, row_idx, strategy))
        .collect();

    domain::Document {
        sheet_name: sheet.name.clone(),
        rows,
    }
}

fn analyze_row(
    sheet: &domain::Sheet,
    row_idx: usize,
    strategy: &dyn AnalysisStrategy,
) -> domain::RenderedRow {
    let blocks = resolve_blocks(sheet, row_idx, strategy);
    let resolved = domain::ResolvedRow { blocks: &blocks };
    let kind = strategy.classify_row(&resolved);
    domain::RenderedRow { kind, blocks }
}

fn resolve_blocks(
    sheet: &domain::Sheet,
    row_idx: usize,
    strategy: &dyn AnalysisStrategy,
) -> Vec<domain::Block> {
    let row = sheet.cells.row(row_idx);
    let mut blocks = Vec::new();
    let mut col = 0;

    while col < row.len() {
        if let Some(merge) = native_merge_at(sheet, row_idx, col) {
            blocks.push(build_block(
                &row[col],
                row_idx,
                col,
                merge.col_end,
                domain::BlockSource::NativeMerge,
                strategy,
            ));
            col = merge.col_end + 1;
            continue;
        }

        if row[col].is_empty() {
            // 縦方向のネイティブ結合セル（rowspan相当）の、左上以外の行に達した場合。
            // Markdownのパイプテーブルはrowspanを持たないため、横方向の結合（colspan、
            // build_blockの呼び出し元コメント参照）と同じ方針で、値は左上セルの行にのみ
            // 出力し、この行では空欄にする（Issue #52。全列が縦結合される業務フォーマット
            // では、値を複製すると実質差分のない行がテーブルに繰り返し出力されて冗長になる
            // ため、Issue #46の複製方針を撤回した）。
            //
            // ただしBlock自体はここで生成し、テキストのみ空にする。生成をやめてしまうと、
            // 「全列が縦結合されている行」でこの行のblocksが空になり、classify_row
            // （tabular.md/grid_paper.md）がFlowと誤判定してテーブルのグループ化が
            // 分断されてしまう（render_body、renderer/mod.md）。col_start/col_endは
            // 結合範囲のまま保持することで、列数（col_count）や行の構造的な扱いは
            // 左上セルの行と変わらないようにする。
            if let Some(merge) = covering_merge(sheet, row_idx, col) {
                let top_left = sheet
                    .cells
                    .get(merge.row_start, merge.col_start)
                    .expect("MergeRange must be within cells bounds (Sheet invariant)");
                let mut block = build_block(
                    top_left,
                    row_idx,
                    merge.col_start,
                    merge.col_end,
                    domain::BlockSource::NativeMerge,
                    strategy,
                );
                block.text.clear();
                blocks.push(block);
                col = merge.col_end + 1;
                continue;
            }

            col += 1;
            continue;
        }

        let following = trailing_empty(sheet, row_idx, &row[col + 1..], col + 1);
        let ctx = OverflowContext {
            source: &row[col],
            following_empty_cells: following,
        };
        let (source, col_end) = match strategy.detect_overflow(&ctx) {
            OverflowDecision::NoMerge => (domain::BlockSource::Single, col),
            OverflowDecision::MergeCells { count } => {
                // `AnalysisStrategy`の実装は`following.len()`を超える`count`を返さない契約だが、
                // 将来のカスタム戦略やバグによる契約違反で行の境界を超えないよう、ここで
                // クランプして防御する（PR #21レビューコメントでの指摘を反映）。
                let clamped_count = count.min(following.len());
                (domain::BlockSource::OverflowMerge, col + clamped_count)
            }
        };
        blocks.push(build_block(
            &row[col], row_idx, col, col_end, source, strategy,
        ));
        col = col_end + 1;
    }

    blocks
}

/// `following`（対象セルの右隣から続くセル列）のうち、「連続する空セル」を返す。ネイティブ
/// 結合セルの範囲（左上セルに限らない）に達した時点で打ち切る。結合範囲の左上以外のセルも
/// グリッド上は空（`CellValue::Empty`）として表現されるため、単純な`is_empty()`だけでは
/// 結合セルの領域まではみ出し結合の対象に含めてしまうため境界チェックする。
fn trailing_empty<'a>(
    sheet: &domain::Sheet,
    row_idx: usize,
    following: &'a [domain::Cell],
    start_col: usize,
) -> &'a [domain::Cell] {
    let mut len = 0;
    for (i, cell) in following.iter().enumerate() {
        if !cell.is_empty() || is_in_native_merge(sheet, row_idx, start_col + i) {
            break;
        }
        len = i + 1;
    }
    &following[..len]
}

/// 指定座標がいずれかのネイティブ結合セルの範囲内（左上セルに限らない）に含まれるか。
fn is_in_native_merge(sheet: &domain::Sheet, row: usize, col: usize) -> bool {
    covering_merge(sheet, row, col).is_some()
}

/// 指定座標を含む（左上セルに限らない）ネイティブ結合セルの範囲を返す。
fn covering_merge(sheet: &domain::Sheet, row: usize, col: usize) -> Option<&domain::MergeRange> {
    sheet.merges.iter().find(|m| {
        (m.row_start..=m.row_end).contains(&row) && (m.col_start..=m.col_end).contains(&col)
    })
}

/// 指定座標が、いずれかのネイティブ結合セル範囲の左上セルであれば、その範囲を返す。
fn native_merge_at(sheet: &domain::Sheet, row: usize, col: usize) -> Option<&domain::MergeRange> {
    sheet
        .merges
        .iter()
        .find(|m| m.row_start == row && m.col_start == col)
}

/// `Block`を構築し、その場で`heading_level`を確定させて格納する。テキスト・フォントは
/// 結合/はみ出し範囲の左上セル（`source_cell`）の値をそのまま使う。
fn build_block(
    source_cell: &domain::Cell,
    row: usize,
    col_start: usize,
    col_end: usize,
    source: domain::BlockSource,
    strategy: &dyn AnalysisStrategy,
) -> domain::Block {
    let mut block = domain::Block {
        row,
        col_start,
        col_end,
        text: source_cell.display_text(),
        font: source_cell.font.clone(),
        source,
        heading_level: None,
    };
    block.heading_level = strategy.heading_level(&block);
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(value: domain::CellValue, column_width: f64, wrap_text: bool) -> domain::Cell {
        domain::Cell {
            value,
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

    fn empty(column_width: f64) -> domain::Cell {
        cell(domain::CellValue::Empty, column_width, false)
    }

    fn text(s: &str, column_width: f64) -> domain::Cell {
        cell(domain::CellValue::String(s.into()), column_width, false)
    }

    /// `AnalysisStrategy`実装が契約(`count <= following.len()`)に反して行の境界を超える
    /// `count`を返しても`resolve_blocks`側でクランプされることを確かめるためのテスト専用戦略。
    struct AlwaysOverflowStrategy;

    impl AnalysisStrategy for AlwaysOverflowStrategy {
        fn id(&self) -> &'static str {
            "fake-overflow"
        }

        fn affinity(&self, _sheet: &domain::Sheet, _metrics: &SheetMetrics) -> f32 {
            0.0
        }

        fn detect_overflow(&self, _ctx: &OverflowContext) -> OverflowDecision {
            OverflowDecision::MergeCells { count: 9999 }
        }

        fn classify_row(&self, _row: &domain::ResolvedRow) -> domain::RowKind {
            domain::RowKind::TableRow
        }

        fn heading_level(&self, _block: &domain::Block) -> Option<u8> {
            None
        }
    }

    #[test]
    fn resolve_blocks_clamps_overflow_count_to_available_columns() {
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(1, 3, vec![text("a", 4.0), empty(4.0), empty(4.0)]),
            merges: vec![],
        };
        let blocks = resolve_blocks(&sheet, 0, &AlwaysOverflowStrategy);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].col_end, 2);
    }

    #[test]
    fn analyze_produces_one_rendered_row_per_sheet_row() {
        let sheet = domain::Sheet {
            name: "Sheet1".into(),
            cells: domain::Grid::new(2, 2, vec![text("a", 8.0); 4]),
            merges: vec![],
        };
        let strategy = registry::StrategyRegistry::with_defaults();
        let strategy = strategy.get("tabular").unwrap();
        let doc = analyze(&sheet, strategy);
        assert_eq!(doc.sheet_name, "Sheet1");
        assert_eq!(doc.rows.len(), 2);
        assert!(doc.rows.iter().all(|r| r.kind == domain::RowKind::TableRow));
    }

    #[test]
    fn resolve_blocks_skips_empty_cells() {
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(1, 3, vec![empty(8.0), text("b", 8.0), empty(8.0)]),
            merges: vec![],
        };
        let strategy = registry::StrategyRegistry::with_defaults();
        let strategy = strategy.get("tabular").unwrap();
        let blocks = resolve_blocks(&sheet, 0, strategy);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].col_start, 1);
        assert_eq!(blocks[0].text, "b");
    }

    #[test]
    fn resolve_blocks_uses_native_merge_range() {
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(1, 3, vec![text("merged", 8.0), empty(8.0), empty(8.0)]),
            merges: vec![domain::MergeRange {
                row_start: 0,
                row_end: 0,
                col_start: 0,
                col_end: 2,
            }],
        };
        let strategy = registry::StrategyRegistry::with_defaults();
        let strategy = strategy.get("tabular").unwrap();
        let blocks = resolve_blocks(&sheet, 0, strategy);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].col_end, 2);
        assert_eq!(blocks[0].source, domain::BlockSource::NativeMerge);
    }

    #[test]
    fn trailing_empty_stops_at_native_merge_boundary() {
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(
                1,
                4,
                vec![text("a", 4.0), empty(4.0), empty(4.0), empty(4.0)],
            ),
            merges: vec![domain::MergeRange {
                row_start: 0,
                row_end: 0,
                col_start: 2,
                col_end: 3,
            }],
        };
        let row = sheet.cells.row(0);
        let following = trailing_empty(&sheet, 0, &row[1..], 1);
        assert_eq!(following.len(), 1);
    }

    #[test]
    fn resolve_blocks_grid_paper_overflow_merges_cells() {
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(
                1,
                3,
                vec![
                    text("very long overflowing text", 4.0),
                    empty(4.0),
                    empty(4.0),
                ],
            ),
            merges: vec![],
        };
        let strategy = registry::StrategyRegistry::with_defaults();
        let strategy = strategy.get("grid-paper").unwrap();
        let blocks = resolve_blocks(&sheet, 0, strategy);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].source, domain::BlockSource::OverflowMerge);
        assert!(blocks[0].col_end > 0);
    }

    #[test]
    fn resolve_blocks_blanks_vertical_merge_continuation_rows() {
        // 縦方向のネイティブ結合セル（rowspan相当、Issue #52）: A列は結合なしで
        // 各行に値が入り、B列は行0-2が"merged"というテキストで縦結合されている。
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(
                3,
                2,
                vec![
                    text("row0", 8.0),
                    text("merged", 8.0),
                    text("row1", 8.0),
                    empty(8.0),
                    text("row2", 8.0),
                    empty(8.0),
                ],
            ),
            merges: vec![domain::MergeRange {
                row_start: 0,
                row_end: 2,
                col_start: 1,
                col_end: 1,
            }],
        };
        let strategy = registry::StrategyRegistry::with_defaults();
        let strategy = strategy.get("tabular").unwrap();

        let top_blocks = resolve_blocks(&sheet, 0, strategy);
        assert_eq!(top_blocks.len(), 2);
        assert_eq!(top_blocks[1].text, "merged");
        assert_eq!(top_blocks[1].source, domain::BlockSource::NativeMerge);

        for row_idx in [1, 2] {
            let blocks = resolve_blocks(&sheet, row_idx, strategy);
            assert_eq!(
                blocks.len(),
                2,
                "row {row_idx} should still get a placeholder block so the row keeps being classified as a table row"
            );
            assert_eq!(
                blocks[1].text, "",
                "continuation row must not repeat the merged value"
            );
            assert_eq!(blocks[1].source, domain::BlockSource::NativeMerge);
            assert_eq!(blocks[1].col_start, 1);
            assert_eq!(blocks[1].col_end, 1);
            assert_eq!(
                blocks[1].row, row_idx,
                "placeholder block should report the current row, not the merge's top row"
            );
        }
    }
}
