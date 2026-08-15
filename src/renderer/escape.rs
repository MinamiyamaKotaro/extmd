//! Markdown特殊文字のエスケープ純粋関数群（docs/design/renderer/escape.md）。

/// CRLF (`\r\n`) をLF (`\n`) に正規化する。Windows環境や一部のライブラリで作成された
/// `.xlsx`はセル内改行を`\r\n`で格納している場合があり、これを考慮せず`\n`のみを
/// 変換対象にすると`\r`が残留し表示崩れの原因になる（PR #22レビューコメントでの指摘を反映）。
fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// テーブルセル内のテキスト用エスケープ。1文字ずつの単一パス走査で変換するため、
/// 複数回`replace`を呼ぶ場合の置換順序への依存（後続の置換が直前の置換で生成した
/// 文字を誤って再エスケープ/破壊する問題）が構造的に発生しない
/// （PR #22レビューコメントでの指摘を反映。ヒープ確保も1回で済む）。
pub(in crate::renderer) fn escape_table_cell(text: &str) -> String {
    let normalized = normalize_line_endings(text);
    let mut escaped = String::with_capacity(normalized.len());
    for c in normalized.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\\' => escaped.push_str("\\\\"),
            '|' => escaped.push_str("\\|"),
            '\n' => escaped.push_str("<br>"),
            _ => escaped.push(c),
        }
    }
    escaped
}

/// Flowテキスト（段落・見出し）用エスケープ。1文字ずつの単一パス走査で変換する。
pub(in crate::renderer) fn escape_flow_text(text: &str) -> String {
    let normalized = normalize_line_endings(text);
    let mut escaped = String::with_capacity(normalized.len());
    for c in normalized.chars() {
        match c {
            '\\' | '*' | '_' | '`' | '[' | ']' | '#' | '&' | '<' | '>' => {
                escaped.push('\\');
                escaped.push(c);
            }
            '\n' => escaped.push_str("  \n"),
            _ => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_table_cell_replaces_pipe_and_newline() {
        assert_eq!(escape_table_cell("a|b\nc"), "a\\|b<br>c");
    }

    #[test]
    fn escape_table_cell_escapes_backslash_before_pipe() {
        // バックスラッシュ→パイプの順で置換しないと、パイプのエスケープで挿入した
        // `\`自身を二重にエスケープしてしまう。
        assert_eq!(escape_table_cell("a\\|b"), "a\\\\\\|b");
    }

    #[test]
    fn escape_table_cell_escapes_html_entities_before_br_insertion() {
        assert_eq!(escape_table_cell("<b>\n"), "&lt;b&gt;<br>");
    }

    #[test]
    fn escape_table_cell_escapes_ampersand_before_entity_generation() {
        // `&`を先に置換しないと、`<`/`>`の置換で挿入した`&lt;`/`&gt;`自身の`&`を
        // 二重エスケープしてしまう。
        assert_eq!(escape_table_cell("&"), "&amp;");
        assert_eq!(escape_table_cell("<"), "&lt;");
    }

    #[test]
    fn escape_flow_text_escapes_markdown_control_chars() {
        assert_eq!(
            escape_flow_text("*bold* _em_ `code`"),
            "\\*bold\\* \\_em\\_ \\`code\\`"
        );
    }

    #[test]
    fn escape_flow_text_escapes_heading_and_brackets() {
        assert_eq!(escape_flow_text("# [link]"), "\\# \\[link\\]");
    }

    #[test]
    fn escape_flow_text_escapes_html_special_chars() {
        assert_eq!(
            escape_flow_text("<script>&</script>"),
            "\\<script\\>\\&\\</script\\>"
        );
    }

    #[test]
    fn escape_flow_text_converts_newline_to_hard_break() {
        assert_eq!(escape_flow_text("a\nb"), "a  \nb");
    }

    #[test]
    fn escape_flow_text_escapes_backslash_itself() {
        assert_eq!(escape_flow_text("a\\b"), "a\\\\b");
    }

    #[test]
    fn escape_table_cell_normalizes_crlf_before_converting_to_br() {
        assert_eq!(escape_table_cell("a\r\nb"), "a<br>b");
    }

    #[test]
    fn escape_flow_text_normalizes_crlf_before_converting_to_hard_break() {
        assert_eq!(escape_flow_text("a\r\nb"), "a  \nb");
    }
}
