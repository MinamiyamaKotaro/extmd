//! Excelのシリアル値 → `chrono::NaiveDateTime` の変換、および表示形式コードから
//! 「このセルは日付として扱うべきか」を判定する純粋関数群。
//! umya-spreadsheetの`Cell`/`Worksheet`型には依存しない
//! （docs/design/reader/date.md 1章）。

use chrono::NaiveDateTime;

pub(crate) fn from_serial(serial: f64) -> NaiveDateTime {
    umya_spreadsheet::helper::date::excel_to_date_time_chrono(serial)
}

/// セルの表示形式コードが日付/時刻を表すパターンかどうかを判定する。
///
/// Excelの組み込み日付/時刻フォーマット（`numFmtId` 14〜22, 45〜47相当）に対応する
/// 固定のコード文字列と、`y`/`m`/`d`/`h`/`s`等の日付トークンを含むカスタム書式を
/// 日付とみなす（docs/design/reader/date.md 4章）。
///
/// リテラル文字列区間（`"..."`の内側）とエスケープされた1文字（`\`の直後の1文字）は
/// トークン検出の対象から除外する（同章の「誤検出（偽陽性）のリスク」への対応）。
pub(crate) fn is_date_formatted(format_code: &str) -> bool {
    if format_code.is_empty() || format_code == "General" {
        return false;
    }
    contains_date_token(format_code)
}

fn contains_date_token(format_code: &str) -> bool {
    let mut chars = format_code.chars().peekable();
    let mut in_literal = false;
    while let Some(c) = chars.next() {
        if in_literal {
            if c == '"' {
                in_literal = false;
            }
            continue;
        }
        match c {
            '"' => in_literal = true,
            '\\' => {
                // バックスラッシュエスケープされた次の1文字はトークン判定から除外する。
                chars.next();
            }
            'y' | 'Y' | 'm' | 'M' | 'd' | 'D' | 'h' | 'H' | 's' | 'S' => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_general_are_not_date() {
        assert!(!is_date_formatted(""));
        assert!(!is_date_formatted("General"));
    }

    #[test]
    fn builtin_date_pattern_is_date() {
        assert!(is_date_formatted("yyyy/m/d"));
        assert!(is_date_formatted("h:mm:ss"));
    }

    #[test]
    fn plain_number_pattern_is_not_date() {
        assert!(!is_date_formatted("#,##0.00"));
        assert!(!is_date_formatted("0.0%"));
    }

    #[test]
    fn literal_string_tokens_are_ignored() {
        // `"m"`はリテラル文字列としての単位表記であり、日付トークンではない。
        assert!(!is_date_formatted("0\"m\""));
    }

    #[test]
    fn escaped_tokens_are_ignored() {
        // `\m`はエスケープされた1文字であり、日付トークンとして扱わない。
        assert!(!is_date_formatted("0\\m\\d"));
    }

    #[test]
    fn from_serial_converts_excel_epoch() {
        // Excelのシリアル値1は1900-01-01（1900年うるう年バグ込みの互換動作）。
        let dt = from_serial(1.0);
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "1900-01-01");
    }
}
