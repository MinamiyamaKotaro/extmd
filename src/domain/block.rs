use super::FontInfo;

pub enum BlockSource {
    /// 単独セル（はみ出し・結合いずれもなし）。
    Single,
    /// はみ出し判定により、右方向の空セルを結合した。
    OverflowMerge,
    /// Excelのネイティブ結合セルによるもの。
    NativeMerge,
}

/// Analyzerが`Sheet`のセル群（はみ出し・ネイティブ結合いずれか、または単独セル）から
/// 生成する、変換後の論理的なテキスト単位。
pub struct Block {
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize, // inclusive
    pub text: String,
    pub font: FontInfo,
    pub source: BlockSource,
    /// 見出しレベル（1〜6）。見出しでなければ`None`。
    pub heading_level: Option<u8>,
}

impl Block {
    pub fn span(&self) -> usize {
        self.col_end - self.col_start + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(col_start: usize, col_end: usize) -> Block {
        Block {
            row: 0,
            col_start,
            col_end,
            text: String::new(),
            font: FontInfo {
                size_pt: 11.0,
                bold: false,
            },
            source: BlockSource::Single,
            heading_level: None,
        }
    }

    #[test]
    fn span_single_cell() {
        assert_eq!(block(2, 2).span(), 1);
    }

    #[test]
    fn span_merged_range() {
        assert_eq!(block(1, 4).span(), 4);
    }
}
