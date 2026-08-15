# `domain::grid` 設計書

対象: [domain/mod.md](mod.md)の対応表における `grid.rs`。

## 1. `Grid<T>`

```rust
pub struct Grid<T> {
    rows: usize,
    cols: usize,
    cells: Vec<T>, // row-major flat storage
}

impl<T> Grid<T> {
    /// `cols == 0` は許容しない（理由は3章参照）。`rows == 0` は許容する
    /// （行データのない空シートを表現できる）。
    pub fn new(rows: usize, cols: usize, cells: Vec<T>) -> Self {
        assert!(cols > 0, "Grid: cols must be greater than 0");
        assert_eq!(cells.len(), rows * cols, "Grid: cells.len() must equal rows * cols");
        Self { rows, cols, cells }
    }

    pub fn rows(&self) -> usize { self.rows }
    pub fn cols(&self) -> usize { self.cols }

    pub fn get(&self, row: usize, col: usize) -> Option<&T> {
        (row < self.rows && col < self.cols).then(|| &self.cells[row * self.cols + col])
    }

    pub fn row(&self, row: usize) -> &[T] {
        &self.cells[row * self.cols..(row + 1) * self.cols]
    }

    /// `row` の安全版。範囲外なら `None` を返す。
    pub fn get_row(&self, row: usize) -> Option<&[T]> {
        (row < self.rows).then(|| self.row(row))
    }

    pub fn iter_rows(&self) -> impl Iterator<Item = &[T]> {
        self.cells.chunks(self.cols)
    }
}
```

`Grid<T>` は `Sheet::cells`（[sheet.md](sheet.md)）で `Grid<Cell>` として使われる、
domain内で唯一の汎用コンテナ型。

## 2. 構築方法（ミュータブルAPIを持たない理由）

`Grid<T>` は [mod.md 2章](mod.md#2-設計方針)の方針により、構築後は不変（`&self`のみの
API）とする。`get_mut`/`row_mut` のようなミュータブルなアクセサは意図的に用意しない。

Readerが1セルずつExcelファイルを読み進める場合も、`Grid`を直接ミュータブルに更新する
のではなく、**Reader側で先に `Vec<T>`（`rows * cols` 件、`CellValue::Empty` 等で初期化
済み）を組み立て、読み取った値でその `Vec` の該当インデックスを埋めてから、最後に
一度だけ `Grid::new(rows, cols, vec)` を呼ぶ**、という構築パターンを想定する。
これにより `Grid` 自体は常にイミュータブルなまま、Reader側だけがミュータブルな
中間状態（生の`Vec<T>`）を扱う設計になる。

## 3. 設計判断: row-majorフラット配列

`Vec<Vec<T>>`（行ごとにVec）ではなく、行優先(row-major)のフラット `Vec<T>` +
`rows`/`cols` を採用する。理由:

- メモリが連続するためキャッシュ効率がよく、行方向の走査（はみ出し判定・行分類は
  いずれも行単位の走査）が速い
- 「全行が同じ列数を持つ」という不変条件を型構築時（`new`）の1箇所で保証でき、
  `Vec<Vec<T>>` のように行ごとに長さがずれる余地がない

## 4. 境界チェックの方針

`get`/`get_row` は境界外アクセスに対して `Option` を返す安全なAPIとし、外部入力
（Reader経由で得られた行・列インデックス等）を扱う場合は必ずこちら経由でアクセスする。
一方 `row`/`iter_rows` は「シート内部の走査（はみ出し判定・行分類など）で、既に
妥当性が保証されたインデックスだけを辿る」内部利用限定の高速パスとして残し、
境界外アクセス時はpanicを許容する。

**`iter_rows`のパニックリスクと`cols > 0`の必須化:** `iter_rows` は
`self.cells.chunks(self.cols)` を使うが、Rust標準ライブラリの `chunks` は
`chunk_size == 0` の場合、スライスが空かどうかに関わらず必ずpanicする
（`cols == 0` かつ `rows == 0` で `cells` が空であっても回避できない）。
そのため `Grid::new` は `cols > 0` を必須のアサーションとする（1章のコード参照）。
`rows == 0` は許容する（列は定義されているが行データのない空シートを表現できる）ため、
「空シート」は「`rows == 0` かつ `cols > 0`」として表現する、という方針にする
（[PR #3のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/3#issuecomment-5301554119)での指摘を反映）。

**Reader側の責務（`Grid`の不変条件）:** `Grid::new` は `cells.len() == rows * cols`
をアサートするため、構築後の `Grid` が不揃いな行を持つことはない。したがって
「まばらなセル（Sparse Sheet）でどの行の長さも揃わない」というリスクは
**`Grid`自体ではなく、`Grid`を構築する前のReader側の生データ変換**にのみ存在する。
Readerは、Excelの実データ範囲（Bounding Box）に基づいて `rows × cols`（`cols >= 1`）
の矩形領域を確保し、値が存在しないセルは `CellValue::Empty`（[cell.md](cell.md)）で
埋めてから `Grid::new` を呼ぶ必要がある。この責務は `reader/` の設計時に正式に
文書化する（[Issue #1のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/issues/1)での指摘を反映）。

## 5. 未確定事項

- 上記のReader側正規化の実装詳細（境界条件の洗い出し）は `reader/` の設計時に詰める
- 列数が0のシート（列そのものが存在しない不正なファイル等）をどう扱うか
  （`Grid`を構築せずにReader/Analyzerの手前でエラーとするか等）は未確定。
  `Grid`自体は`cols == 0`を受け付けない方針とした（4章参照）
