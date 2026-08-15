use super::{Cell, Grid};

pub struct MergeRange {
    pub row_start: usize,
    pub row_end: usize, // inclusive
    pub col_start: usize,
    pub col_end: usize, // inclusive
}

impl MergeRange {
    /// 単一の行、または単一の列にまたがる結合（1×N または N×1）か。
    /// analysis層の`merge_irregularity`算出で使う。
    pub fn is_single_row_or_column(&self) -> bool {
        self.row_start == self.row_end || self.col_start == self.col_end
    }
}

/// Readerが構築した後は不変（immutable）として扱う、1シート分の生データ。
///
/// `merges`の各`MergeRange`は`cells`の範囲内に収まっていることを前提とするが、
/// この不変条件は`Sheet`自身は検証せず、Readerが`Sheet`を構築する時点で担保する契約
/// とする（docs/design/domain/sheet.md 3章）。
pub struct Sheet {
    pub name: String,
    pub cells: Grid<Cell>,
    pub merges: Vec<MergeRange>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_single_row_or_column_true_for_row_strip() {
        let m = MergeRange {
            row_start: 0,
            row_end: 0,
            col_start: 0,
            col_end: 3,
        };
        assert!(m.is_single_row_or_column());
    }

    #[test]
    fn is_single_row_or_column_true_for_column_strip() {
        let m = MergeRange {
            row_start: 0,
            row_end: 3,
            col_start: 0,
            col_end: 0,
        };
        assert!(m.is_single_row_or_column());
    }

    #[test]
    fn is_single_row_or_column_false_for_block() {
        let m = MergeRange {
            row_start: 0,
            row_end: 1,
            col_start: 0,
            col_end: 1,
        };
        assert!(!m.is_single_row_or_column());
    }
}
