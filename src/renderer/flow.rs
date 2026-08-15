//! `RowKind::Flow`行の変換（段落・見出し）（docs/design/renderer/flow.md）。

use crate::domain;

use super::escape;

/// `RenderedRow.blocks`は`RowKind::Flow`であっても複数の`Block`を持ちうるため、
/// 1行にまとめず`Block`ごとに1行として出力する（複数ブロックを1行に連結すると、
/// 先頭以外のブロックの`heading_level`を表現できなくなるため）。
pub(in crate::renderer) fn render_row(row: &domain::RenderedRow, heading_offset: u8) -> String {
    row.blocks
        .iter()
        .map(|block| render_block(block, heading_offset))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_block(block: &domain::Block, heading_offset: u8) -> String {
    let text = escape::escape_flow_text(&block.text);

    match block.heading_level {
        Some(level) => {
            // domain/block.mdの契約によりheading_levelは常に1..=6。契約違反はレンダラー側で
            // クランプ等のフォールバックを行わず、デバッグビルドで早期検出する。
            debug_assert!((1..=6).contains(&level), "heading_level must be 1..=6");
            let level = (level + heading_offset).min(6);
            format!("{} {text}", "#".repeat(level as usize))
        }
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(text: &str, heading_level: Option<u8>) -> domain::Block {
        domain::Block {
            row: 0,
            col_start: 0,
            col_end: 0,
            text: text.into(),
            font: domain::FontInfo {
                size_pt: 11.0,
                bold: false,
            },
            source: domain::BlockSource::Single,
            heading_level,
        }
    }

    #[test]
    fn render_row_joins_multiple_blocks_with_newline() {
        let blocks = vec![block("a", None), block("b", None)];
        let row = domain::RenderedRow {
            kind: domain::RowKind::Flow,
            blocks,
        };
        assert_eq!(render_row(&row, 0), "a\nb");
    }

    #[test]
    fn render_row_renders_paragraph_without_heading_level() {
        let row = domain::RenderedRow {
            kind: domain::RowKind::Flow,
            blocks: vec![block("plain text", None)],
        };
        assert_eq!(render_row(&row, 0), "plain text");
    }

    #[test]
    fn render_row_renders_heading_with_hash_prefix() {
        let row = domain::RenderedRow {
            kind: domain::RowKind::Flow,
            blocks: vec![block("Title", Some(1))],
        };
        assert_eq!(render_row(&row, 0), "# Title");
    }

    #[test]
    fn render_row_applies_heading_offset() {
        let row = domain::RenderedRow {
            kind: domain::RowKind::Flow,
            blocks: vec![block("Title", Some(1))],
        };
        assert_eq!(render_row(&row, 1), "## Title");
    }

    #[test]
    fn render_row_clamps_heading_level_at_six() {
        let row = domain::RenderedRow {
            kind: domain::RowKind::Flow,
            blocks: vec![block("Deep", Some(6))],
        };
        assert_eq!(render_row(&row, 1), "###### Deep");
    }

    #[test]
    fn render_row_escapes_block_text() {
        let row = domain::RenderedRow {
            kind: domain::RowKind::Flow,
            blocks: vec![block("*emph*", None)],
        };
        assert_eq!(render_row(&row, 0), "\\*emph\\*");
    }
}
