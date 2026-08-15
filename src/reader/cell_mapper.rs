//! `umya_spreadsheet::Cell` 1つを `domain::Cell` へ変換する。値・列幅・折り返し設定・
//! フォント・文字揃え・表示形式コードの抽出をここに閉じ込め、[xlsx.rs](super::xlsx)から
//! umya-spreadsheetのCell/Style型が見えないようにする（docs/design/reader/cell_mapper.md）。

use umya_spreadsheet::{CellRawValue, HorizontalAlignmentValues};

use crate::domain;

use super::date;

/// スタイル未設定セルに使うExcelの既定フォント（游ゴシック相当、11pt・非太字）。
const DEFAULT_FONT: domain::FontInfo = domain::FontInfo {
    size_pt: 11.0,
    bold: false,
};

pub(crate) fn map_cell(cell: &umya_spreadsheet::Cell, column_width: f64) -> domain::Cell {
    let style = cell.style();
    let format_code = style.numbering_format().map(|nf| nf.format_code());

    domain::Cell {
        value: map_value(cell, format_code),
        column_width,
        wrap_text: style.alignment().is_some_and(|a| a.wrap_text()),
        alignment: style.alignment().map_or(domain::Alignment::default(), |a| {
            map_alignment(a.horizontal())
        }),
        font: style.font().map_or(DEFAULT_FONT, |f| domain::FontInfo {
            size_pt: f.size() as f32,
            bold: f.bold(),
        }),
        number_format: format_code
            .filter(|code| *code != "General")
            .map(str::to_string),
    }
}

/// 数式セル専用のバリアントは存在しない。`Cell::formula()`/`Cell::is_formula()`は数式
/// 文字列を返す別系統のアクセサであり、`raw_value()`は数式セルであっても常に計算済みの
/// キャッシュ値を返す。数式そのものは対象外（要件定義書4.2）のため、`formula()`/
/// `is_formula()`は一切参照しない（cell_mapper.md 2章）。
fn map_value(cell: &umya_spreadsheet::Cell, format_code: Option<&str>) -> domain::CellValue {
    match cell.raw_value() {
        CellRawValue::Empty => domain::CellValue::Empty,
        CellRawValue::Numeric(n) if format_code.is_some_and(date::is_date_formatted) => {
            domain::CellValue::Date(date::from_serial(*n))
        }
        CellRawValue::Numeric(n) => domain::CellValue::Number(*n),
        CellRawValue::Bool(b) => domain::CellValue::Bool(*b),
        CellRawValue::String(s) => domain::CellValue::String(s.to_string()),
        CellRawValue::RichText(rt) => domain::CellValue::String(rt.text().into_owned()),
        CellRawValue::Lazy(s) => domain::CellValue::String(s.to_string()),
        // `CellErrorType`のDisplay実装は既に`#`を含む文字列（例: "#DIV/0!"）を返すため、
        // ここで追加の`#`を前置しない（design docのcell_mapper.md 2章記載のコード例
        // `format!("#{e}")`は"##DIV/0!"のように二重の`#`になってしまう誤りだったため、
        // 実装時にto_string()へ修正した）。
        CellRawValue::Error(e) => domain::CellValue::String(e.to_string()),
    }
}

fn map_alignment(horizontal: &HorizontalAlignmentValues) -> domain::Alignment {
    match horizontal {
        HorizontalAlignmentValues::Right => domain::Alignment::Right,
        HorizontalAlignmentValues::Center
        | HorizontalAlignmentValues::CenterContinuous
        | HorizontalAlignmentValues::Distributed
        | HorizontalAlignmentValues::Justify => domain::Alignment::Center,
        HorizontalAlignmentValues::Left
        | HorizontalAlignmentValues::General
        | HorizontalAlignmentValues::Fill => domain::Alignment::Left,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_alignment_right_is_right() {
        assert_eq!(
            map_alignment(&HorizontalAlignmentValues::Right),
            domain::Alignment::Right
        );
    }

    #[test]
    fn map_alignment_center_variants_are_center() {
        assert_eq!(
            map_alignment(&HorizontalAlignmentValues::Center),
            domain::Alignment::Center
        );
        assert_eq!(
            map_alignment(&HorizontalAlignmentValues::Distributed),
            domain::Alignment::Center
        );
    }

    #[test]
    fn map_alignment_general_and_left_are_left() {
        assert_eq!(
            map_alignment(&HorizontalAlignmentValues::General),
            domain::Alignment::Left
        );
        assert_eq!(
            map_alignment(&HorizontalAlignmentValues::Left),
            domain::Alignment::Left
        );
    }

    #[test]
    fn map_cell_from_default_umya_cell_is_empty() {
        let cell = umya_spreadsheet::Cell::default();
        let mapped = map_cell(&cell, 8.38);
        assert!(mapped.is_empty());
        assert_eq!(mapped.column_width, 8.38);
        assert_eq!(mapped.font, DEFAULT_FONT);
    }
}
