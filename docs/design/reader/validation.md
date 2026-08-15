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
        .filter(|m| is_ordered(m) && is_within_bounds(m, rows, cols))
        .collect()
}

fn to_domain_range(range: &umya_spreadsheet::Range) -> Option<domain::MergeRange> {
    // coordinate_start_row()等は`Option<&RowReference>`/`Option<&ColumnReference>`を返す
    // （u32を直接返すわけではない）。実際の数値は`RowReference::num()`/
    // `ColumnReference::num()`で取得する。umya-spreadsheetは1-based座標のため、
    // 0-based(Gridの座標系)へ変換するには-1する。
    Some(domain::MergeRange {
        row_start: range.coordinate_start_row()?.num().checked_sub(1)? as usize,
        row_end: range.coordinate_end_row()?.num().checked_sub(1)? as usize,
        col_start: range.coordinate_start_col()?.num().checked_sub(1)? as usize,
        col_end: range.coordinate_end_col()?.num().checked_sub(1)? as usize,
    })
}

/// `row_start <= row_end`かつ`col_start <= col_end`か。umya-spreadsheetの
/// `Range::set_range`はXMLの`mergeCell`要素の`ref`属性（例: `"A1:B2"`）を`:`で
/// 分割し、前半をそのまま開始座標、後半をそのまま終了座標として採用するだけで、
/// 大小関係の正規化は行わない。そのため破損した/悪意あるxlsxが`ref="B2:A1"`の
/// ように開始・終了を逆転させた値を埋め込んだ場合、そのまま`row_start > row_end`
/// （または`col_start > col_end`）の`MergeRange`が生成されてしまう。これを後段
/// （`analysis`層が`Block`を構築する際等）まで伝播させると`usize`アンダーフローの
/// リスクになるため、Reader側で明示的に検証し破棄する（PR #20レビュー指摘を反映）。
fn is_ordered(m: &domain::MergeRange) -> bool {
    m.row_start <= m.row_end && m.col_start <= m.col_end
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
ロギングクレートは`log`を採用する（[cli.md 4章](../cli.md#4-ロギング方針)参照）。実装では
`validation.rs`側は`log::warn!`の呼び出しのみを持ち、実際にログを出力するかどうかの
subscriber初期化（`env_logger`、ログレベルの`-v`切り替え等）は`cli.rs`/`main.rs`側の責務とする
（subscriber未初期化の状態では`log`マクロの呼び出しは何もしないため、`reader`単体で
先行して`log::warn!`を組み込んでも問題ない）。

## 4. 未確定事項（実装フェーズで解決済みの項目を含む）

- ~~ロギングクレートの選定~~ → `log`クレートを採用（上記の通り解決）。
- ~~`is_within_bounds` が偶然「範囲内だが元々存在しないセル座標」を通してしまうケースが
  ないか~~ → 実データ（umya-spreadsheetの実ファイルI/O経由）で検証済み。
  `highest_column_and_row()`は値の存在するセルのみから算出され、結合セル自体の範囲は
  考慮しないため、値を一切持たない行/列にのみ結合セルが設定されているケースでは
  `Grid`がその行/列を含まず、`is_within_bounds`が正しく範囲外として除外することを確認した
  （`tests/reader.rs`の`read_sheets_discards_merge_extending_into_wholly_empty_row`）。
  これは`Sheet.merges`は`cells`の範囲内に収まるという不変条件（[sheet.md 3章](../domain/sheet.md#3-不変条件-merges-は-cells-の範囲内に収まる)）
  に従った意図した挙動であり、「本来存在するはずの座標が誤って除外される」バグではない。
