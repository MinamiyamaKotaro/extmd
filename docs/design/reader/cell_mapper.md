# `reader::cell_mapper` 設計書

対象: [reader/mod.md](mod.md)の対応表における `cell_mapper.rs`。

## 1. 責務

`umya_spreadsheet::Cell` 1つを、[domain/cell.md](../domain/cell.md)の `domain::Cell` へ
変換する。値・列幅・折り返し設定・フォント・文字揃え・表示形式コードの抽出をすべて
このファイルに閉じ込め、[xlsx.rs](xlsx.md)からはumya-spreadsheetのCell/Style型が
見えないようにする。

```rust
pub(crate) fn map_cell(
    cell: &umya_spreadsheet::Cell,
    column_width: f64, // grid_builder.rsが列単位で解決し、セル単位で渡す
) -> domain::Cell {
    let style = cell.style(); // Style自体はOptionではない（&Style）
    let format_code = style.numbering_format().map(|nf| nf.format_code());

    domain::Cell {
        value: map_value(cell, format_code),
        column_width,
        // alignment()/font()/numbering_format()はいずれも
        // 未設定の場合があるためOption<&T>を返す（3.0.0時点のAPI）。
        // 未設定時はExcelの既定値（左揃え・折り返しなし・既定フォント）を採用する。
        wrap_text: style.alignment().is_some_and(|a| a.wrap_text()),
        alignment: style
            .alignment()
            .map_or(domain::Alignment::default(), |a| map_alignment(a.horizontal())),
        font: style.font().map_or(DEFAULT_FONT, |f| domain::FontInfo {
            size_pt: f.size() as f32,
            bold: f.bold(),
        }),
        number_format: format_code
            .filter(|code| *code != "General")
            .map(str::to_string),
    }
}

/// スタイル未設定セルに使うExcelの既定フォント（游ゴシック相当、11pt・非太字）。
const DEFAULT_FONT: domain::FontInfo = domain::FontInfo { size_pt: 11.0, bold: false };
```

## 2. 値の変換（数式セルの解決方針を含む）

`umya_spreadsheet::Cell` の生値は `CellRawValue` で表現され、
`String` / `RichText` / `Lazy` / `Numeric` / `Bool` / `Error` / `Empty` の7バリアントを持つ。
数式セル専用のバリアントは存在せず、`Cell::formula()`/`Cell::is_formula()` は
数式**文字列**を返す別系統のアクセサである。すなわち `Cell::value()`/`value_number()`/
`raw_value()` は数式セルであっても常に**計算済みのキャッシュ値**を返す。

これにより、[要件定義書 4.2「対象外」](../../requirement/requirements.md#42-対象外v1では扱わない)
（数式そのものは対象外、計算結果のみ使用）は次の方針で自然に満たせる:
**`cell_mapper` は `formula()`/`is_formula()` を一切参照せず、常に `raw_value()` 経由で
値を取得する。** 数式かどうかの分岐は不要。

```rust
fn map_value(cell: &umya_spreadsheet::Cell, format_code: Option<&str>) -> domain::CellValue {
    use umya_spreadsheet::CellRawValue;
    match cell.raw_value() {
        CellRawValue::Empty => domain::CellValue::Empty,
        CellRawValue::Numeric(n) if format_code.is_some_and(date::is_date_formatted) => {
            domain::CellValue::Date(date::from_serial(*n)) // date.md参照
        }
        CellRawValue::Numeric(n) => domain::CellValue::Number(*n),
        CellRawValue::Bool(b) => domain::CellValue::Bool(*b),
        CellRawValue::String(s) => domain::CellValue::String(s.to_string()),
        CellRawValue::RichText(rt) => domain::CellValue::String(rt.get_text().into_owned()),
        CellRawValue::Lazy(s) => domain::CellValue::String(s.to_string()),
        CellRawValue::Error(e) => domain::CellValue::String(format!("#{e}")), // 3章参照
    }
}
```

`cell.raw_value()`は`&CellRawValue`を返すため、matchアームの束縛はいずれも参照になる
（`n: &f64`, `b: &bool`, `s: &Box<str>`, `rt: &RichText`）。`Numeric`/`Bool`は値型なので
`*n`/`*b`でデリファレンスし、`String`/`Lazy`（`Box<str>`）は`.to_string()`、
`RichText::get_text()`は`Cow<'static, str>`を返すため`.into_owned()`で所有権のある
`String`に変換する。

## 3. `CellRawValue::Error` の扱い（未確定事項）

`#DIV/0!` 等のエラー値セルに対応する `CellValue` バリアントは
[cell.md](../domain/cell.md)に存在しない。上記コードは暫定的に
`CellValue::String("#<エラー種別>")` として文字列化しているが、
専用の `CellValue::Error` バリアントを追加すべきかは未確定。
要件定義書にエラーセルの扱いに関する明記がないため、実装フェーズで
サンプルファイルでの発生頻度を見て判断する。

## 4. 日付判定: `is_date_formatted`

`umya-spreadsheet`の `CellRawValue` には `calamine` の `DataType::DateTime` に相当する
「日付として型付けされた値」が存在せず、数値セルはすべて `Numeric(f64)` になる
（Excelファイル自体、日付をシリアル値+表示形式の組で表現しており、値自体に
日付という型情報はない）。したがって `cell_mapper` は、セルの表示形式コード
（`number_format`）が日付/時刻を表すパターンかどうかで日付セルを判定する必要がある。
判定ロジックの詳細は[date.md 3章](date.md#3-日付セルの判定-is_date_formatted)を参照。

## 5. 列幅を`Cell`が複製して持つ理由

`column_width` は本来「列」の属性だが、[cell.md 3章](../domain/cell.md#3-cell)の設計判断により
`Cell` にも複製して持たせる。このため `cell_mapper::map_cell` は列幅を引数で受け取る形にし、
実際の列幅解決（`Column::width()` の取得、および[3章](#3-cellrawvalueerror-の扱い未確定事項)とは別に
Excelの「未設定列はデフォルト幅」というルールの反映）は[grid_builder.rs](grid_builder.md)側の
責務とする（`cell_mapper` はワークシート全体やcolumn_dimensionsを見ない、1セル単位の
変換に閉じた関数にするため）。

## 6. `number_format` フィールドの追加（domain層への変更）

`display_text()`（[cell.md 1章](../domain/cell.md#1-cellvalue)）が呼ぶ `format_number`/`format_date`
が、桁区切り・パーセンテージ・和暦等のExcel書式（[cell.md 4章](../domain/cell.md#4-format_number--format_date-のフォーマット方針)）を
再現するには、そのセルの表示形式コード（例: `"#,##0"`, `"yyyy/m/d"`）が必要になる。
これをReaderが取得して渡せるよう、`domain::Cell` に `number_format: Option<String>` を
追加した（[cell.md](../domain/cell.md)を本Issueに合わせて更新済み）。`None` は
umya-spreadsheetの `format_code()` が既定値（`"General"` 等、書式未設定）を返した場合を表す。

## 7. 未確定事項

- [cell.md 3章](#3-cellrawvalueerror-の扱い未確定事項)の `CellValue::Error` バリアント要否
- `RichText` セルを単純に「装飾を無視したプレーンテキスト」として結合してよいか
  （部分的に太字/色が異なる文字列など、`FontInfo` を持つ `Cell` 単位の書式では表現しきれない
  ケースがどの程度実データに存在するかは未検証）
