# `renderer::table` 設計書

対象: [renderer/mod.md](mod.md)の対応表における `table.rs`。

## 1. 責務

`mod.rs`（[mod.md 6章](mod.md#6-documentから本文への組み立て)）がグループ化した、連続する
`RowKind::TableRow`の`RenderedRow`列を、1つのMarkdownパイプテーブルへ変換する。

## 2. 変換アルゴリズム

```rust
pub(in crate::renderer) fn render_table(rows: &[domain::RenderedRow]) -> String {
    let col_count = rows
        .iter()
        .flat_map(|row| row.blocks.iter())
        .map(|b| b.col_end + 1)
        .max()
        .unwrap_or(0);

    let mut lines = Vec::with_capacity(rows.len() + 1);
    let mut prev_cells: Option<Vec<String>> = None;
    for (i, row) in rows.iter().enumerate() {
        let cells = row_cells(row, col_count);
        // 3.1章: ヘッダー行（i == 0）自体は比較対象にせず、i > 1のデータ行同士でのみ
        // 完全一致による重複を除去する
        if i > 1 && prev_cells.as_ref() == Some(&cells) {
            continue;
        }
        lines.push(render_data_row(&cells));
        if i == 0 {
            // 4章: グループ先頭行をヘッダー行として扱う
            lines.push(alignment_row(col_count));
        }
        prev_cells = Some(cells);
    }
    lines.join("\n")
}

fn row_cells(row: &domain::RenderedRow, col_count: usize) -> Vec<String> {
    let mut cells = vec![String::new(); col_count];
    for block in &row.blocks {
        // 3章: 結合範囲は左端セル（col_start）にのみ値を出力し、
        // col_start+1..=col_end は空セルのままにする
        cells[block.col_start] = escape::escape_table_cell(&block.text);
    }
    cells
}

fn render_data_row(cells: &[String]) -> String {
    format!("| {} |", cells.join(" | "))
}

fn alignment_row(col_count: usize) -> String {
    format!("|{}|", vec!["---"; col_count].join("|"))
}
```

## 3. 結合セル（`col_start`/`col_end`）の表現

Markdownのパイプテーブルはネイティブな`colspan`構文を持たない。`Block`が複数列にまたがる
場合（`span() > 1`、[domain/block.md](../domain/block.md#2-block)）、左端のセル（`col_start`）に
値を出力し、結合されている残りの列（`col_start + 1 〜 col_end`）は空文字のセルとして出力する。
これによりテーブル全体の列数（`col_count`）を破壊せず、シンプルに変換できる
（[Issue #8の最初のコメント](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301901971)で
提案された方針をそのまま採用）。

縦方向の結合（rowspan相当）はMarkdownのパイプテーブルにネイティブな構文がないため、
`table.rs`自身はrowspanを表現しない。その代わり`analysis`層（[analysis/mod.md 4章](../analysis/mod.md#4-行単位のオーケストレーション-はみ出しネイティブ結合見出し判定)）が、
結合範囲の左上セル以外の各行についても左上セルの値を複製した`Block`
（`source: BlockSource::NativeMerge`）を生成する（[analysis/mod.md 4章](../analysis/mod.md#4-行単位のオーケストレーション-はみ出しネイティブ結合見出し判定)）。
`table.rs`から見れば結合範囲の全ての行に同じ値を持つ独立したブロックが存在するだけであり、
上記の通常の`col_start`書き込みロジックがそのまま適用される。当初は縦方向の結合をv1スコープ外
としていたが、実データで業務フォーマット（議事録等）に頻出することが判明したため
（[Issue #46](https://github.com/MinamiyamaKotaro/extmd/issues/46)）、値を空欄にせず
複製する方針に変更した（何も表示しないより、同じ値が複数行に渡って表示される方が
可読性が高いという判断。セル結合のビジュアル自体はMarkdownで再現できないため対象外のまま）。

### 3.1 全列が完全一致するデータ行の除去

Excel方眼紙的な業務フォーマット（スキルシート等）では、1件の論理レコードを複数行の高さに
見せるためだけに、多くの列（プロジェクト番号・期間・使用技術等）が同じ行範囲でまとめて縦結合
されていることがある。この場合、3章の複製方針を複数列に単純適用すると、実質的な差分を
持たない行がそのままMarkdownテーブルの行として繰り返し出力され、冗長になる（実データ
`tests/fixtures/complex.xlsx`で確認された挙動）。

これを避けるため、`render_table`はヘッダー行（`i == 0`）を除く各データ行について、直前の
データ行と全列の値が完全に一致する場合はその行を出力しない。ヘッダー行自体は比較対象に
含めない（先頭データ行がたまたまヘッダーと同じ値であっても出力する）。この結果、ある列だけが
異なる行（例: プロジェクト最終行だけ期間が「10ヶ月」のような要約値に変わる）は、その列の差分が
あるため引き続き独立した行として出力される。

## 4. ヘッダー行の扱いについての設計判断

Markdownのパイプテーブル構文は、ヘッダー行と区切り行（`|---|---|`）を必須とする。一方、
`domain::Block`/`RenderedRow`にはどの行が「見出し行」かを示すフィールドは存在せず
（[domain/document.md](../domain/document.md)）、`AnalysisStrategy::classify_row`
（[architecture.md 4章](../architecture.md#4-analysisstrategy-トレイト)）も`TableRow`かどうかしか
判定しない。

v1では、**連続する`TableRow`グループの先頭行を常にヘッダー行として扱う**設計とする。
実務上、方眼紙・通常表のいずれでも表の先頭行が列見出しであるケースが多いという想定に基づく
ヒューリスティックであり、先頭行が実際には見出しではない（データ行がそのままヘッダー行として
太字表示される）表形式データについては、v1では正しく表現できない既知の制約とする（6章）。

## 5. セルのエスケープ

各セルの値は[escape.md](escape.md)の`escape_table_cell`を通す。パイプ文字（`|`）や
セル内改行（`\n`）を放置するとテーブル構造が壊れるため、`table.rs`は自前でエスケープせず
`escape.rs`の関数に委譲する（[Issue #8の最初のコメント](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301901971)）。

## 6. 未確定事項

- 先頭行を常にヘッダー行として扱う方針（4章）が実データにおいて許容範囲か。
  「本当はヘッダー行を持たない表」をどう見分けるかは、`AnalysisStrategy`側に
  ヘッダー行判定を持たせるかどうかを含め、将来の拡張として検討する
- ~~`col_count`が0になるケース（グループ内の全行が空の`blocks`を持つ）は理論上
  `RowKind::TableRow`と分類される時点で発生しないはずだが、実装時にテストで確認する~~
  → 実装時、`TabularStrategy::classify_row`（[strategies/tabular.md 2章](../analysis/strategies/tabular.md#2-トレイト実装)）が
  空の`blocks`でも常に`TableRow`を返す実装になっており、この前提が成立しないことが判明した
  （Issue #33）。`GridPaperStrategy::classify_row`（PR #21）と同じ理由で
  `TabularStrategy::classify_row`も`blocks.is_empty()`の場合に`Flow`を返すよう修正し、
  `TableRow`に分類される行は必ず1つ以上のブロックを持つという不変条件を回復した。
