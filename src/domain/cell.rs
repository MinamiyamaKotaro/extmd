use chrono::{Datelike, NaiveDateTime, Timelike};

#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Empty,
    String(String),
    Number(f64),
    Date(NaiveDateTime),
    Bool(bool),
}

impl CellValue {
    pub fn is_empty(&self) -> bool {
        matches!(self, CellValue::Empty)
    }

    /// Markdown出力やはみ出し幅推定に使う文字列表現。表示形式コード（`number_format`）を
    /// 考慮しない、コンテキストフリーの既定フォーマット。`number_format`を考慮した
    /// フォーマットが必要な場合は`Cell::display_text()`を使う（`number_format`は
    /// `CellValue`ではなく`Cell`が持つフィールドのため、ここでは参照できない）。
    pub fn display_text(&self) -> String {
        match self {
            CellValue::Empty => String::new(),
            CellValue::String(s) => s.clone(),
            CellValue::Number(n) => format_number(*n, None),
            CellValue::Date(d) => format_date(*d, None),
            CellValue::Bool(b) => {
                if *b {
                    "TRUE".into()
                } else {
                    "FALSE".into()
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontInfo {
    pub size_pt: f32,
    pub bold: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub value: CellValue,
    /// 列幅（文字数換算）。
    pub column_width: f64,
    pub wrap_text: bool,
    pub alignment: Alignment,
    pub font: FontInfo,
    /// 表示形式コード（例: `"#,##0"`, `"yyyy/m/d"`）。未設定なら`None`。
    pub number_format: Option<String>,
}

impl Cell {
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// `number_format`（表示形式コード）を考慮したMarkdown出力用の文字列表現。
    /// 桁区切り・小数点以下桁数・パーセンテージ・基本的な日付/時刻の書式に対応する
    /// （docs/design/domain/cell.md 4章の「必須（v1スコープ）」の範囲）。
    /// 和暦・通貨記号等ロケール依存書式は[Issue #2](https://github.com/MinamiyamaKotaro/extmd/issues/2)
    /// のスパイク検証の結果`ssfmt`クレートをv1では不採用と決定したためv1スコープ外のまま
    /// 未対応で、該当する場合は表示形式を無視した既定フォーマットにフォールバックする。
    pub fn display_text(&self) -> String {
        let format_code = self.number_format.as_deref();
        match &self.value {
            CellValue::Number(n) => format_number(*n, format_code),
            CellValue::Date(d) => format_date(*d, format_code),
            _ => self.value.display_text(),
        }
    }
}

/// 数値の表示形式コードを適用する。対応するのは「必須」スコープのみ:
/// 桁区切り（`,`を含む書式）・小数点以下桁数（`.`以降の`0`/`#`の個数）・
/// パーセンテージ（`%`を含む書式）。それ以外の記号（通貨記号・条件付き書式の
/// セクション分岐等）はストリップ（無視）する。
fn format_number(n: f64, format_code: Option<&str>) -> String {
    let code = match format_code {
        Some(code) if !code.is_empty() && code != "General" => code,
        _ => return format_general_number(n),
    };

    let is_percentage = code.contains('%');
    let value = if is_percentage { n * 100.0 } else { n };
    let decimal_places = count_decimal_places(code);
    let mut formatted = format!("{value:.decimal_places$}");
    if code.contains(',') {
        formatted = insert_thousands_separators(&formatted);
    }
    if is_percentage {
        formatted.push('%');
    }
    formatted
}

/// `format!("{value:.N$}")`に渡す小数点以下桁数の上限。`f64`の有効桁数（約17桁）を
/// 大きく超える桁数を指定してもゼロ埋めが増えるだけで意味がなく、悪意ある/破損した
/// `.xlsx`が極端に長い表示形式コード（例: `0`が数万文字連続する文字列）を埋め込んで
/// 巨大な文字列確保によるメモリ枯渇 (DoS) を引き起こすことを防ぐガード
/// （docs/security/design-review.md #2と同種の「入力ファイルの内容に起因するリソース
/// 消費を上限で防ぐ」方針に従う）。
const MAX_DECIMAL_PLACES: usize = 20;

fn count_decimal_places(code: &str) -> usize {
    match code.split_once('.') {
        Some((_, frac)) => frac
            .chars()
            .take_while(|c| *c == '0' || *c == '#')
            .count()
            .min(MAX_DECIMAL_PLACES),
        None => 0,
    }
}

fn insert_thousands_separators(s: &str) -> String {
    let negative = s.starts_with('-');
    let unsigned = s.strip_prefix('-').unwrap_or(s);
    let (int_part, frac_part) = unsigned.split_once('.').unwrap_or((unsigned, ""));

    let mut grouped: Vec<char> = Vec::with_capacity(int_part.len() + int_part.len() / 3);
    for (i, c) in int_part.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    grouped.reverse();

    let mut result = String::new();
    if negative {
        result.push('-');
    }
    result.extend(grouped);
    if !frac_part.is_empty() {
        result.push('.');
        result.push_str(frac_part);
    }
    result
}

/// 表示形式コードが未設定（`None`または`"General"`）の場合のフォールバック。
/// 整数値は小数点なしで、それ以外はRustの既定浮動小数点表示に委ねる。
fn format_general_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{n:.0}")
    } else {
        format!("{n}")
    }
}

/// 日付/時刻の表示形式コードを適用する。対応するのは「必須」スコープの基本トークン
/// （`y`/`m`/`d`/`h`/`s`の連続文字数による0埋め桁数指定）のみ。`m`は月/分の両方を
/// 表しうるが、直前に`h`（時）系トークンが出現していれば分、そうでなければ月として扱う
/// （docs/design/reader/date.md 4章と同じ制約。`mm:ss`のように`hh`を伴わない
/// 分:秒表記は月として誤判定するのが既知の制約）。
fn format_date(d: NaiveDateTime, format_code: Option<&str>) -> String {
    match format_code {
        Some(code) if !code.is_empty() && code != "General" => render_date_tokens(d, code),
        _ => d.format("%Y/%m/%d %H:%M").to_string(),
    }
}

fn render_date_tokens(d: NaiveDateTime, code: &str) -> String {
    let chars: Vec<char> = code.chars().collect();
    let mut out = String::new();
    let mut in_time_context = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let run_len = chars[i..].iter().take_while(|&&ch| ch == c).count();
        match c {
            'y' | 'Y' => {
                if run_len >= 4 {
                    out.push_str(&format!("{:04}", d.year()));
                } else {
                    out.push_str(&format!("{:02}", d.year().rem_euclid(100)));
                }
                in_time_context = false;
            }
            'd' | 'D' => {
                if run_len >= 2 {
                    out.push_str(&format!("{:02}", d.day()));
                } else {
                    out.push_str(&d.day().to_string());
                }
                in_time_context = false;
            }
            'h' | 'H' => {
                if run_len >= 2 {
                    out.push_str(&format!("{:02}", d.hour()));
                } else {
                    out.push_str(&d.hour().to_string());
                }
                in_time_context = true;
            }
            's' | 'S' => {
                if run_len >= 2 {
                    out.push_str(&format!("{:02}", d.second()));
                } else {
                    out.push_str(&d.second().to_string());
                }
                in_time_context = true;
            }
            'm' | 'M' => {
                if in_time_context {
                    if run_len >= 2 {
                        out.push_str(&format!("{:02}", d.minute()));
                    } else {
                        out.push_str(&d.minute().to_string());
                    }
                } else if run_len >= 2 {
                    out.push_str(&format!("{:02}", d.month()));
                } else {
                    out.push_str(&d.month().to_string());
                }
            }
            other => {
                for _ in 0..run_len {
                    out.push(other);
                }
            }
        }
        i += run_len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn default_font() -> FontInfo {
        FontInfo {
            size_pt: 11.0,
            bold: false,
        }
    }

    fn cell(value: CellValue, number_format: Option<&str>) -> Cell {
        Cell {
            value,
            column_width: 8.38,
            wrap_text: false,
            alignment: Alignment::default(),
            font: default_font(),
            number_format: number_format.map(str::to_string),
        }
    }

    #[test]
    fn cell_value_is_empty() {
        assert!(CellValue::Empty.is_empty());
        assert!(!CellValue::Number(0.0).is_empty());
    }

    #[test]
    fn display_text_bool() {
        assert_eq!(CellValue::Bool(true).display_text(), "TRUE");
        assert_eq!(CellValue::Bool(false).display_text(), "FALSE");
    }

    #[test]
    fn number_general_format_strips_trailing_zero() {
        let c = cell(CellValue::Number(42.0), None);
        assert_eq!(c.display_text(), "42");
    }

    #[test]
    fn number_thousands_and_decimal_places() {
        let c = cell(CellValue::Number(1234567.891), Some("#,##0.00"));
        assert_eq!(c.display_text(), "1,234,567.89");
    }

    #[test]
    fn number_percentage() {
        let c = cell(CellValue::Number(0.4567), Some("0.0%"));
        assert_eq!(c.display_text(), "45.7%");
    }

    #[test]
    fn number_format_with_excessive_decimal_places_is_capped() {
        // レビュー指摘: 小数点以下の`0`が極端に長い(悪意ある/破損した)number_formatを
        // そのまま`format!("{value:.N$}")`に渡すと、Nに比例した巨大な文字列を
        // 確保してしまう(DoSリスク)。MAX_DECIMAL_PLACESでクランプされることを確認する。
        let huge_format = format!("0.{}", "0".repeat(100_000));
        let c = cell(CellValue::Number(1.5), Some(&huge_format));
        let text = c.display_text();
        assert_eq!(text.len(), "1.".len() + MAX_DECIMAL_PLACES);
        assert!(text.starts_with("1.5"));
    }

    #[test]
    fn number_negative_thousands_separator() {
        let c = cell(CellValue::Number(-1234.5), Some("#,##0.0"));
        assert_eq!(c.display_text(), "-1,234.5");
    }

    #[test]
    fn date_default_format() {
        let dt = NaiveDate::from_ymd_opt(2026, 8, 15)
            .unwrap()
            .and_hms_opt(21, 53, 0)
            .unwrap();
        let c = cell(CellValue::Date(dt), None);
        assert_eq!(c.display_text(), "2026/08/15 21:53");
    }

    #[test]
    fn date_basic_pattern() {
        let dt = NaiveDate::from_ymd_opt(2026, 8, 5)
            .unwrap()
            .and_hms_opt(9, 5, 0)
            .unwrap();
        let c = cell(CellValue::Date(dt), Some("yyyy/mm/dd hh:mm"));
        assert_eq!(c.display_text(), "2026/08/05 09:05");
    }

    #[test]
    fn date_month_vs_minute_disambiguation() {
        // "mm" before "hh" is a month; "mm" after "hh" is minutes.
        let dt = NaiveDate::from_ymd_opt(2026, 3, 1)
            .unwrap()
            .and_hms_opt(14, 7, 0)
            .unwrap();
        let c = cell(CellValue::Date(dt), Some("mm/dd hh:mm"));
        assert_eq!(c.display_text(), "03/01 14:07");
    }

    #[test]
    fn date_preserves_repeated_literal_separators() {
        // レビュー指摘: `render_date_tokens`のリテラル文字（トークン以外）分岐は
        // 連続した同一文字（例: `--`）の2文字目以降を読み飛ばすバグがあった。
        let dt = NaiveDate::from_ymd_opt(2026, 8, 15)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let c = cell(CellValue::Date(dt), Some("yyyy--mm--dd"));
        assert_eq!(c.display_text(), "2026--08--15");
    }
}
