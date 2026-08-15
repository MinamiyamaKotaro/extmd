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
    pub fn new(rows: usize, cols: usize, cells: Vec<T>) -> Self {
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

## 2. 設計判断: row-majorフラット配列

`Vec<Vec<T>>`（行ごとにVec）ではなく、行優先(row-major)のフラット `Vec<T>` +
`rows`/`cols` を採用する。理由:

- メモリが連続するためキャッシュ効率がよく、行方向の走査（はみ出し判定・行分類は
  いずれも行単位の走査）が速い
- 「全行が同じ列数を持つ」という不変条件を型構築時（`new`）の1箇所で保証でき、
  `Vec<Vec<T>>` のように行ごとに長さがずれる余地がない

## 3. 境界チェックの方針

`get`/`get_row` は境界外アクセスに対して `Option` を返す安全なAPIとし、外部入力
（Reader経由で得られた行・列インデックス等）を扱う場合は必ずこちら経由でアクセスする。
一方 `row`/`iter_rows` は「シート内部の走査（はみ出し判定・行分類など）で、既に
妥当性が保証されたインデックスだけを辿る」内部利用限定の高速パスとして残し、
境界外アクセス時はpanicを許容する。

**Reader側の責務（`Grid`の不変条件）:** `Grid::new` は `cells.len() == rows * cols`
をアサートするため、構築後の `Grid` が不揃いな行を持つことはない。したがって
「まばらなセル（Sparse Sheet）でどの行の長さも揃わない」というリスクは
**`Grid`自体ではなく、`Grid`を構築する前のReader側の生データ変換**にのみ存在する。
Readerは、Excelの実データ範囲（Bounding Box）に基づいて `rows × cols` の矩形領域を
確保し、値が存在しないセルは `CellValue::Empty`（[cell.md](cell.md)）で埋めてから
`Grid::new` を呼ぶ必要がある。この責務は `reader/` の設計時に正式に文書化する
（[Issue #1のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/issues/1)での指摘を反映）。

## 4. 未確定事項

- 上記のReader側正規化の実装詳細（境界条件の洗い出し）は `reader/` の設計時に詰める
- 空シート（`rows == 0` または `cols == 0`）を `Grid` がどう表現するか
  （`Grid::new(0, 0, vec![])` を許容するか等）は未確定
