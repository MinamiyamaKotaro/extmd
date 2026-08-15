# `reader::date` 設計書

対象: [reader/mod.md](mod.md)の対応表における `date.rs`。

## 1. 責務

Excelのシリアル値（`f64`）から `chrono::NaiveDateTime`（[cell.md 1章](../domain/cell.md#1-cellvalue)の
`CellValue::Date`）への変換、および[cell_mapper.rs](cell_mapper.md)から呼ばれる
「このセルは日付として扱うべきか」の判定を担う。umya-spreadsheetの `Cell`/`Worksheet`型には
依存せず、`f64` と表示形式コード文字列（`&str`）だけを引数に取る純粋関数として実装し、
Excelファイルを読み込まずに単体テスト可能にする。

## 2. シリアル値からの変換

`umya-spreadsheet` は `umya_spreadsheet::helper::date` モジュールに変換ヘルパーを
公式に提供しており、その中の **`excel_to_date_time_chrono`** がシリアル値を直接
`chrono::NaiveDateTime` に変換する。

```rust
pub(crate) fn from_serial(serial: f64) -> chrono::NaiveDateTime {
    umya_spreadsheet::helper::date::excel_to_date_time_chrono(serial)
}
```

カレンダーシステムは umya-spreadsheet 側で Windows 1900 基準（1900年うるう年バグの
互換込み）が既定であり、`.xlsx`（Excel 2007以降）はほぼ全てこの基準を使うため、
Mac 1904 基準への切り替えは v1 では扱わない（発生した場合は
[6章](#6-未確定事項)の未確定事項とする）。

## 3. 日付セルの判定: `is_date_formatted`

[cell_mapper.md 4章](cell_mapper.md#4-日付判定-is_date_formatted)で述べた通り、
umya-spreadsheetの `CellRawValue` は日付専用の型を持たず、数値セルはすべて
`Numeric(f64)` になる。そのため、セルが「日付として表示されるべき数値」かどうかは
**表示形式コード（`number_format`）のパターンで判定する**。

```rust
pub(crate) fn is_date_formatted(format_code: &str) -> bool {
    // Excelの組み込み日付/時刻フォーマットID(14〜22, 45〜47)相当のコード、
    // および `y`/`m`/`d`/`h`/`s` を含むカスタム書式コードを日付とみなす。
    // 詳細なパターン一覧は実装時に確定する（4章参照）。
    matches!(format_code, "General") == false && contains_date_token(format_code)
}
```

判定を「値の型」ではなく「表示形式の文字列パターン」に頼る設計は、Excel自体が
日付をこの方式（シリアル値+書式）でしか表現しないことに起因する構造的な制約であり、
`calamine` の `chrono` フィーチャーが自動的に `DataType::DateTime` を返すのとは
異なるアプローチになる（[cell.md](../domain/cell.md)が前提としていた挙動との違いは
[5章](#5-cellmdのchrono採用根拠との整合)を参照）。

## 4. 書式コードのパターン判定（未確定事項）

Excelの組み込み日付/時刻書式（`numFmtId` 14〜22、45〜47）は固定のコード文字列に
対応するが、ユーザーがカスタム書式（例: `"yyyy年m月d日"`, `"[$-409]h:mm AM/PM"`）を
設定しているケースもある。完全なパーサを自前実装するとスコープが肥大化するため、
v1では次の優先度で対応する:

| 優先度 | 対象 | 方針 |
|---|---|---|
| 必須 | 組み込み日付/時刻フォーマットID相当のコード | 固定のコード文字列リストと照合 |
| 検証待ち | `y`/`m`/`d`/`h`/`s` 等の日付トークンを含むカスタム書式 | 簡易的なトークン検出（[cell.md 4章](../domain/cell.md#4-format_number--format_date-のフォーマット方針)の`ssfmt`採用可否と合わせて実装フェーズで検証） |
| 対象外 | 日付トークンを含まない書式なのに実質日付として使われているセル（誤検出源） | 検出不能として`Number`扱いにする |

この判定ロジックが `ssfmt`（採用候補、[cell.md 4章](../domain/cell.md#4-format_number--format_date-のフォーマット方針)）の
書式パーサと重複する可能性があるため、`ssfmt`採用が決まった場合はその書式コード解析結果を
流用できないか実装フェーズで検討する。

**誤検出（偽陽性）のリスク:** Excelのカスタム書式は、ダブルクォートで囲んだリテラル文字列
（例: `0"m"` = 数値の後ろに単位「m」を付けるだけの書式）や、バックスラッシュエスケープ
（例: `0\m\d`）を含むことができる。素朴に書式コード文字列中に `y`/`m`/`d` 等の文字が
含まれるかどうかだけを見ると、これらの「日付トークンに見えるだけの非日付書式」を
誤って日付と判定してしまう。トークン検出を実装する際は、リテラル文字列区間
（`"..."` の内側）とエスケープされた1文字（`\` の直後の1文字）を判定対象から
除外する必要がある。

さらに実装時に、**角括弧区間（`[...]`）** も同様の誤検出源であることが判明した。
Excelの表示形式は色指定（`[Red]0.00`）やロケール指定（`[$-de-DE]0.00`）を角括弧で
表現でき、"Red"の`d`や"Magenta"の`M`のように、角括弧の中身がたまたま日付トークンに
見える文字を含みうる。これらは通常の数値セルの書式であり、日付と誤判定してはならない。
ただし角括弧は経過時間書式（`[h]`/`[hh]`/`[m]`/`[mm]`/`[s]`/`[ss]`、24時間・60分を
超える経過時間を単一の単位で表示するための書式）にも使われ、こちらは日付/時刻の一種
として扱う必要がある。実装では、角括弧の中身が経過時間書式のトークンと完全一致する
場合のみ日付トークンとみなし、それ以外の角括弧の中身は丸ごと判定対象から除外する
（`src/reader/date.rs`の`contains_date_token`、PR #20レビュー指摘を反映）。

## 5. cell.mdのchrono採用根拠との整合

[cell.md 1章](../domain/cell.md#1-cellvalue)は「`chrono`を選ぶ唯一の決定根拠」を
「Reader候補である`calamine`が公式に`chrono`フィーチャーを提供している」ことだと
記載しているが、[mod.md 4章](mod.md#4-使用ライブラリの決定-umya-spreadsheet)の通り本Issueでは
`calamine`ではなく`umya-spreadsheet`を採用している。

改めて検証した結果、**`umya-spreadsheet`も`helper::date::excel_to_date_time_chrono`として
`chrono::NaiveDateTime`への変換ヘルパーを公式に提供しており**（2章）、`calamine`と同様に
`chrono`エコシステムとの親和性は満たされている。したがって `chrono` 採用の結論自体は
引き続き妥当だが、**cell.md 1章の「根拠」の記述（`calamine`前提）は事実と食い違っており、
「Reader候補であるumya-spreadsheetが`helper::date`モジュールでchrono変換ヘルパーを提供している」
という記述に修正する必要がある。この修正は本Issueのスコープ外のため、別Issue/PRで
cell.mdを更新する。**

## 6. 未確定事項

- カスタム日付書式のトークン検出ロジックの詳細（4章）。リテラル文字列・エスケープ文字・
  角括弧区間（色指定/ロケール指定/経過時間書式）の除外は実装済みだが、他の未知の
  誤検出源が実データで見つかる可能性は残る
- `ssfmt`採用可否との実装重複の整理（4章）
- Mac 1904カレンダー基準ファイルへの対応要否（2章、実データでの遭遇頻度次第）
- [cell.mdのchrono根拠記述を修正するPR](#5-cellmdのchrono採用根拠との整合)（別Issueとして起票）
