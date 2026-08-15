//! 散在するExcelセル情報から `(0, 0)` 起点の `rows × cols` 矩形領域を構築し、
//! 値のないセルを `CellValue::Empty` で埋めた `domain::Grid<Cell>` を生成する
//! （docs/design/reader/grid_builder.md）。

use crate::domain;

use super::cell_mapper;

/// Excel既定の列幅。
const DEFAULT_COLUMN_WIDTH: f64 = 8.38;

pub(crate) fn build_grid(
    ws: &umya_spreadsheet::Worksheet,
    rows: usize,
    cols: usize,
) -> domain::Grid<domain::Cell> {
    // 1. 列幅を先に解決する（列ごとに1回、cols回だけumya-spreadsheetを問い合わせる）。
    let column_widths: Vec<f64> = (1..=cols as u32)
        .map(|col| {
            ws.column_dimension_by_number(col)
                .map(|c| c.width())
                .unwrap_or(DEFAULT_COLUMN_WIDTH)
        })
        .collect();

    // 2. CellValue::Empty相当のdomain::Cellでrows*cols件を初期化する。
    let mut cells: Vec<domain::Cell> = (0..rows * cols)
        .map(|i| empty_cell(column_widths[i % cols]))
        .collect();

    // 3. 実際に値の存在するセルだけを走査し、該当インデックスを上書きする。
    for excel_cell in ws.cells() {
        let (col, row) = (
            excel_cell.coordinate().col_num(),
            excel_cell.coordinate().row_num(),
        );
        // 3.1: highest_column_and_row()で求めた範囲を超えるセルは理論上存在しないはずだが、
        // umya-spreadsheet側の実装詳細を過信せず、範囲外インデックスへの書き込みで`cells`
        // の境界を超えないことを防御的にチェックする。
        if row == 0 || col == 0 || (row as usize) > rows || (col as usize) > cols {
            continue;
        }
        let (r, c) = (row as usize - 1, col as usize - 1);
        cells[r * cols + c] = cell_mapper::map_cell(excel_cell, column_widths[c]);
    }

    // 4. Grid::newは最後に1回だけ呼ぶ。
    domain::Grid::new(rows, cols, cells)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_grid_on_empty_worksheet_fills_default_cells() {
        let ws = umya_spreadsheet::Worksheet::default();
        let grid = build_grid(&ws, 2, 3);
        assert_eq!(grid.rows(), 2);
        assert_eq!(grid.cols(), 3);
        for row in grid.iter_rows() {
            for cell in row {
                assert!(cell.is_empty());
                assert_eq!(cell.column_width, DEFAULT_COLUMN_WIDTH);
            }
        }
    }

    #[test]
    fn build_grid_zero_rows_produces_empty_grid() {
        // xlsx.md 3章: 列数0のシート(rows=0, cols=1)を渡した場合の空Grid構築。
        let ws = umya_spreadsheet::Worksheet::default();
        let grid = build_grid(&ws, 0, 1);
        assert_eq!(grid.rows(), 0);
        assert_eq!(grid.cols(), 1);
        assert_eq!(grid.iter_rows().count(), 0);
    }

    #[test]
    fn build_grid_maps_a_populated_cell() {
        let mut ws = umya_spreadsheet::Worksheet::default();
        ws.cell_mut("A1").set_value("hello");
        let grid = build_grid(&ws, 1, 1);
        assert_eq!(
            grid.get(0, 0).unwrap().value,
            domain::CellValue::String("hello".to_string())
        );
    }
}
