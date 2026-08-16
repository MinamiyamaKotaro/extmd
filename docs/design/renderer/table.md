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
    for (i, row) in rows.iter().enumerate() {
        let cells = row_cells(row, col_count);
        // 3.1章: 全列が縦結合の後続行（空欄）である行は出力しない。ヘッダー行（i == 0）は
        // 常に出力する
        if i > 0 && cells.iter().all(String::is_empty) {
            continue;
        }
        lines.push(render_data_row(&cells));
        if i == 0 {
            // 4章: グループ先頭行をヘッダー行として扱う
            lines.push(alignment_row(col_count));
        }
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
`table.rs`自身はrowspanを表現しない。横方向の結合（colspan）と同じ方針を採り、値は
結合範囲の左上セルの行にのみ出力し、それ以外の行では空欄にする。当初（[Issue #46](https://github.com/MinamiyamaKotaro/extmd/issues/46)）は
「何も表示しないより複製する方が可読性が高い」という判断で左上セルの値を複製していたが、
Excel方眼紙的な業務フォーマット（スキルシート等、実データ`tests/fixtures/complex.xlsx`）では
1件の論理レコードを複数行の高さに見せるためだけに多くの列がまとめて縦結合されており、
複製方針だと実質差分の無い行がテーブルに繰り返し出力されて冗長になることが判明したため、
複製せず空欄にする方針へ変更した（[Issue #52](https://github.com/MinamiyamaKotaro/extmd/issues/52)）。

`analysis`層（[analysis/mod.md 4章](../analysis/mod.md#4-行単位のオーケストレーション-はみ出しネイティブ結合見出し判定)）は、
結合範囲の左上セル以外の各行についても`Block`（`source: BlockSource::NativeMerge`、
`col_start`/`col_end`は結合範囲のまま）は生成し続けるが、`text`は空文字にする。`table.rs`
から見れば、結合範囲の後続行にも「値が空文字のブロック」が存在するだけであり、上記の通常の
`col_start`書き込みロジックがそのまま適用されて空欄セルになる。Blockの生成自体は続ける理由は
3.1章を参照。

### 3.1 全列が縦結合の後続行であるデータ行の除去

3章の方針により、縦結合の後続行では対象列が空欄になる。Excel方眼紙的な業務フォーマットでは、
1つの論理行の全列が縦結合の後続行になり、その行が実質的に何の情報も持たない（空欄セルだけの
行になる）ケースがある。これをそのままMarkdownテーブルの行として出力すると、意味のない
空行（`| | | |`等）がテーブル中に現れてしまう。

これを避けるため、`render_table`はヘッダー行（`i == 0`）を除く各データ行について、全列が
空文字である場合はその行を出力しない。1列でも値を持つ行（例: プロジェクト最終行だけ期間が
「10ヶ月」のような要約値に変わる行）は、その列の値を保持するため引き続き出力される。

なお`Block`自体（テキストは空でも）は`analysis`層が結合範囲の全行に生成し続ける（3章）。
これは、ある行の全列が縦結合の後続行であっても`blocks`は空にならないようにするためで、
`TabularStrategy`/`GridPaperStrategy`の`classify_row`（[strategies/tabular.md](../analysis/strategies/tabular.md)/
[strategies/grid_paper.md](../analysis/strategies/grid_paper.md)）は`blocks`が空の行を`Flow`に
分類する。もし`Block`ごと生成しなければ、全列が縦結合の後続行である行が`Flow`と誤判定され、
`render_body`（[mod.md 6章](mod.md#6-documentから本文への組み立て)）の`TableRow`グループ化が
そこで分断されてしまう（後続のテーブル行がヘッダー行の無い新しいテーブルとして出力される）。
その分断を防ぐため、行の構造的な分類は`analysis`層のBlock生成で担保し、値の重複除去は
`table.rs`側の空欄行スキップで担保する、という役割分担にしている。

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
