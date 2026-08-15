# `reader::validation` 設計書

対象: [reader/mod.md](mod.md)の対応表における `validation.rs`。

## 1. 責務

Excelから取得した結合セル範囲（`umya_spreadsheet::Range`）を
`domain::MergeRange`（[sheet.md 1章](../domain/sheet.md#1-mergerange)）へ変換し、
[grid_builder.rs](grid_builder.md)が構築した `Grid` の範囲外にある不正な結合情報を
破棄（無視）する。[sheet.md 3章](../domain/sheet.md#3-不変条件-merges-は-cells-の範囲内に収まる)が
定める「`Sheet.merges`は`cells`の範囲内に収まる」という不変条件を、Reader側で
実際に担保する箇所がこのファイルである。

## 2. 変換・検証アルゴリズム

```rust
pub(crate) fn collect_valid_merges(
    ws: &umya_spreadsheet::Worksheet,
    rows: usize,
    cols: usize,
) -> Vec<domain::MergeRange> {
    ws.merge_cells()
        .iter()
        .filter_map(|range| to_domain_range(range))
        .filter(|m| is_within_bounds(m, rows, cols))
        .collect()
}

fn to_domain_range(range: &umya_spreadsheet::Range) -> Option<domain::MergeRange> {
    // umya-spreadsheetは1-based座標。0-based(Gridの座標系)へ変換する。
    Some(domain::MergeRange {
        row_start: range.coordinate_start_row()?.checked_sub(1)? as usize,
        row_end: range.coordinate_end_row()?.checked_sub(1)? as usize,
        col_start: range.coordinate_start_col()?.checked_sub(1)? as usize,
        col_end: range.coordinate_end_col()?.checked_sub(1)? as usize,
    })
}

fn is_within_bounds(m: &domain::MergeRange, rows: usize, cols: usize) -> bool {
    m.row_end < rows && m.col_end < cols
}
```

`coordinate_start_row()` 等が `None`（座標情報が欠落した不正な `Range`）を返す場合も
`filter_map` によって黙って除外する（3章参照）。

## 3. 破棄（無視）の方針とログ出力

範囲外・座標欠落のいずれの理由であっても、該当する結合セル情報は**エラーにせず
黙って除外**し、変換処理自体は継続する（Excelファイルのメタデータ破損はまれに
実データで起こりうるため、1つの不正な結合セル情報のためにファイル全体の変換を
失敗させるべきではないと判断）。

`-v`/`--verbose`（[要件定義書 5.1](../../requirement/requirements.md#51-cli仕様案)）指定時は、
除外した `MergeRange` ごとに警告ログを出力する
（[Issue #4のコメント](https://github.com/MinamiyamaKotaro/extmd/issues/4#issuecomment-5301613143)の提案を反映）。
ログ出力の具体的な仕組み（`log`/`tracing`クレートの選定等）は `reader` 単体の設計スコープ外とし、
CLI全体のロギング方針（`cli.rs`/`main.rs`の設計時）で決定する。

## 4. 未確定事項

- ロギングクレートの選定（3章、CLI全体の設計と合わせて決定）
- `is_within_bounds` が偶然「範囲内だが元々存在しないセル座標」を通してしまうケースがないか
  （`row_end < rows && col_end < cols` は矩形Gridの範囲チェックとして必要十分なはずだが、
  実データでの検証は未実施）
