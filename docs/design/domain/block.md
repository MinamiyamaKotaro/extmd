# `domain::block` 設計書

対象: [domain/mod.md](mod.md)の対応表における `block.rs`。

## 1. `BlockSource`

```rust
pub enum BlockSource {
    /// 単独セル（はみ出し・結合いずれもなし）。
    Single,
    /// はみ出し判定により、右方向の空セルを結合した。
    OverflowMerge,
    /// Excelのネイティブ結合セルによるもの。
    NativeMerge,
}
```

`OverflowMerge`/`NativeMerge` を分けているのは、レンダラーが「結合by overflow」と
「結合byネイティブ結合」を区別して出力を変えたい場合（例: デバッグ出力、
`--no-overflow-merge` との差分表示）に備えるため。v1のMarkdown出力自体は
`BlockSource` を分岐しない想定だが、型としては保持しておく。

**設計判断: 結合セル数はフィールドとして持たない。** 当初 `OverflowMerge { merged_cols: usize }`
のように結合したセル数を持たせる案だったが、その値は常に `Block` 自身の
`col_start`/`col_end` から `span() - 1` として一意に導出できるため、フィールドとして
二重に保持すると値がずれる不整合リスクがある。結合セル数が必要な場面では
`block.span() - 1` を呼ぶ（[PR #3のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/3#issuecomment-5301554119)での指摘を反映）。

## 2. `Block`

```rust
pub struct Block {
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize, // inclusive
    pub text: String,
    pub font: FontInfo,
    pub source: BlockSource,
    pub heading_level: Option<u8>, // 見出しレベル（1〜6）。見出しでなければ None
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

**`heading_level`フィールドについて:** アーキテクチャ設計書2章の「Rendererは`analysis`
（Strategy）に依存しない」という方針により、`AnalysisStrategy::heading_level`の判定結果を
Renderer到達前（Analyzer内）に確定させて`Block`へ保持しておく必要がある。
（[analysis層の詳細設計](../analysis/mod.md#5-domain層への変更-blockheading_level-の追加)で決定。
Analyzerがこのフィールドを持たないと、Rendererが見出し出力のために`AnalysisStrategy`を
直接呼ぶことになり、依存方向の方針に反してしまう。）
