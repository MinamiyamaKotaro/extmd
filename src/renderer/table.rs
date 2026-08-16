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
        let cells = row_cells(row, col_count);
        // 縦方向のネイティブ結合（rowspan相当）は、値を左上セルの行にのみ出力し、
        // 結合範囲に含まれる後続行では空欄にする（analysis/mod.rs、Issue #52）。
        // ある行の全列がいずれかの縦結合の後続行になっている場合、その行はもう
        // 実質的な内容を持たないため、ヘッダー行（i == 0）を除いて出力しない。
        if i > 0 && cells.iter().all(String::is_empty) {
            continue;
        }
        lines.push(render_data_row(&cells));
        if i == 0 {
            lines.push(alignment_row(col_count));
        }
    }
    lines.join("\n")
}

/// 結合範囲は左端セル（`col_start`）にのみ値を出力し、`col_start+1..=col_end`は
/// 空セルのままにする（Markdownのパイプテーブルはネイティブなcolspan構文を持たないため）。
fn row_cells(row: &domain::RenderedRow, col_count: usize) -> Vec<String> {
    let mut cells = vec![String::new(); col_count];
    for block in &row.blocks {
        cells[block.col_start] = escape::escape_table_cell(&block.text);
    }
    cells
}

fn render_data_row(cells: &[String]) -> String {
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

    #[test]
    fn render_table_skips_fully_blank_data_rows() {
        // 縦方向のネイティブ結合（rowspan相当、Issue #52）の後続行は、全列が空欄の
        // `Block`（analysis/mod.rs）として渡ってくることがある。そのような行は実質的な
        // 内容を持たないため出力しない。
        let rows = vec![
            row(vec![block(0, 0, "h")]),
            row(vec![block(0, 0, "a")]),
            row(vec![block(0, 0, "")]),
            row(vec![block(0, 0, "b")]),
        ];
        let rendered = table_lines(&rows);
        assert_eq!(rendered, vec!["| h |", "|---|", "| a |", "| b |"]);
    }

    #[test]
    fn render_table_never_skips_the_header_row_even_if_blank() {
        let rows = vec![row(vec![block(0, 0, "")]), row(vec![block(0, 0, "a")])];
        let rendered = table_lines(&rows);
        assert_eq!(rendered, vec!["|  |", "|---|", "| a |"]);
    }

    #[test]
    fn render_table_keeps_rows_with_at_least_one_non_blank_column() {
        let rows = vec![
            row(vec![block(0, 0, "h"), block(1, 1, "h2")]),
            row(vec![block(0, 0, "a"), block(1, 1, "x")]),
            row(vec![block(0, 0, ""), block(1, 1, "y")]),
        ];
        let rendered = table_lines(&rows);
        assert_eq!(
            rendered,
            vec!["| h | h2 |", "|---|---|", "| a | x |", "|  | y |"]
        );
    }

    fn table_lines(rows: &[domain::RenderedRow]) -> Vec<String> {
        render_table(rows).lines().map(str::to_string).collect()
    }
}
