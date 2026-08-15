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

## 5. `ResolvedRow`（借用）→`RenderedRow`（所有）間で`clone`は発生しない

一見、`ResolvedRow<'a> { blocks: &'a [Block] }` から `RenderedRow { blocks: Vec<Block> }` を
作る際に `Block`（`text: String` を含む）の `clone()` が必要に思えるが、**実際には不要**。
Analyzerの実装は以下のパターンになる想定で、NLL（non-lexical lifetimes）により
`resolved` の借用は `classify_row` 呼び出しの直後で終了するため、その後に元の
所有 `Vec<Block>` をそのまま `RenderedRow` へムーブできる。

```rust
let blocks: Vec<Block> = /* はみ出し判定・結合解決の結果（所有） */;
let resolved = ResolvedRow { blocks: &blocks };
let kind = strategy.classify_row(&resolved);
// `resolved` の最終利用はここまで。NLLにより借用はここで終わるため、
// 以下の `blocks` のムーブに clone() は不要。
RenderedRow { kind, blocks }
```

（[PR #3のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/3#issuecomment-5301568286)で
提起されたパフォーマンス懸念を検証した結果。最小再現コードでコンパイル・実行して確認済み）
