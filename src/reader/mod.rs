//! 設計方針・モジュール構成・`ReaderError`・公開API（docs/design/reader/mod.md）。

mod cell_mapper;
mod date;
mod grid_builder;
mod validation;
mod xlsx;

use crate::domain;

#[derive(Debug)]
pub enum ReaderError {
    /// ファイルが存在しない、権限がない等のI/Oエラー。
    Io(std::io::Error),
    /// xlsxとして不正な形式・破損したファイル（umya-spreadsheetのパースエラーをラップ）。
    Parse(String),
    /// シートの`rows * cols`が`max_cells`を超過した（4.1節参照）。悪意ある/破損した
    /// ファイルが座標だけを巨大な値に細工しているケースを含め、疎な巨大シートによる
    /// メモリ枯渇(DoS)を未然に防ぐための拒否。
    SheetTooLarge {
        name: String,
        rows: usize,
        cols: usize,
        limit: usize,
    },
}

impl std::fmt::Display for ReaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReaderError::Io(e) => write!(f, "{e}"),
            ReaderError::Parse(msg) => write!(f, "{msg}"),
            ReaderError::SheetTooLarge {
                name,
                rows,
                cols,
                limit,
            } => write!(
                f,
                "Sheet '{name}' has {rows} x {cols} cells, which exceeds the limit of {limit} cells"
            ),
        }
    }
}

impl std::error::Error for ReaderError {}

/// 指定した`.xlsx`ファイルの全シートを読み込み、`domain::Sheet`の列へ変換する。
/// シートの絞り込み（要件定義書5.1 `-s`/`--sheet`）は呼び出し側（`lib.rs`）の責務とし、
/// `reader`は常に全シートを返す。
///
/// `max_cells`は1シートあたりの`rows * cols`の上限（CLIの`--max-cells`）。超過した
/// シートが1つでもあれば`ReaderError::SheetTooLarge`を返し、処理全体を打ち切る。
pub fn read_sheets(
    path: &std::path::Path,
    max_cells: usize,
) -> Result<Vec<domain::Sheet>, ReaderError> {
    xlsx::read_sheets(path, max_cells)
}
