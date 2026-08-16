//! 設計方針・公開API(`render`/`OutputTarget`)・`Document`から本文への組み立て
//! (docs/design/renderer/mod.md)。
//!
//! `renderer`は`domain`にのみ依存する(`analysis`には依存しない)。内部モジュール
//! (`flow`/`table`/`escape`/`output`)は`renderer`外部には公開しない。

mod escape;
mod flow;
mod output;
mod table;

use crate::domain;

#[derive(Debug)]
pub enum OutputTarget {
    Stdout,
    SingleFile(std::path::PathBuf),
    SplitDirectory(std::path::PathBuf),
}

#[derive(Debug)]
pub enum RendererError {
    Io(std::io::Error),
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RendererError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RendererError {}

/// `Analyzer`が生成した各シートの`Document`をMarkdownへ変換し、`target`へ書き出す。
/// `target`の構築(CLI引数の解釈)は呼び出し側(`lib.rs`)の責務とし、`render`は既に
/// 決定済みの`OutputTarget`を受け取るだけとする。
pub fn render(documents: &[domain::Document], target: OutputTarget) -> Result<(), RendererError> {
    let heading_offset = heading_offset_for(&target);

    match target {
        OutputTarget::Stdout => {
            output::write_stdout(&render_concatenated(documents, heading_offset))
        }
        OutputTarget::SingleFile(path) => {
            output::write_single_file(&path, &render_concatenated(documents, heading_offset))
        }
        OutputTarget::SplitDirectory(dir) => {
            let bodies = documents
                .iter()
                .map(|doc| (doc.sheet_name.clone(), render_body(doc, heading_offset)))
                .collect();
            output::write_split(&dir, bodies)
        }
    }
}

/// `Block.heading_level`はシート名の存在を知らずに算出されるため、複数シートを1つの
/// Markdownへ連結する出力ではシート名にH1を占有させ、本文の見出しは+1オフセットして
/// H2始まりにする。`SplitDirectory`はファイル名自体がシートタイトルの役割を果たすため
/// オフセットなし。
fn heading_offset_for(target: &OutputTarget) -> u8 {
    match target {
        OutputTarget::Stdout | OutputTarget::SingleFile(_) => 1,
        OutputTarget::SplitDirectory(_) => 0,
    }
}

fn render_concatenated(documents: &[domain::Document], heading_offset: u8) -> String {
    documents
        .iter()
        .map(|doc| {
            let heading = format!("# {}", escape::escape_flow_text(&doc.sheet_name));
            format!("{heading}\n\n{}", render_body(doc, heading_offset))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `doc.rows`(`RowKind`混在)を、連続する`TableRow`をひとまとまりのテーブルとして
/// グループ化しながら本文Markdownへ変換する。
fn render_body(doc: &domain::Document, heading_offset: u8) -> String {
    let mut parts = Vec::new();
    let mut i = 0;

    while i < doc.rows.len() {
        match doc.rows[i].kind {
            domain::RowKind::Flow => {
                // 全セルが空の行（blocksが空）は、GridPaperStrategy/TabularStrategyの
                // classify_row（PR #21/#38）によりFlowに分類されるが、render_rowは
                // 常に空文字列を返す。これをそのままpartsに積むと、実データの範囲外まで
                // 広い使用範囲（used range）を持つシート（例: 書式だけが遠くの行まで
                // 適用されている実務ファイル）で、大量の空文字列partsが`\n\n`で
                // 連結され無意味な空行の羅列になってしまうため、何も出力しない行は
                // partに積まない。
                if !doc.rows[i].blocks.is_empty() {
                    parts.push(flow::render_row(&doc.rows[i], heading_offset));
                }
                i += 1;
            }
            domain::RowKind::TableRow => {
                let start = i;
                while i < doc.rows.len() && matches!(doc.rows[i].kind, domain::RowKind::TableRow) {
                    i += 1;
                }
                parts.push(table::render_table(&doc.rows[start..i]));
            }
        }
    }

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow_block(text: &str, heading_level: Option<u8>) -> domain::Block {
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

    fn table_block(col_start: usize, col_end: usize, text: &str) -> domain::Block {
        domain::Block {
            row: 0,
            col_start,
            col_end,
            text: text.into(),
            font: domain::FontInfo {
                size_pt: 11.0,
                bold: false,
            },
            source: domain::BlockSource::Single,
            heading_level: None,
        }
    }

    #[test]
    fn heading_offset_is_one_for_concatenated_targets() {
        assert_eq!(heading_offset_for(&OutputTarget::Stdout), 1);
        assert_eq!(
            heading_offset_for(&OutputTarget::SingleFile("x.md".into())),
            1
        );
    }

    #[test]
    fn heading_offset_is_zero_for_split_directory() {
        assert_eq!(
            heading_offset_for(&OutputTarget::SplitDirectory("out".into())),
            0
        );
    }

    #[test]
    fn render_body_groups_consecutive_table_rows_and_interleaves_flow() {
        let doc = domain::Document {
            sheet_name: "Sheet1".into(),
            rows: vec![
                domain::RenderedRow {
                    kind: domain::RowKind::Flow,
                    blocks: vec![flow_block("Title", Some(1))],
                },
                domain::RenderedRow {
                    kind: domain::RowKind::TableRow,
                    blocks: vec![table_block(0, 0, "a")],
                },
                domain::RenderedRow {
                    kind: domain::RowKind::TableRow,
                    blocks: vec![table_block(0, 0, "1")],
                },
                domain::RenderedRow {
                    kind: domain::RowKind::Flow,
                    blocks: vec![flow_block("footer", None)],
                },
            ],
        };

        let body = render_body(&doc, 0);
        assert_eq!(body, "# Title\n\n| a |\n|---|\n| 1 |\n\nfooter");
    }

    #[test]
    fn render_body_empty_document_is_empty_string() {
        let doc = domain::Document {
            sheet_name: "Empty".into(),
            rows: vec![],
        };
        assert_eq!(render_body(&doc, 0), "");
    }

    /// 実データで判明した回帰: Excelの使用範囲(used range)が実データより大幅に広い
    /// シート（書式だけが遠くの行まで適用されている実務ファイル等）では、実データを
    /// 超えた範囲の全行が完全に空の`Flow`行（blocksが空、PR #21/#38）になる。
    /// これらを1行ごとに空文字列のpartとしてpushしてしまうと、大量の空行が
    /// `\n\n`で連結され出力される（大量の空行の中に実質何も情報が無いため、
    /// ユーザーからは「何も返ってこない」ように見える）。
    #[test]
    fn render_body_skips_consecutive_fully_empty_flow_rows() {
        let empty_row = || domain::RenderedRow {
            kind: domain::RowKind::Flow,
            blocks: vec![],
        };
        let doc = domain::Document {
            sheet_name: "S".into(),
            rows: vec![
                domain::RenderedRow {
                    kind: domain::RowKind::Flow,
                    blocks: vec![flow_block("content", None)],
                },
                empty_row(),
                empty_row(),
                empty_row(),
            ],
        };
        assert_eq!(render_body(&doc, 0), "content");
    }

    #[test]
    fn render_concatenated_inserts_sheet_name_as_h1_heading() {
        let doc = domain::Document {
            sheet_name: "My Sheet".into(),
            rows: vec![domain::RenderedRow {
                kind: domain::RowKind::Flow,
                blocks: vec![flow_block("body", None)],
            }],
        };
        let out = render_concatenated(std::slice::from_ref(&doc), 1);
        assert_eq!(out, "# My Sheet\n\nbody");
    }

    #[test]
    fn render_writes_to_stdout_target_without_error() {
        let doc = domain::Document {
            sheet_name: "S".into(),
            rows: vec![],
        };
        assert!(render(&[doc], OutputTarget::Stdout).is_ok());
    }
}
