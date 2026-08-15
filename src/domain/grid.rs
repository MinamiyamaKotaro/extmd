/// 行優先(row-major)のフラット2次元配列。`Sheet::cells`で`Grid<Cell>`として使う、
/// domain内で唯一の汎用コンテナ型。
///
/// 設計判断（row-majorフラット配列を選んだ理由・境界チェックの方針等）は
/// docs/design/domain/grid.md を参照。
#[derive(Debug, Clone, PartialEq)]
pub struct Grid<T> {
    rows: usize,
    cols: usize,
    cells: Vec<T>,
}

impl<T> Grid<T> {
    /// `cols == 0` は許容しない。`chunks(0)`が常にpanicするため（`iter_rows`参照）。
    /// `rows == 0` は許容する（列は定義されているが行データのない空シートを表現できる）。
    ///
    /// `rows * cols`は`checked_mul`でオーバーフローを検出する。オーバーフローした状態で
    /// `cells.len()`とたまたま一致してしまうと、後続の`row`/`iter_rows`が誤ったチャンク
    /// 幅で境界外アクセスを起こしうるため、単純な`assert_eq!(cells.len(), rows * cols)`
    /// より安全側に倒す。
    pub fn new(rows: usize, cols: usize, cells: Vec<T>) -> Self {
        assert!(cols > 0, "Grid: cols must be greater than 0");
        let expected_len = rows
            .checked_mul(cols)
            .expect("Grid: rows * cols overflowed usize");
        assert_eq!(
            cells.len(),
            expected_len,
            "Grid: cells.len() must equal rows * cols"
        );
        Self { rows, cols, cells }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    /// 境界外アクセスに対して`None`を返す安全なAPI。外部入力由来のインデックスは
    /// 必ずこちら経由でアクセスする。
    pub fn get(&self, row: usize, col: usize) -> Option<&T> {
        (row < self.rows && col < self.cols).then(|| &self.cells[row * self.cols + col])
    }

    /// シート内部の走査（既に妥当性が保証されたインデックスだけを辿る）向けの高速パス。
    /// 境界外アクセス時はpanicする。
    pub fn row(&self, row: usize) -> &[T] {
        &self.cells[row * self.cols..(row + 1) * self.cols]
    }

    /// `row`の安全版。範囲外なら`None`を返す。
    pub fn get_row(&self, row: usize) -> Option<&[T]> {
        (row < self.rows).then(|| self.row(row))
    }

    pub fn iter_rows(&self) -> impl Iterator<Item = &[T]> {
        self.cells.chunks(self.cols)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_builds_row_major_grid() {
        let grid = Grid::new(2, 3, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(grid.rows(), 2);
        assert_eq!(grid.cols(), 3);
        assert_eq!(grid.row(0), &[1, 2, 3]);
        assert_eq!(grid.row(1), &[4, 5, 6]);
    }

    #[test]
    fn new_allows_zero_rows() {
        let grid: Grid<i32> = Grid::new(0, 1, vec![]);
        assert_eq!(grid.rows(), 0);
        assert_eq!(grid.cols(), 1);
        assert_eq!(grid.iter_rows().count(), 0);
    }

    #[test]
    #[should_panic(expected = "cols must be greater than 0")]
    fn new_rejects_zero_cols() {
        let _: Grid<i32> = Grid::new(0, 0, vec![]);
    }

    #[test]
    #[should_panic(expected = "cells.len() must equal rows * cols")]
    fn new_rejects_mismatched_cell_count() {
        let _ = Grid::new(2, 2, vec![1, 2, 3]);
    }

    #[test]
    #[should_panic(expected = "rows * cols overflowed usize")]
    fn new_rejects_overflowing_dimensions() {
        // レビュー指摘: rows * cols の乗算オーバーフローで assert_eq! をすり抜けないことを確認する。
        let _: Grid<i32> = Grid::new(usize::MAX, 2, vec![]);
    }

    #[test]
    fn get_returns_none_out_of_bounds() {
        let grid = Grid::new(1, 1, vec![42]);
        assert_eq!(grid.get(0, 0), Some(&42));
        assert_eq!(grid.get(1, 0), None);
        assert_eq!(grid.get(0, 1), None);
    }

    #[test]
    fn get_row_returns_none_out_of_bounds() {
        let grid = Grid::new(1, 2, vec![1, 2]);
        assert_eq!(grid.get_row(0), Some(&[1, 2][..]));
        assert_eq!(grid.get_row(1), None);
    }

    #[test]
    fn iter_rows_yields_each_row() {
        let grid = Grid::new(3, 2, vec![1, 2, 3, 4, 5, 6]);
        let rows: Vec<&[i32]> = grid.iter_rows().collect();
        assert_eq!(rows, vec![&[1, 2][..], &[3, 4][..], &[5, 6][..]]);
    }
}
