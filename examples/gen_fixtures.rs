//! `tests/fixtures/*.xlsx` を生成する使い捨てスクリプト。
//! `cargo run --example gen_fixtures` で実行し、`tests/fixtures/` 配下に書き出す。
//! フィクスチャの内容を変更したくなったらこのファイルを一時的に復元して再生成する。

use std::path::Path;
use umya_spreadsheet::{HorizontalAlignmentValues, Worksheet};

fn set_column_widths(ws: &mut Worksheet, cols: u32, width: f64) {
    for col in 1..=cols {
        ws.column_dimension_by_number_mut(col).set_width(width);
    }
}

fn heading(ws: &mut Worksheet, coord: &str, text: &str, size: f64) {
    let cell = ws.cell_mut(coord);
    cell.set_value(text);
    cell.style_mut().font_mut().set_size(size).set_bold(true);
}

/// Excel方眼紙的な文書: 列幅が狭く均一、長いテキストが右隣の空セルへはみ出す行が多く、
/// 行ごとに非空列の位置がバラバラ（タイトル行・記入欄などレイアウトが異なる）。
fn build_grid_paper(book: &mut umya_spreadsheet::Workbook) {
    let ws = book.sheet_by_name_mut("Sheet1").unwrap();
    ws.set_name("方眼紙シート");
    set_column_widths(ws, 4, 2.5);

    heading(ws, "A1", "サンプル方眼紙文書のタイトル見出しです", 16.0);
    ws.cell_mut("A2").set_value("氏名");
    ws.cell_mut("C2").set_value("部署");
    ws.cell_mut("A3")
        .set_value("連絡先メールアドレスの記入欄です");
    ws.cell_mut("B4")
        .set_value("備考欄はこちらに記入してください");
    ws.cell_mut("A5").set_value("日付");
}

/// 通常の集計表: 列幅が広く均一、ヘッダー行 + データ行が同じ列パターンで隙間なく埋まる。
fn build_tabular(book: &mut umya_spreadsheet::Workbook) {
    let ws = book.sheet_by_name_mut("Sheet1").unwrap();
    ws.set_name("通常表シート");
    set_column_widths(ws, 4, 12.0);

    let header = ["ID", "氏名", "部署", "金額"];
    for (col, text) in header.iter().enumerate() {
        let cell = ws.cell_mut(((col + 1) as u32, 1u32));
        cell.set_value(*text);
        cell.style_mut().font_mut().set_bold(true);
        cell.style_mut()
            .alignment_mut()
            .set_horizontal(HorizontalAlignmentValues::Center);
    }

    let rows = [
        ["1", "山田太郎", "営業部", "1000"],
        ["2", "佐藤花子", "経理部", "2000"],
        ["3", "鈴木一郎", "開発部", "3000"],
        ["4", "高橋次郎", "総務部", "4000"],
    ];
    for (r, row) in rows.iter().enumerate() {
        for (c, text) in row.iter().enumerate() {
            ws.cell_mut(((c + 1) as u32, (r + 2) as u32))
                .set_value(*text);
        }
    }
}

/// 申請書・議事録のような業務フォーマット: ネイティブ結合セル（タイトル行の横結合、
/// ラベル+記入欄の横結合）を持つ方眼紙的文書。architecture.md 5.3の業務ドメイン特化
/// 戦略が想定するレイアウトのv1相当（grid-paper戦略が適用される）。
fn build_application_form(book: &mut umya_spreadsheet::Workbook) {
    let ws = book.sheet_by_name_mut("Sheet1").unwrap();
    ws.set_name("申請書シート");
    set_column_widths(ws, 4, 3.0);

    heading(ws, "A1", "出張申請書", 18.0);
    ws.add_merge_cells("A1:D1");

    ws.cell_mut("A2").set_value("氏名");
    ws.cell_mut("B2").set_value("山田太郎");
    ws.add_merge_cells("B2:D2");

    ws.cell_mut("A3").set_value("部署");
    ws.cell_mut("B3").set_value("営業部");
    ws.add_merge_cells("B3:D3");

    ws.cell_mut("A4").set_value("申請理由");
    ws.cell_mut("B4")
        .set_value("顧客訪問のため出張を申請します。詳細は別紙の通りです。");
    ws.add_merge_cells("B4:D4");

    ws.cell_mut("A5").set_value("日付");
    ws.cell_mut("B5").set_value("2026-08-16");

    // 実際のExcelは結合範囲内の全セル（左上以外も含む）にセルエントリを書き出すため、
    // C/D列にも（値を設定せず）触れておき、highest_column_and_row()が結合の右端まで
    // 認識するようにする。umya-spreadsheetのxlsx writerは値・スタイルの両方が空の
    // セルをファイルへ書き出さないため（Cell::write_toの`empty_flag_value &&
    // empty_flag_style`分岐）、スタイルだけ明示的に触れて空セルとして永続化させる。
    for row in 1..=5u32 {
        ws.cell_mut((3u32, row)).style_mut().alignment_mut();
        ws.cell_mut((4u32, row)).style_mut().alignment_mut();
    }
}

/// 議事録のような業務フォーマット: ラベル+入力欄（1行×複数列の結合、application_formと同種）
/// に加えて、行・列の両方にまたがる結合セル（`MergeRange::is_single_row_or_column()`が
/// `false`になる、表形式の予定表を模した2次元結合）を持つ方眼紙的文書。
/// v1のAnalyzerが縦方向の結合（rowspan相当）をどう扱うか（table.md 3章: 結合範囲の
/// 2行目以降は空セルとして扱われ、ブロックを生成しない）を実データに近い形で検証する。
fn build_meeting_minutes(book: &mut umya_spreadsheet::Workbook) {
    let ws = book.sheet_by_name_mut("Sheet1").unwrap();
    ws.set_name("議事録シート");
    set_column_widths(ws, 4, 3.0);

    heading(ws, "A1", "第3回定例会議 議事録", 16.0);
    ws.add_merge_cells("A1:D1");

    ws.cell_mut("A2").set_value("日時");
    ws.cell_mut("B2").set_value("2026-08-20 14:00-15:00");
    ws.add_merge_cells("B2:D2");

    ws.cell_mut("A3").set_value("場所");
    ws.cell_mut("B3").set_value("会議室A");
    ws.add_merge_cells("B3:D3");

    // 予定表(表形式)のヘッダー行。
    ws.cell_mut("A4").set_value("時間");
    ws.cell_mut("B4").set_value("内容");
    ws.cell_mut("D4").set_value("担当");

    // B5:C6 は2行×2列にまたがる結合（is_single_row_or_column() == false）。
    // 「資料説明」という1つのセッションが2つの時間枠(14:00/15:00)を占めることを表す。
    ws.cell_mut("A5").set_value("14:00");
    ws.cell_mut("B5").set_value("資料説明");
    ws.add_merge_cells("B5:C6");
    ws.cell_mut("D5").set_value("田中");

    ws.cell_mut("A6").set_value("15:00");
    ws.cell_mut("D6").set_value("鈴木");

    for row in 1..=6u32 {
        ws.cell_mut((3u32, row)).style_mut().alignment_mut();
        ws.cell_mut((4u32, row)).style_mut().alignment_mut();
    }
}

/// 1ワークブック内に方眼紙シートと通常表シートが混在するケース
/// （要件定義書8章 #2、architecture.md 6章）。
fn build_mixed_workbook() -> umya_spreadsheet::Workbook {
    let mut book = umya_spreadsheet::new_file();
    build_grid_paper(&mut book);
    {
        let ws = book.new_sheet("通常表シート").unwrap();
        set_column_widths(ws, 4, 12.0);
        let header = ["ID", "氏名", "部署", "金額"];
        for (col, text) in header.iter().enumerate() {
            ws.cell_mut(((col + 1) as u32, 1u32)).set_value(*text);
        }
        let rows = [
            ["1", "山田太郎", "営業部", "1000"],
            ["2", "佐藤花子", "経理部", "2000"],
            ["3", "鈴木一郎", "開発部", "3000"],
        ];
        for (r, row) in rows.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                ws.cell_mut(((c + 1) as u32, (r + 2) as u32))
                    .set_value(*text);
            }
        }
    }
    book
}

fn write(book: &umya_spreadsheet::Workbook, dir: &Path, name: &str) {
    let path = dir.join(name);
    umya_spreadsheet::writer::xlsx::write(book, &path).unwrap();
    println!("wrote {}", path.display());
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::create_dir_all(&dir).unwrap();

    let mut grid_paper = umya_spreadsheet::new_file();
    build_grid_paper(&mut grid_paper);
    write(&grid_paper, &dir, "grid_paper.xlsx");

    let mut tabular = umya_spreadsheet::new_file();
    build_tabular(&mut tabular);
    write(&tabular, &dir, "tabular.xlsx");

    let mut application_form = umya_spreadsheet::new_file();
    build_application_form(&mut application_form);
    write(&application_form, &dir, "application_form.xlsx");

    let mut meeting_minutes = umya_spreadsheet::new_file();
    build_meeting_minutes(&mut meeting_minutes);
    write(&meeting_minutes, &dir, "meeting_minutes.xlsx");

    let mixed = build_mixed_workbook();
    write(&mixed, &dir, "mixed_workbook.xlsx");
}
