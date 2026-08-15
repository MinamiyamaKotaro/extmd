//! Markdown特殊文字のエスケープ純粋関数群（docs/design/renderer/escape.md）。

/// テーブルセル内のテキスト用エスケープ。置換順序が重要（後続の置換が、直前の置換で
/// 生成した文字を誤って再エスケープ/破壊しないようにするため）。
pub(in crate::renderer) fn escape_table_cell(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', "<br>")
}

/// Flowテキスト（段落・見出し）用エスケープ。1文字ずつ走査するため、
/// `escape_table_cell`のような複数回`replace`呼び出しの順序問題は起きない。
pub(in crate::renderer) fn escape_flow_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(
            c,
            '\\' | '*' | '_' | '`' | '[' | ']' | '#' | '&' | '<' | '>'
        ) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped.replace('\n', "  \n")
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
}
