# `reader::xlsx` 設計書

対象: [reader/mod.md](mod.md)の対応表における `xlsx.rs`。

## 1. 責務

`umya-spreadsheet` を用いたファイル・ワークブック・ワークシート操作のライフサイクルを
管理し、[cell_mapper.rs](cell_mapper.md)・[date.rs](date.md)・[grid_builder.rs](grid_builder.md)・
[validation.rs](validation.md) を呼び出して結果を統合する、`reader` モジュールの
唯一の「まとめ役」。umya-spreadsheetの型（`umya_spreadsheet::Worksheet` 等）に
直接触れるのはこのファイルと、このファイルから呼ばれる子モジュールのみに限定する。

## 2. 処理フロー

```rust
pub(crate) fn read_sheets(path: &std::path::Path) -> Result<Vec<domain::Sheet>, ReaderError> {
    let book = umya_spreadsheet::reader::xlsx::read(path)
        .map_err(|e| ReaderError::Parse(e.to_string()))?;

    book.get_sheet_collection()
        .iter()
        .map(build_sheet)
        .collect()
}

fn build_sheet(ws: &umya_spreadsheet::Worksheet) -> Result<domain::Sheet, ReaderError> {
    let (highest_col, highest_row) = ws.highest_column_and_row();

    // 3章: 列数0シートの扱い
    let (rows, cols) = if highest_col == 0 {
        (0, 1)
    } else {
        (highest_row as usize, highest_col as usize)
    };

    let cells = grid_builder::build_grid(ws, rows, cols); // grid_builder.md
    let merges = validation::collect_valid_merges(ws, rows, cols); // validation.md

    Ok(domain::Sheet {
        name: ws.get_name().to_string(),
        cells,
        merges,
    })
}
```

`umya_spreadsheet::reader::xlsx::read` はファイルI/O・xml解析の両方のエラーを
返しうるため、`ReaderError::Parse` にラップする（[mod.md 5章](mod.md#5-readererror-と公開api)）。
ファイル不在自体は `read` 呼び出し前に確認せず、`read` のエラーをそのまま
`ReaderError` に変換する（存在しないパスもumya-spreadsheet側のI/Oエラーとして
一貫して扱えるため、二重にチェックしない）。

## 3. 列数0のシートの扱い

`umya-spreadsheet`の `Worksheet::highest_column_and_row()` はデータが1つもない
シートに対して `(0, 0)` を返す。[grid.md 4章](../domain/grid.md#4-境界チェックの方針)より
`Grid::new` は `cols > 0` を必須とするため、このケースは
**`rows = 0, cols = 1`** として `Grid` を構築する（空の1列・0行のGrid）。
これによりパニックを起こさず変換処理を続行し、空のMarkdownとして出力できる
（[Issue #4のコメント](https://github.com/MinamiyamaKotaro/extmd/issues/4#issuecomment-5301613143)の提案を反映）。

ファイル自体が破損している、またはxlsxとして不正な状態になっている場合
（`umya_spreadsheet::reader::xlsx::read` がエラーを返す場合）は、このパスには
到達せず `ReaderError::Parse` として上位に伝播する。両者は原因が異なるため区別する:

| ケース | 扱い |
|---|---|
| ファイルは正常だが、シートにセルデータが1つもない | `rows=0, cols=1` の空`Grid`として処理続行 |
| ファイル自体の読み込み・パースに失敗（破損等） | `ReaderError::Parse` を返しシート単位では処理しない |

## 4. シート単位のエラー伝播

`book.get_sheet_collection().iter().map(build_sheet).collect()` により、
1シートでも `build_sheet` が失敗（`ReaderError` を返す）した場合は全体を
`Result::Err` として打ち切る（`Iterator::collect::<Result<Vec<_>, _>>()` の挙動）。
一部のシートだけ変換をスキップして処理を続行する「部分成功」は v1 では扱わない
（[要件定義書 5.2](../../requirement/requirements.md#52-入力)の「エラー発生時は原因が特定しやすい
メッセージを出力する」という方針とも整合し、どのシートが原因かを呼び出し元が
特定しやすくなる）。

## 5. 未確定事項

- `ReaderError::Parse` に、失敗したシート名を含めるかどうか（4章の「原因特定しやすさ」を
  高めるため、`build_sheet` 側でシート名をエラーに付与する案が有力）
- 数千シート規模のワークブックに対する `read_sheets` 全体の処理時間（非機能要件との整合は
  実データでの検証が必要）
