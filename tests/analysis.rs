//! `tests/fixtures/*.xlsx` を用いたfixtureテスト。実在する`.xlsx`ファイルを
//! `reader::read_sheets`で読み込み、`analysis::StrategyRegistry`/`analysis::analyze`に
//! 通すことで、列幅・はみ出しテキスト・行パターン・ネイティブ結合セルといったExcel由来の
//! 特徴量が意図通りに反映され、正しい戦略・ブロック構造になることをエンドツーエンドで
//! 検証する（docs/design/analysis/metrics.md 5章・docs/design/architecture.md 6.1.5節）。
//!
//! フィクスチャは`examples/gen_fixtures.rs`（`cargo run --example gen_fixtures`）で
//! 生成したものを`tests/fixtures/`にコミットしている。内容を変更したい場合は
//! `examples/gen_fixtures.rs`を編集し再生成すること。
//!
//! - `grid_paper.xlsx`: 方眼紙的な文書（狭い均一列幅・はみ出しテキスト・不規則な行パターン）
//! - `tabular.xlsx`: 通常の集計表（広い均一列幅・隙間なく埋まった規則的な行パターン）
//! - `application_form.xlsx`: 申請書のような業務フォーマット（ネイティブ結合セルを持つ
//!   方眼紙的文書。architecture.md 5.3の業務ドメイン特化戦略が想定するレイアウトのv1相当）
//! - `meeting_minutes.xlsx`: 議事録のような業務フォーマット。ラベル+入力欄の1行結合に加え、
//!   行・列の両方にまたがる結合セル（予定表内の2コマ分の枠）を持ち、
//!   `MergeRange::is_single_row_or_column()`が`false`になる表形式が方眼紙文書の中に
//!   埋め込まれたパターンを検証する
//! - `mixed_workbook.xlsx`: 1ワークブックに方眼紙シートと通常表シートが混在するケース
//!   （要件定義書8章 #2、architecture.md 6章）

use extmd::analysis::{analyze, StrategyRegistry};
use extmd::domain::BlockSource;
use extmd::reader::read_sheets;

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn read_single_sheet(fixture: &str) -> extmd::domain::Sheet {
    let sheets = read_sheets(&fixture_path(fixture), 1_000_000).unwrap();
    assert_eq!(
        sheets.len(),
        1,
        "fixture '{fixture}' should contain exactly one sheet"
    );
    sheets.into_iter().next().unwrap()
}

#[test]
fn select_auto_picks_grid_paper_for_the_grid_paper_fixture() {
    let sheet = read_single_sheet("grid_paper.xlsx");
    let registry = StrategyRegistry::with_defaults();
    assert_eq!(registry.select_auto(&sheet).id(), "grid-paper");
}

#[test]
fn select_auto_picks_tabular_for_the_tabular_fixture() {
    let sheet = read_single_sheet("tabular.xlsx");
    let registry = StrategyRegistry::with_defaults();
    assert_eq!(registry.select_auto(&sheet).id(), "tabular");
}

#[test]
fn select_auto_picks_grid_paper_for_the_application_form_fixture() {
    // ネイティブ結合セルを持つ申請書レイアウトも、狭い列幅・不規則な行パターンの
    // 方眼紙的特徴を備えていればgrid-paperと判定される（v1は業務ドメイン特化戦略を
    // 持たないため、grid-paperがこのレイアウトのデフォルトの扱いとなる）。
    //
    // このフィクスチャは実際にはtabular側のaffinityがgrid-paperをわずかに上回る
    // （2列×5行の規則的なブロック構造を持つため）が、その差が
    // `StrategyConfig::affinity_fallback_margin`（既定0.05）未満の僅差であるため
    // フォールバックでgrid-paperが選ばれる。architecture.md 6.1.5節が求める
    // 「方眼紙と通常表の中間的な境界サンプル」による僅差フォールバックの検証を、
    // 実データに近い形で兼ねている。
    let sheet = read_single_sheet("application_form.xlsx");
    let registry = StrategyRegistry::with_defaults();
    assert_eq!(registry.select_auto(&sheet).id(), "grid-paper");
}

#[test]
fn select_auto_picks_grid_paper_for_the_meeting_minutes_fixture() {
    let sheet = read_single_sheet("meeting_minutes.xlsx");
    let registry = StrategyRegistry::with_defaults();
    assert_eq!(registry.select_auto(&sheet).id(), "grid-paper");
}

#[test]
fn select_auto_distinguishes_sheets_within_the_mixed_workbook_fixture() {
    let sheets = read_sheets(&fixture_path("mixed_workbook.xlsx"), 1_000_000).unwrap();
    assert_eq!(sheets.len(), 2);

    let registry = StrategyRegistry::with_defaults();
    let grid_paper_sheet = sheets.iter().find(|s| s.name == "方眼紙シート").unwrap();
    let tabular_sheet = sheets.iter().find(|s| s.name == "通常表シート").unwrap();

    assert_eq!(registry.select_auto(grid_paper_sheet).id(), "grid-paper");
    assert_eq!(registry.select_auto(tabular_sheet).id(), "tabular");
}

#[test]
fn analyze_resolves_native_merges_in_the_application_form_fixture() {
    let sheet = read_single_sheet("application_form.xlsx");
    let registry = StrategyRegistry::with_defaults();
    let strategy = registry.select_auto(&sheet);
    let doc = analyze(&sheet, strategy);

    // 行1: "出張申請書"はA1:D1のネイティブ結合。タイトル行は単一の大きな見出しブロックに
    // なり、はみ出し・結合の有無によらずFlow行として扱われる。
    let title_row = &doc.rows[0];
    assert_eq!(title_row.kind, extmd::domain::RowKind::Flow);
    assert_eq!(title_row.blocks.len(), 1);
    let title_block = &title_row.blocks[0];
    assert_eq!(title_block.text, "出張申請書");
    assert_eq!(title_block.source, BlockSource::NativeMerge);
    assert_eq!(title_block.col_start, 0);
    assert_eq!(title_block.col_end, 3);
    assert!(
        title_block.heading_level.is_some(),
        "18pt bold title should be detected as a heading"
    );

    // 行2: "氏名"(単独セルA2) + "山田太郎"(B2:D2のネイティブ結合)の2ブロック。
    let name_row = &doc.rows[1];
    assert_eq!(name_row.blocks.len(), 2);
    assert_eq!(name_row.blocks[0].text, "氏名");
    assert_eq!(name_row.blocks[0].source, BlockSource::Single);
    assert_eq!(name_row.blocks[1].text, "山田太郎");
    assert_eq!(name_row.blocks[1].source, BlockSource::NativeMerge);
    assert_eq!(name_row.blocks[1].col_start, 1);
    assert_eq!(name_row.blocks[1].col_end, 3);
}

#[test]
fn analyze_resolves_a_row_and_column_spanning_merge_in_the_meeting_minutes_fixture() {
    let sheet = read_single_sheet("meeting_minutes.xlsx");
    let registry = StrategyRegistry::with_defaults();
    let strategy = registry.select_auto(&sheet);
    let doc = analyze(&sheet, strategy);

    // 行5(0-indexで4): "資料説明"はB5:C6(2行×2列)のネイティブ結合。結合範囲の左上セルが
    // あるこの行では、A5"14:00" / B5:C6"資料説明"(結合) / D5"田中"の3ブロックになる。
    let session_row = &doc.rows[4];
    assert_eq!(session_row.blocks.len(), 3);
    assert_eq!(session_row.blocks[0].text, "14:00");
    assert_eq!(session_row.blocks[0].source, BlockSource::Single);
    assert_eq!(session_row.blocks[1].text, "資料説明");
    assert_eq!(session_row.blocks[1].source, BlockSource::NativeMerge);
    assert_eq!(session_row.blocks[1].col_start, 1);
    assert_eq!(session_row.blocks[1].col_end, 2);
    assert_eq!(session_row.blocks[2].text, "田中");

    // 行6(0-indexで5): 結合範囲の2行目はネイティブ結合の左上セルではなく、かつグリッド上は
    // 空セル(CellValue::Empty)として表現されるため、B/C列にはブロックが生成されない
    // （table.mdが明記する「縦方向の結合(rowspan相当)はv1のスコープ外」という制約通り、
    // 2行目分の空間はブロックなしの空白として扱われる）。A6"15:00" / D6"鈴木"の2ブロックのみ。
    let continuation_row = &doc.rows[5];
    assert_eq!(continuation_row.blocks.len(), 2);
    assert_eq!(continuation_row.blocks[0].text, "15:00");
    assert_eq!(continuation_row.blocks[1].text, "鈴木");
}
