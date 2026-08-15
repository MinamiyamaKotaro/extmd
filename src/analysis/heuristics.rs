//! `metrics.rs`（シート全体の特徴量算出）と`strategies/grid_paper.rs`（個別セルのはみ出し
//! 判定）の両方から呼ばれる、セル単位の判定・計算ロジックを共通化する
//! （docs/design/analysis/heuristics.md）。

use crate::domain;

pub(in crate::analysis) fn is_overflow_candidate(
    cell: &domain::Cell,
    next: Option<&domain::Cell>,
    threshold: f64,
) -> bool {
    !cell.wrap_text
        && next.is_some_and(domain::Cell::is_empty)
        && estimate_render_width(cell) > cell.column_width * threshold
}

pub(in crate::analysis) fn estimate_render_width(cell: &domain::Cell) -> f64 {
    let text = cell.value.display_text();
    let base_width: f64 = text
        .chars()
        .map(|c| if is_full_width(c) { 2.0 } else { 1.0 })
        .sum();

    // Excelの列幅は既定フォントサイズ（11pt）を基準とした単位のため、セルのフォント
    // サイズがそれより大きい/小さい場合は比例して幅を補正する。
    base_width * (cell.font.size_pt as f64 / 11.0)
}

/// Unicode East Asian Widthに基づく簡易判定（Wide/Fullwidthに該当する主要な範囲のみ）。
fn is_full_width(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F
            | 0x2E80..=0x303E
            | 0x3041..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xA000..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x20000..=0x3FFFD
    )
}

/// 値を`[min, max]`の範囲で`[0.0, 1.0]`にクランプ線形マッピングする。
pub(in crate::analysis) fn normalize(value: f64, min: f64, max: f64) -> f32 {
    (((value - min) / (max - min).max(f64::EPSILON)).clamp(0.0, 1.0)) as f32
}

pub(in crate::analysis) fn normalize_inverse(value: f64, min: f64, max: f64) -> f32 {
    1.0 - normalize(value, min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(value: domain::CellValue, column_width: f64, wrap_text: bool) -> domain::Cell {
        domain::Cell {
            value,
            column_width,
            wrap_text,
            alignment: domain::Alignment::default(),
            font: domain::FontInfo {
                size_pt: 11.0,
                bold: false,
            },
            number_format: None,
        }
    }

    #[test]
    fn is_full_width_detects_japanese_and_ascii() {
        assert!(is_full_width('あ'));
        assert!(is_full_width('漢'));
        assert!(!is_full_width('a'));
        assert!(!is_full_width('1'));
    }

    #[test]
    fn estimate_render_width_counts_full_width_as_two() {
        let c = cell(domain::CellValue::String("ab".into()), 8.0, false);
        assert_eq!(estimate_render_width(&c), 2.0);
        let c = cell(domain::CellValue::String("あい".into()), 8.0, false);
        assert_eq!(estimate_render_width(&c), 4.0);
    }

    #[test]
    fn estimate_render_width_scales_with_font_size() {
        let mut c = cell(domain::CellValue::String("ab".into()), 8.0, false);
        c.font.size_pt = 22.0;
        assert_eq!(estimate_render_width(&c), 4.0);
    }

    #[test]
    fn is_overflow_candidate_requires_no_wrap_and_empty_next() {
        let source = cell(
            domain::CellValue::String("長い文字列です".into()),
            4.0,
            false,
        );
        let empty_next = cell(domain::CellValue::Empty, 4.0, false);
        assert!(is_overflow_candidate(&source, Some(&empty_next), 1.0));

        let non_empty_next = cell(domain::CellValue::String("x".into()), 4.0, false);
        assert!(!is_overflow_candidate(&source, Some(&non_empty_next), 1.0));
        assert!(!is_overflow_candidate(&source, None, 1.0));

        let wrapped = cell(
            domain::CellValue::String("長い文字列です".into()),
            4.0,
            true,
        );
        assert!(!is_overflow_candidate(&wrapped, Some(&empty_next), 1.0));
    }

    #[test]
    fn normalize_clamps_to_unit_range() {
        assert_eq!(normalize(5.0, 0.0, 10.0), 0.5);
        assert_eq!(normalize(-5.0, 0.0, 10.0), 0.0);
        assert_eq!(normalize(15.0, 0.0, 10.0), 1.0);
    }

    #[test]
    fn normalize_inverse_is_complement_of_normalize() {
        assert_eq!(normalize_inverse(5.0, 0.0, 10.0), 0.5);
        assert_eq!(normalize_inverse(0.0, 0.0, 10.0), 1.0);
    }

    #[test]
    fn normalize_handles_zero_width_range_without_panic() {
        assert_eq!(normalize(5.0, 3.0, 3.0), 1.0);
    }
}
