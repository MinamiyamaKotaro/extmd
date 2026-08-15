use super::Block;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// 段落・見出しとして出力する。
    Flow,
    /// Markdownテーブルの1行として出力する。
    TableRow,
}

/// はみ出し判定・ネイティブ結合解決が終わった後、`RowKind`がまだ決まっていない段階の
/// 1行分の`Block`列。`AnalysisStrategy::classify_row`の入力として使う。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedRow<'a> {
    pub blocks: &'a [Block],
}

/// `classify_row`適用後の行データ。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedRow {
    pub kind: RowKind,
    pub blocks: Vec<Block>,
}

/// Analyzerの最終出力（Rendererへの入力）。
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub sheet_name: String,
    pub rows: Vec<RenderedRow>,
}
