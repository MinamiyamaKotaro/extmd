//! `umya-spreadsheet`を用いたファイル・ワークブック・ワークシート操作のライフサイクルを
//! 管理し、他の子モジュールを呼び出して結果を統合する、`reader`モジュールの
//! 唯一の「まとめ役」（docs/design/reader/xlsx.md）。

use crate::domain;

use super::{grid_builder, validation, ReaderError};

pub(crate) fn read_sheets(
    path: &std::path::Path,
    max_cells: usize,
) -> Result<Vec<domain::Sheet>, ReaderError> {
    let book = umya_spreadsheet::reader::xlsx::read(path)
        .map_err(|e| ReaderError::Parse(e.to_string()))?;

    book.sheet_collection()
        .iter()
        .map(|ws| build_sheet(ws, max_cells))
        .collect()
}

fn build_sheet(
    ws: &umya_spreadsheet::Worksheet,
    max_cells: usize,
) -> Result<domain::Sheet, ReaderError> {
    let (highest_col, highest_row) = ws.highest_column_and_row();

    // 列数0シートの扱い: umya-spreadsheetはデータが1つもないシートに対して(0, 0)を返す。
    // Grid::newはcols > 0を必須とするため、rows=0, cols=1として構築する。
    let (rows, cols) = if highest_col == 0 {
        (0, 1)
    } else {
        (highest_row as usize, highest_col as usize)
    };

    // 悪意ある/破損したファイルがメタデータ上の座標のみを巨大化させ、
    // grid_builder::build_gridのrows * cols件のメモリ確保でDoSを引き起こすことを防ぐ
    // （reader/mod.md 4.1節、docs/security/design-review.md #2、Issue #14）。
    let cell_count = rows.saturating_mul(cols);
    if cell_count > max_cells {
        return Err(ReaderError::SheetTooLarge {
            name: ws.name().to_string(),
            rows,
            cols,
            limit: max_cells,
        });
    }

    let cells = grid_builder::build_grid(ws, rows, cols);
    let merges = validation::collect_valid_merges(ws, rows, cols);

    Ok(domain::Sheet {
        name: ws.name().to_string(),
        cells,
        merges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_sheets_rejects_nonexistent_path() {
        let result = read_sheets(
            std::path::Path::new("/nonexistent/path/does-not-exist.xlsx"),
            1_000_000,
        );
        assert!(matches!(result, Err(ReaderError::Parse(_))));
    }

    #[test]
    fn build_sheet_rejects_sheet_exceeding_max_cells() {
        let mut ws = umya_spreadsheet::Worksheet::default();
        ws.cell_mut((100u32, 100u32)).set_value("x");
        let result = build_sheet(&ws, 50);
        assert!(matches!(
            result,
            Err(ReaderError::SheetTooLarge {
                rows: 100,
                cols: 100,
                limit: 50,
                ..
            })
        ));
    }

    #[test]
    fn build_sheet_on_empty_sheet_uses_one_column() {
        let ws = umya_spreadsheet::Worksheet::default();
        let sheet = build_sheet(&ws, 1_000_000).unwrap();
        assert_eq!(sheet.cells.rows(), 0);
        assert_eq!(sheet.cells.cols(), 1);
    }
}
