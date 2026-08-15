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
pub(crate) fn read_sheets(path: &std::path::Path, max_cells: usize) -> Result<Vec<domain::Sheet>, ReaderError> {
    let book = umya_spreadsheet::reader::xlsx::read(path)
        .map_err(|e| ReaderError::Parse(e.to_string()))?;

    book.get_sheet_collection()
        .iter()
        .map(|ws| build_sheet(ws, max_cells))
        .collect()
}

fn build_sheet(ws: &umya_spreadsheet::Worksheet, max_cells: usize) -> Result<domain::Sheet, ReaderError> {
    let (highest_col, highest_row) = ws.highest_column_and_row();

    // 3章: 列数0シートの扱い
    let (rows, cols) = if highest_col == 0 {
        (0, 1)
    } else {
        (highest_row as usize, highest_col as usize)
    };

    // 3.1章: 悪意ある/破損したファイルがメタデータ上の座標のみを巨大化させ、
    // grid_builder::build_grid の rows * cols 件のメモリ確保でDoSを引き起こすことを防ぐ
    // （reader/mod.md 4.1節、docs/security/design-review.md #2、Issue #14）。
    let cell_count = rows.saturating_mul(cols);
    if cell_count > max_cells {
        return Err(ReaderError::SheetTooLarge {
            name: ws.get_name().to_string(),
            rows,
            cols,
            limit: max_cells,
        });
    }

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

### 3.1 `max_cells` によるシートサイズの上限チェック

`umya_spreadsheet::reader::xlsx::read` の時点で対象ファイル全体のパースは既に完了しており
（Eagerパース、[mod.md 4章](mod.md#4-使用ライブラリの決定-umya-spreadsheet)）、この
チェックはパース自体の実行中に発生しうるリソース消費を防ぐものではない。あくまで
「パースには成功したが、`Grid`構築（`grid_builder::build_grid`）に必要な
`rows * cols` 件のメモリ確保がメモリ枯渇を引き起こす」という、`highest_column_and_row()`
が返す座標を悪意的に巨大化させた場合のリスクに対する防御である
（[mod.md 4.1節](mod.md#41-依存ライブラリのセキュリティ検証と監査方針)が明記する
Zip Bomb由来の残存リスクとは区別される）。

## 4. シート単位のエラー伝播

`book.get_sheet_collection().iter().map(|ws| build_sheet(ws, max_cells)).collect()` により、
1シートでも `build_sheet` が失敗（`ReaderError` を返す）した場合は全体を
`Result::Err` として打ち切る（`Iterator::collect::<Result<Vec<_>, _>>()` の挙動）。
一部のシートだけ変換をスキップして処理を続行する「部分成功」は v1 では扱わない
（[要件定義書 5.2](../../requirement/requirements.md#52-入力)の「エラー発生時は原因が特定しやすい
メッセージを出力する」という方針とも整合し、どのシートが原因かを呼び出し元が
特定しやすくなる）。`SheetTooLarge`（3.1節）もこの伝播規則に従い、超過したシートが
1つでもあれば全体を打ち切る。

## 5. 未確定事項

- ~~`ReaderError::Parse` に、失敗したシート名を含めるかどうか（4章の「原因特定しやすさ」を
  高めるため、`build_sheet` 側でシート名をエラーに付与する案が有力）~~
  → Issue #29での実装時の再確認により、この案は本ファイルの実装と噛み合わないことが判明した。
  `read_sheets`（本ファイル1行目〜）は`umya_spreadsheet::reader::xlsx::read(path)`で
  ワークブック全体を一括パースしており、`ReaderError::Parse`はこの全体パースが失敗した
  場合にのみ発生する。個々のシート単位でパースが失敗するという経路は現行アーキテクチャには
  存在しない（`build_sheet`が返しうるエラーは`ReaderError::SheetTooLarge`のみで、
  こちらは元々`name`フィールドを持っている）ため、`Parse`にシート名を付与する変更は行わない。
  未確定事項として残っていた「原因特定しやすさ」の向上は、代わりに呼び出し元の
  `ConvertError::Reader`に入力パスを持たせる形で一部対応した（[reader/mod.md 6章](mod.md#6-未確定事項)参照）。
- 数千シート規模のワークブックに対する `read_sheets` 全体の処理時間（非機能要件との整合は
  実データでの検証が必要）
