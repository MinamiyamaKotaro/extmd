//! シートの構造的特徴量。`registry::StrategyRegistry::select_auto`が1シートにつき1回だけ
//! 計算し、登録された全戦略の`affinity`に配る（docs/design/analysis/metrics.md）。

use crate::domain;

use super::heuristics;

/// `metrics::compute_sheet_metrics`が使う保守的な既定しきい値。`registry::StrategyConfig`の
/// `overflow_threshold`既定値と同じ値だが、`metrics`は`registry`に依存しないためここで
/// 独立して定義する（heuristics.md 2章）。
const DEFAULT_OVERFLOW_THRESHOLD: f64 = 1.0;

/// フィールド・[`compute_sheet_metrics`]の可視性は`pub(in crate::analysis)`とし、
/// `select_auto`以外からの呼び出し・構築を防ぐ（4章）。構造体自体は`AnalysisStrategy::affinity`
/// という公開トレイトメソッドの引数型として現れるため`pub`とする。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetMetrics {
    pub(in crate::analysis) avg_column_width: f64,
    pub(in crate::analysis) column_width_stddev: f64,
    pub(in crate::analysis) overflow_candidate_rate: f32,
    pub(in crate::analysis) fill_density: f32,
    pub(in crate::analysis) row_structural_regularity: f32,
    pub(in crate::analysis) merge_irregularity: f32,
}

/// `registry::StrategyRegistry::select_auto`からのみ呼ばれる契約。
pub(in crate::analysis) fn compute_sheet_metrics(sheet: &domain::Sheet) -> SheetMetrics {
    let non_empty_count = sheet
        .cells
        .iter_rows()
        .flatten()
        .filter(|c| !c.is_empty())
        .count();

    // 非空セルが1つもない場合、以下の各指標はいずれもゼロ除算になりうる。
    // `rows=0`の空Gridを含め、パニックさせずデフォルト値を返す。
    if non_empty_count == 0 {
        return SheetMetrics {
            avg_column_width: 0.0,
            column_width_stddev: 0.0,
            overflow_candidate_rate: 0.0,
            fill_density: 0.0,
            row_structural_regularity: 0.0,
            merge_irregularity: 0.0,
        };
    }

    let (avg_column_width, column_width_stddev) = column_width_stats(sheet);
    let overflow_candidate_rate = overflow_candidate_rate(sheet, non_empty_count);
    let fill_density = fill_density(sheet, non_empty_count);
    let row_structural_regularity = row_structural_regularity(sheet);
    let merge_irregularity = merge_irregularity(sheet);

    SheetMetrics {
        avg_column_width,
        column_width_stddev,
        overflow_candidate_rate,
        fill_density,
        row_structural_regularity,
        merge_irregularity,
    }
}

/// 列幅（文字数換算）の平均・標準偏差。全セル（空セルを含む）を対象とする。
fn column_width_stats(sheet: &domain::Sheet) -> (f64, f64) {
    let widths: Vec<f64> = sheet
        .cells
        .iter_rows()
        .flatten()
        .map(|c| c.column_width)
        .collect();
    let count = widths.len() as f64;
    let mean = widths.iter().sum::<f64>() / count;
    let variance = widths.iter().map(|w| (w - mean).powi(2)).sum::<f64>() / count;
    (mean, variance.sqrt())
}

/// 「推定描画幅 > 列幅 かつ 右隣が空」を満たす非空セル数 ÷ 非空セル総数。
fn overflow_candidate_rate(sheet: &domain::Sheet, non_empty_count: usize) -> f32 {
    let mut candidates = 0usize;
    for row in sheet.cells.iter_rows() {
        for (col, cell) in row.iter().enumerate() {
            if cell.is_empty() {
                continue;
            }
            if heuristics::is_overflow_candidate(cell, row.get(col + 1), DEFAULT_OVERFLOW_THRESHOLD)
            {
                candidates += 1;
            }
        }
    }
    candidates as f32 / non_empty_count as f32
}

/// 非空セルのbounding box面積に対する非空セル数の割合。
fn fill_density(sheet: &domain::Sheet, non_empty_count: usize) -> f32 {
    let mut min_row = usize::MAX;
    let mut max_row = 0usize;
    let mut min_col = usize::MAX;
    let mut max_col = 0usize;

    for (row_idx, row) in sheet.cells.iter_rows().enumerate() {
        for (col_idx, cell) in row.iter().enumerate() {
            if cell.is_empty() {
                continue;
            }
            min_row = min_row.min(row_idx);
            max_row = max_row.max(row_idx);
            min_col = min_col.min(col_idx);
            max_col = max_col.max(col_idx);
        }
    }

    let area = (max_row - min_row + 1) * (max_col - min_col + 1);
    non_empty_count as f32 / area as f32
}

/// 各行の「非空セルが存在する列インデックスの集合」について、隣接する行同士の
/// Jaccard類似度の平均を取る。両方とも非空セルを持たない行同士のペアは対象から除く。
///
/// 各行の列インデックスは`row.iter().enumerate()`により昇順で収集されるため、
/// `HashSet`ではなくソート済み`Vec`のマージ走査で積集合・和集合のサイズを
/// `O(N1 + N2)`で計算し、ハッシュ化とヒープ確保のオーバーヘッドを避ける
/// （PR #21レビューコメントでの指摘を反映）。
fn row_structural_regularity(sheet: &domain::Sheet) -> f32 {
    let non_empty_cols: Vec<Vec<usize>> = sheet
        .cells
        .iter_rows()
        .map(|row| {
            row.iter()
                .enumerate()
                .filter(|(_, c)| !c.is_empty())
                .map(|(i, _)| i)
                .collect()
        })
        .collect();

    let mut jaccard_sum = 0.0f32;
    let mut pair_count = 0usize;
    for pair in non_empty_cols.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let intersection = count_sorted_intersection(a, b);
        let union = a.len() + b.len() - intersection;
        if union == 0 {
            continue;
        }
        jaccard_sum += intersection as f32 / union as f32;
        pair_count += 1;
    }

    if pair_count == 0 {
        0.0
    } else {
        jaccard_sum / pair_count as f32
    }
}

/// 昇順ソート済みの2つのスライスの積集合の要素数を、マージ走査で計算する。
fn count_sorted_intersection(a: &[usize], b: &[usize]) -> usize {
    let (mut i, mut j) = (0, 0);
    let mut count = 0;
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Equal => {
                count += 1;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    count
}

/// ネイティブ結合セルのうち、単純な「1行×複数列」または「複数行×1列」の矩形以外の
/// 形を取る割合。結合セルが1つもなければ`0.0`（ゼロ除算を避ける独立したガード）。
fn merge_irregularity(sheet: &domain::Sheet) -> f32 {
    if sheet.merges.is_empty() {
        return 0.0;
    }
    let irregular = sheet
        .merges
        .iter()
        .filter(|m| !m.is_single_row_or_column())
        .count();
    irregular as f32 / sheet.merges.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(value: domain::CellValue, column_width: f64) -> domain::Cell {
        domain::Cell {
            value,
            column_width,
            wrap_text: false,
            alignment: domain::Alignment::default(),
            font: domain::FontInfo {
                size_pt: 11.0,
                bold: false,
            },
            number_format: None,
        }
    }

    fn empty(column_width: f64) -> domain::Cell {
        cell(domain::CellValue::Empty, column_width)
    }

    fn text(s: &str, column_width: f64) -> domain::Cell {
        cell(domain::CellValue::String(s.into()), column_width)
    }

    #[test]
    fn compute_sheet_metrics_returns_default_for_all_empty_sheet() {
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(2, 2, vec![empty(8.0), empty(8.0), empty(8.0), empty(8.0)]),
            merges: vec![],
        };
        let m = compute_sheet_metrics(&sheet);
        assert_eq!(m.avg_column_width, 0.0);
        assert_eq!(m.fill_density, 0.0);
        assert_eq!(m.row_structural_regularity, 0.0);
        assert_eq!(m.merge_irregularity, 0.0);
    }

    #[test]
    fn compute_sheet_metrics_returns_default_for_zero_row_sheet() {
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(0, 1, vec![]),
            merges: vec![],
        };
        let m = compute_sheet_metrics(&sheet);
        assert_eq!(m.overflow_candidate_rate, 0.0);
    }

    #[test]
    fn column_width_stats_uses_all_cells() {
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(1, 2, vec![text("a", 4.0), text("b", 6.0)]),
            merges: vec![],
        };
        let m = compute_sheet_metrics(&sheet);
        assert_eq!(m.avg_column_width, 5.0);
        assert_eq!(m.column_width_stddev, 1.0);
    }

    #[test]
    fn fill_density_is_full_for_completely_filled_rectangle() {
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(2, 2, vec![text("a", 4.0); 4]),
            merges: vec![],
        };
        let m = compute_sheet_metrics(&sheet);
        assert_eq!(m.fill_density, 1.0);
    }

    #[test]
    fn row_structural_regularity_is_one_for_identical_column_patterns() {
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(2, 2, vec![text("a", 4.0); 4]),
            merges: vec![],
        };
        let m = compute_sheet_metrics(&sheet);
        assert_eq!(m.row_structural_regularity, 1.0);
    }

    #[test]
    fn merge_irregularity_counts_non_strip_merges() {
        let sheet = domain::Sheet {
            name: "s".into(),
            cells: domain::Grid::new(2, 2, vec![text("a", 4.0); 4]),
            merges: vec![
                domain::MergeRange {
                    row_start: 0,
                    row_end: 0,
                    col_start: 0,
                    col_end: 1,
                },
                domain::MergeRange {
                    row_start: 0,
                    row_end: 1,
                    col_start: 0,
                    col_end: 1,
                },
            ],
        };
        let m = compute_sheet_metrics(&sheet);
        assert_eq!(m.merge_irregularity, 0.5);
    }
}
