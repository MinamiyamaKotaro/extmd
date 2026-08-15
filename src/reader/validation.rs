//! Excelから取得した結合セル範囲を`domain::MergeRange`へ変換し、`Grid`の範囲外にある
//! 不正な結合情報を破棄（無視）する（docs/design/reader/validation.md）。

use crate::domain;

pub(crate) fn collect_valid_merges(
    ws: &umya_spreadsheet::Worksheet,
    rows: usize,
    cols: usize,
) -> Vec<domain::MergeRange> {
    // 範囲外・座標欠落のいずれの理由であっても、変換処理自体は継続する（3章の方針）。
    // `-v`/`--verbose`指定時にのみ表示されるよう`log::warn!`を用いる（実際にログを
    // 出力するかどうかはCLI層でのsubscriber初期化に委ねる。subscriber未初期化の場合、
    // `log`クレートのマクロ呼び出しは何もしない）。
    ws.merge_cells()
        .iter()
        .filter_map(|range| match to_domain_range(range) {
            Some(m) if is_within_bounds(&m, rows, cols) => Some(m),
            Some(m) => {
                log::warn!(
                    "Ignoring merge cell range {}:{}-{}:{} (out of sheet bounds {rows}x{cols})",
                    m.row_start + 1,
                    m.col_start + 1,
                    m.row_end + 1,
                    m.col_end + 1
                );
                None
            }
            None => {
                log::warn!(
                    "Ignoring a merge cell range with missing or invalid coordinate information"
                );
                None
            }
        })
        .collect()
}

fn to_domain_range(range: &umya_spreadsheet::Range) -> Option<domain::MergeRange> {
    // umya-spreadsheetは1-based座標のため、0-based(Gridの座標系)へ変換するには-1する。
    // 座標情報が欠落した不正なRangeの場合、`checked_sub(1)`が`None`を返し
    // `?`演算子でそのまま除外される。
    Some(domain::MergeRange {
        row_start: range.coordinate_start_row()?.num().checked_sub(1)? as usize,
        row_end: range.coordinate_end_row()?.num().checked_sub(1)? as usize,
        col_start: range.coordinate_start_col()?.num().checked_sub(1)? as usize,
        col_end: range.coordinate_end_col()?.num().checked_sub(1)? as usize,
    })
}

fn is_within_bounds(m: &domain::MergeRange, rows: usize, cols: usize) -> bool {
    m.row_end < rows && m.col_end < cols
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_valid_merges_on_worksheet_without_merges_is_empty() {
        let ws = umya_spreadsheet::Worksheet::default();
        assert!(collect_valid_merges(&ws, 10, 10).is_empty());
    }

    #[test]
    fn collect_valid_merges_keeps_in_bounds_merge() {
        let mut ws = umya_spreadsheet::Worksheet::default();
        ws.add_merge_cells("A1:B2");
        let merges = collect_valid_merges(&ws, 10, 10);
        assert_eq!(
            merges,
            vec![domain::MergeRange {
                row_start: 0,
                row_end: 1,
                col_start: 0,
                col_end: 1
            }]
        );
    }

    #[test]
    fn collect_valid_merges_discards_out_of_bounds_merge() {
        let mut ws = umya_spreadsheet::Worksheet::default();
        ws.add_merge_cells("A1:B2");
        // Gridは1x1しかないため、A1:B2は範囲外として破棄される。
        assert!(collect_valid_merges(&ws, 1, 1).is_empty());
    }
}
