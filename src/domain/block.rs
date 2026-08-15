use super::FontInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq)]
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
        // `col_end >= col_start`はAnalyzerが`Block`を構築する際の契約であり、外部入力を
        // 直接扱うわけではない内部不変条件のため、renderer/flow.mdと同じ方針
        // （契約違反はランタイムのクランプではなくdebug_assertで早期検出する）に従う。
        debug_assert!(
            self.col_end >= self.col_start,
            "Block: col_end must be >= col_start"
        );
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

    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "col_end must be >= col_start")
    )]
    fn span_panics_in_debug_when_col_end_before_col_start() {
        // レビュー指摘: col_end < col_start の不正なBlockに対する`usize`アンダーフロー対策。
        // デバッグビルドではdebug_assertで早期検出する（リリースビルドでの挙動保証はしない）。
        let _ = block(4, 1).span();
    }
}
