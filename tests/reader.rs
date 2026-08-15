//! umya-spreadsheetの実際のwriter/readerを介した往復（ZIP展開・XMLパースを含む）で
//! `extmd::reader::read_sheets`を検証する結合テスト。domain/reader配下の単体テストは
//! 主に`Worksheet::default()`等のin-memory構築を対象にしているため、実ファイルI/Oを
//! 経由した経路もあわせて確認する。

use extmd::domain::CellValue;
use extmd::reader::{read_sheets, ReaderError};

fn temp_xlsx_path(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "extmd-test-{name}-{nanos}-{}.xlsx",
        std::process::id()
    ))
}

#[test]
fn read_sheets_round_trips_a_written_workbook() {
    let path = temp_xlsx_path("round-trip");

    let mut book = umya_spreadsheet::new_file();
    {
        let sheet = book.sheet_by_name_mut("Sheet1").unwrap();
        sheet.cell_mut("A1").set_value("hello");
        sheet.cell_mut("B1").set_value_number(42);
        // 結合セルの範囲内に値を置く。`highest_column_and_row()`はumya-spreadsheet内部で
        // 値の存在するセルのみから算出され、結合セル自体の範囲は考慮しないため
        // （`Cells::highest_column_and_row`実装より）、値のない行への結合だけでは
        // Gridの行数が広がらず、`validation::is_within_bounds`が範囲外として除外してしまう
        // （`read_sheets_discards_merge_extending_into_wholly_empty_row`で別途確認）。
        sheet.cell_mut("A2").set_value("merged");
        sheet.add_merge_cells("A2:B2");
    }
    umya_spreadsheet::writer::xlsx::write(&book, &path).unwrap();

    let sheets = read_sheets(&path, 1_000_000).unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(sheets.len(), 1);
    let sheet = &sheets[0];
    assert_eq!(sheet.name, "Sheet1");
    assert_eq!(
        sheet.cells.get(0, 0).unwrap().value,
        CellValue::String("hello".to_string())
    );
    assert_eq!(
        sheet.cells.get(0, 1).unwrap().value,
        CellValue::Number(42.0)
    );
    assert_eq!(sheet.merges.len(), 1);
    assert_eq!(sheet.merges[0].row_start, 1);
    assert_eq!(sheet.merges[0].col_end, 1);
}

/// docs/design/reader/validation.md 4章「未確定事項」の検証: `is_within_bounds`が
/// 「範囲内だが元々存在しないセル座標」を通してしまうケースはないか、という問いに対する
/// 実データでの確認。値を一切持たない行にだけ結合セルを設定した場合、
/// `highest_column_and_row()`はその行を認識しない（値のあるセルのみから算出されるため）。
/// そのため`Grid`はその行を含まず、結合セル情報はGrid範囲外として正しく破棄される
/// （`Sheet.merges`は`cells`の範囲内に収まるという不変条件に従った、意図した挙動）。
#[test]
fn read_sheets_discards_merge_extending_into_wholly_empty_row() {
    let path = temp_xlsx_path("merge-into-empty-row");

    let mut book = umya_spreadsheet::new_file();
    {
        let sheet = book.sheet_by_name_mut("Sheet1").unwrap();
        sheet.cell_mut("A1").set_value("hello");
        // A2:B2には一切値を書き込まず、結合情報だけを設定する。
        sheet.add_merge_cells("A2:B2");
    }
    umya_spreadsheet::writer::xlsx::write(&book, &path).unwrap();

    let sheets = read_sheets(&path, 1_000_000).unwrap();
    std::fs::remove_file(&path).ok();

    let sheet = &sheets[0];
    assert_eq!(sheet.cells.rows(), 1, "Gridは値のある1行分しか持たない");
    assert!(sheet.merges.is_empty(), "Grid範囲外の結合セルは破棄される");
}

#[test]
fn read_sheets_reports_parse_error_for_non_xlsx_file() {
    let path = temp_xlsx_path("not-an-xlsx");
    std::fs::write(&path, b"this is not a zip/xlsx file").unwrap();

    let result = read_sheets(&path, 1_000_000);
    std::fs::remove_file(&path).ok();

    assert!(matches!(result, Err(ReaderError::Parse(_))));
}

#[test]
fn read_sheets_rejects_sheet_exceeding_max_cells() {
    let path = temp_xlsx_path("too-large");

    let mut book = umya_spreadsheet::new_file();
    {
        let sheet = book.sheet_by_name_mut("Sheet1").unwrap();
        // 実データは1セルのみだが、座標(200, 200)への書き込みでhighest_column_and_row
        // が(200, 200)相当まで押し上げられる。
        sheet.cell_mut((200u32, 200u32)).set_value("x");
    }
    umya_spreadsheet::writer::xlsx::write(&book, &path).unwrap();

    let result = read_sheets(&path, 1_000);
    std::fs::remove_file(&path).ok();

    assert!(matches!(result, Err(ReaderError::SheetTooLarge { .. })));
}
