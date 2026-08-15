# `domain::document` 設計書

対象: [domain/mod.md](mod.md)の対応表における `document.rs`。

[アーキテクチャ設計書のパイプライン図](../architecture.md#2-パイプライン全体像)は
`Analyzer: Sheet → Vec<Block>` としていたが、Rendererが「表として出力するか、
段落として出力するか」（`RowKind`）を知るには行ごとの分類結果も必要になる。
このファイルは、その分類結果を含めた Analyzer ↔ Renderer 間の境界となる型を定義する。

## 1. `RowKind`

```rust
pub enum RowKind {
    Flow,      // 段落・見出し
    TableRow,  // Markdownテーブルの1行
}
```

`AnalysisStrategy::classify_row`（[アーキテクチャ設計書 4章](../architecture.md#4-analysisstrategy-トレイト)）
の戻り値。

## 2. `ResolvedRow`

```rust
pub struct ResolvedRow<'a> {
    pub blocks: &'a [Block],
}
```

はみ出し判定・ネイティブ結合解決が終わった後、`RowKind`がまだ決まっていない段階の
1行分の `Block` 列。`classify_row` の**入力**として使う。`Block` は [block.md](block.md)参照。

## 3. `RenderedRow` / `Document`

```rust
pub struct RenderedRow {
    pub kind: RowKind,
    pub blocks: Vec<Block>,
}

pub struct Document {
    pub sheet_name: String,
    pub rows: Vec<RenderedRow>,
}
```

`RenderedRow` は `classify_row` 適用**後**の行データで、`Document` は1シート分の
Analyzerの最終出力（Rendererへの入力）。`ResolvedRow`（借用・分類前）と
`RenderedRow`（所有・分類後）は役割が異なるため型を分けている。

## 4. 型の流れ

```
Sheet (domain::sheet)
  │  Analyzer: はみ出し判定・ネイティブ結合解決
  ▼
ResolvedRow<'a> (1行ごと)
  │  AnalysisStrategy::classify_row
  ▼
RenderedRow (kind: RowKind, blocks: Vec<Block>)
  │  集約
  ▼
Document (Renderer への入力)
```
