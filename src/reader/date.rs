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
/// リテラル文字列区間（`"..."`の内側）・エスケープされた1文字（`\`の直後の1文字）・
/// 角括弧区間（`[...]`、色指定`[Red]`やロケール指定`[$-de-DE]`等。経過時間書式
/// `[h]`/`[hh]`/`[m]`/`[mm]`/`[s]`/`[ss]`のみ例外的に日付トークンとして扱う）は
/// トークン検出の対象から除外する（同章の「誤検出（偽陽性）のリスク」への対応）。
pub(crate) fn is_date_formatted(format_code: &str) -> bool {
    if format_code.is_empty() || format_code == "General" {
        return false;
    }
    contains_date_token(format_code)
}

/// 経過時間書式（`[h]`等）として認識する角括弧内容（大文字小文字は区別しない）。
const ELAPSED_TIME_BRACKET_TOKENS: [&str; 6] = ["h", "hh", "m", "mm", "s", "ss"];

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
            '[' => {
                // 色指定（`[Red]`）・ロケール指定（`[$-de-DE]`）等は、"Red"の`d`や
                // "Magenta"の`M`のようにたまたま日付トークンに見える文字を含みうるため、
                // 角括弧内は経過時間書式（`[h]`等）と完全一致する場合のみ日付トークンと
                // みなし、それ以外は内容ごと無視する。
                let mut bracket_content = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == ']' {
                        closed = true;
                        break;
                    }
                    bracket_content.push(next);
                }
                if closed
                    && ELAPSED_TIME_BRACKET_TOKENS
                        .contains(&bracket_content.to_ascii_lowercase().as_str())
                {
                    return true;
                }
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
    fn color_bracket_tokens_are_ignored() {
        // レビュー指摘: "[Red]"の`d`・"[Magenta]"の`M`を日付トークンと誤検出しない。
        assert!(!is_date_formatted("[Red]0.00"));
        assert!(!is_date_formatted("[Magenta]0.00"));
    }

    #[test]
    fn locale_bracket_tokens_are_ignored() {
        // レビュー指摘: ロケール指定 "[$-de-DE]"の`d`・"[$-sv-SE]"の`s`を誤検出しない。
        assert!(!is_date_formatted("[$-de-DE]0.00"));
        assert!(!is_date_formatted("[$-sv-SE]0.00"));
    }

    #[test]
    fn elapsed_time_bracket_tokens_are_dates() {
        // `[h]`/`[mm]`/`[ss]`は経過時間書式であり、これ自体は日付トークンとして扱う
        // （角括弧の外に他の日付トークン文字を含まない形で単体検証する）。
        assert!(is_date_formatted("[h]"));
        assert!(is_date_formatted("[mm]"));
        assert!(is_date_formatted("[ss]"));
    }

    #[test]
    fn from_serial_converts_excel_epoch() {
        // Excelのシリアル値1は1900-01-01（1900年うるう年バグ込みの互換動作）。
        let dt = from_serial(1.0);
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "1900-01-01");
    }
}
