//! `RowKind::TableRow`の連続行のグループ化とMarkdownパイプテーブル構築
//! （docs/design/renderer/table.md）。

use crate::domain;

use super::escape;

/// `mod.rs`がグループ化した、連続する`RowKind::TableRow`の`RenderedRow`列を、
/// 1つのMarkdownパイプテーブルへ変換する。グループ先頭行を常にヘッダー行として扱う。
pub(in crate::renderer) fn render_table(rows: &[domain::RenderedRow]) -> String {
    let col_count = rows
        .iter()
        .flat_map(|row| row.blocks.iter())
        .map(|b| b.col_end + 1)
        .max()
        .unwrap_or(0);

    let mut lines = Vec::with_capacity(rows.len() + 1);
    for (i, row) in rows.iter().enumerate() {
        lines.push(render_data_row(row, col_count));
        if i == 0 {
            lines.push(alignment_row(col_count));
        }
    }
    lines.join("\n")
}

/// 結合範囲は左端セル（`col_start`）にのみ値を出力し、`col_start+1..=col_end`は
/// 空セルのままにする（Markdownのパイプテーブルはネイティブなcolspan構文を持たないため）。
fn render_data_row(row: &domain::RenderedRow, col_count: usize) -> String {
    let mut cells = vec![String::new(); col_count];
    for block in &row.blocks {
        cells[block.col_start] = escape::escape_table_cell(&block.text);
    }
    format!("| {} |", cells.join(" | "))
}

fn alignment_row(col_count: usize) -> String {
    format!("|{}|", vec!["---"; col_count].join("|"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(col_start: usize, col_end: usize, text: &str) -> domain::Block {
        domain::Block {
            row: 0,
            col_start,
            col_end,
            text: text.into(),
            font: domain::FontInfo {
                size_pt: 11.0,
                bold: false,
            },
            source: domain::BlockSource::Single,
            heading_level: None,
        }
    }

    fn row(blocks: Vec<domain::Block>) -> domain::RenderedRow {
        domain::RenderedRow {
            kind: domain::RowKind::TableRow,
            blocks,
        }
    }

    #[test]
    fn render_table_emits_header_and_alignment_row() {
        let rows = vec![
            row(vec![block(0, 0, "a"), block(1, 1, "b")]),
            row(vec![block(0, 0, "1"), block(1, 1, "2")]),
        ];
        let rendered = table_lines(&rows);
        assert_eq!(rendered, vec!["| a | b |", "|---|---|", "| 1 | 2 |"]);
    }

    #[test]
    fn render_table_places_merged_block_at_col_start_and_leaves_rest_empty() {
        let rows = vec![row(vec![block(0, 2, "merged"), block(3, 3, "x")])];
        let rendered = table_lines(&rows);
        assert_eq!(rendered[0], "| merged |  |  | x |");
    }

    #[test]
    fn render_table_col_count_is_max_across_all_rows() {
        let rows = vec![row(vec![block(0, 0, "a")]), row(vec![block(0, 2, "wide")])];
        let rendered = table_lines(&rows);
        assert_eq!(rendered[0], "| a |  |  |");
        assert_eq!(rendered[2], "| wide |  |  |");
    }

    #[test]
    fn render_table_escapes_cell_text() {
        let rows = vec![row(vec![block(0, 0, "a|b")])];
        let rendered = table_lines(&rows);
        assert_eq!(rendered[0], "| a\\|b |");
    }

    fn table_lines(rows: &[domain::RenderedRow]) -> Vec<String> {
        render_table(rows).lines().map(str::to_string).collect()
    }
}
