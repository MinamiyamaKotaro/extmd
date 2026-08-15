# `domain::block` 設計書

対象: [domain/mod.md](mod.md)の対応表における `block.rs`。

## 1. `BlockSource`

```rust
pub enum BlockSource {
    /// 単独セル（はみ出し・結合いずれもなし）。
    Single,
    /// はみ出し判定により、右方向に `merged_cols` 個の空セルを結合した。
    OverflowMerge { merged_cols: usize },
    /// Excelのネイティブ結合セルによるもの。
    NativeMerge,
}
```

`merged_cols` を持たせるのは、レンダラーが「結合by overflow」と「結合byネイティブ結合」を
区別して出力を変えたい場合（例: デバッグ出力、`--no-overflow-merge` との差分表示）に
備えるため。v1のMarkdown出力自体は `BlockSource` を分岐しない想定だが、型としては保持しておく。

## 2. `Block`

```rust
pub struct Block {
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize, // inclusive
    pub text: String,
    pub font: FontInfo,
    pub source: BlockSource,
}

impl Block {
    pub fn span(&self) -> usize {
        self.col_end - self.col_start + 1
    }
}
```

`Block` は Analyzer が `Sheet` のセル群（はみ出し・ネイティブ結合いずれか、または単独セル）
から生成する、変換後の論理的なテキスト単位。`font: FontInfo`（[cell.md](cell.md)参照）は
`AnalysisStrategy::heading_level` の判定に使う。

座標フィールド（`row`/`col_start`/`col_end`）の型は
[mod.md 3章](mod.md#3-座標の表現rowindexcolindexの検討)の方針に従い、素の `usize` とする。
