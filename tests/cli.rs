//! コンパイル済みバイナリ(`extmd`)を実際に起動し、CLI全体のパイプライン
//! (引数パース → convert() → 標準出力/ファイル書き込み)をエンドツーエンドで検証する。

use std::process::Command;

fn temp_path(name: &str, ext: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "extmd-cli-test-{name}-{nanos}-{}.{ext}",
        std::process::id()
    ))
}

fn write_minimal_workbook(path: &std::path::Path, value: &str) {
    let mut book = umya_spreadsheet::new_file();
    book.sheet_by_name_mut("Sheet1")
        .unwrap()
        .cell_mut("A1")
        .set_value(value);
    umya_spreadsheet::writer::xlsx::write(&book, path).unwrap();
}

#[test]
fn cli_converts_workbook_to_stdout() {
    let input = temp_path("stdout", "xlsx");
    write_minimal_workbook(&input, "hello world");

    let output = Command::new(env!("CARGO_BIN_EXE_extmd"))
        .arg(&input)
        .output()
        .expect("failed to run extmd binary");

    std::fs::remove_file(&input).ok();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Sheet1"));
    assert!(stdout.contains("hello world"));
}

#[test]
fn cli_writes_to_output_file() {
    let input = temp_path("outfile-in", "xlsx");
    let out_path = temp_path("outfile-out", "md");
    write_minimal_workbook(&input, "hello");

    let status = Command::new(env!("CARGO_BIN_EXE_extmd"))
        .arg(&input)
        .arg("-o")
        .arg(&out_path)
        .status()
        .expect("failed to run extmd binary");

    assert!(status.success());
    let body = std::fs::read_to_string(&out_path).unwrap();
    assert!(body.contains("hello"));

    std::fs::remove_file(&input).ok();
    std::fs::remove_file(&out_path).ok();
}

#[test]
fn cli_exits_with_error_for_missing_input_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_extmd"))
        .arg("/nonexistent/does-not-exist.xlsx")
        .output()
        .expect("failed to run extmd binary");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Input file not found"));
}

#[test]
fn cli_rejects_conflicting_strategy_and_no_overflow_merge_flags() {
    let input = temp_path("conflict", "xlsx");
    write_minimal_workbook(&input, "hello");

    let output = Command::new(env!("CARGO_BIN_EXE_extmd"))
        .arg(&input)
        .arg("--strategy")
        .arg("tabular")
        .arg("--no-overflow-merge")
        .output()
        .expect("failed to run extmd binary");

    std::fs::remove_file(&input).ok();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn cli_split_mode_writes_one_file_per_sheet_into_directory() {
    let input = temp_path("split-in", "xlsx");
    write_minimal_workbook(&input, "hello");
    let dir = temp_path("split-out", "dir");

    let status = Command::new(env!("CARGO_BIN_EXE_extmd"))
        .arg(&input)
        .arg("--split")
        .arg("-o")
        .arg(&dir)
        .status()
        .expect("failed to run extmd binary");

    assert!(status.success());
    let body = std::fs::read_to_string(dir.join("Sheet1.md")).unwrap();
    assert!(body.contains("hello"));

    std::fs::remove_file(&input).ok();
    std::fs::remove_dir_all(&dir).ok();
}
