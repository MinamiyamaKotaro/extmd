# `analysis::strategy` 設計書

対象: [analysis/mod.md](mod.md)の対応表における `strategy.rs`。

## 1. `OverflowContext` / `OverflowDecision` の配置場所についての設計判断

アーキテクチャ設計書3章「コアドメイン型」では`OverflowContext`/`OverflowDecision`を
`Sheet`/`Cell`等と並べて掲載していたが、[domain設計書 mod.md 1章](../domain/mod.md#1-対応表)の
対応表にこの2型は含まれていない。改めて精査した結果、両型は
`AnalysisStrategy::detect_overflow`のためだけに存在する入出力データであり、
`domain`側の型（`Sheet`/`Cell`/`Block`等）のようにreader/analysis/renderer全層で
共有されるコア型ではないため、本設計では**`analysis::strategy`（このファイル）に定義する**
方針とする。

`domain`の依存方向（[domain/mod.md 2章](../domain/mod.md#2-設計方針)）とも整合する:
`OverflowContext<'a> { source: &'a Cell, .. }`は`domain::Cell`を参照するだけなので、
analysis→domainの一方向依存のまま成立する。

（この配置変更はアーキテクチャ設計書3章にも反映済み）

```rust
/// はみ出し判定の対象となる1セルとその右方向の空セル列。
pub struct OverflowContext<'a> {
    pub source: &'a domain::Cell,
    pub following_empty_cells: &'a [domain::Cell], // 右隣から連続する空セルのみ（mod.md 4章）
}

pub enum OverflowDecision {
    /// はみ出しなし。単独セルとして扱う。
    NoMerge,
    /// 右方向に `count` 個の空セルまで結合する。
    MergeCells { count: usize },
}
```

## 2. `AnalysisStrategy` トレイト

```rust
pub trait AnalysisStrategy {
    /// CLIの `--strategy` で指定するための識別子（例: "grid-paper", "tabular"）。
    fn id(&self) -> &'static str;

    /// このシートに対して自身がどの程度適合しそうかを返す（0.0〜1.0）。
    /// `metrics` は `StrategyRegistry::select_auto` が一度だけ計算して
    /// 全戦略に配る（registry.md 2章）。
    fn affinity(&self, sheet: &domain::Sheet, metrics: &metrics::SheetMetrics) -> f32;

    /// はみ出し判定: 対象セルを右方向の空セルへどこまで結合するか決定する。
    fn detect_overflow(&self, ctx: &OverflowContext) -> OverflowDecision;

    /// 解決済みの1行が「表の行」か「文章の流れ」かを分類する。
    fn classify_row(&self, row: &domain::ResolvedRow) -> domain::RowKind;

    /// ブロックの書式情報から見出しレベル（1〜6）を判定する。見出しでなければ None。
    fn heading_level(&self, block: &domain::Block) -> Option<u8>;
}
```

- トレイトの可視性は`pub`とする。`StrategyRegistry::select_auto`/`get`
  （[registry.md](registry.md)）の戻り値`&dyn AnalysisStrategy`が`analysis`モジュール外
  （`lib.rs`）まで渡るため、この型は必然的にクレート全体に公開される。
- `metrics: &SheetMetrics`引数の型`SheetMetrics`自体も、同じ理由で`pub`とする。
  一方でフィールドと構築関数（`compute_sheet_metrics`）は`pub(in crate::analysis)`で
  絞る非対称な公開範囲とする（詳細は[metrics.md 4章](metrics.md#4-可視性の設計-pub--pubin-crateanalysis)。
  [Issue #6のレビュー議論](https://github.com/MinamiyamaKotaro/extmd/issues/6#issuecomment-5301803968)で確定）。
- 各メソッドは`&self`のみを取り、ステートレス。パラメータ調整はコンストラクタ引数として渡す
  （[registry.md 1章](registry.md#1-strategyconfig)の`StrategyConfig`参照）。

## 3. v1スコープの実装

`strategy.rs`自体はトレイト定義（と1章の入出力型）のみを持ち、実装
（`GridPaperStrategy`/`TabularStrategy`）は[strategies/](strategies/mod.md)に置く。
v1では[strategies/mod.md 2章](strategies/mod.md#2-v1スコープ-grid-paper--tabular-の2戦略のみ)の通り
この2戦略のみとし、業務ドメイン特化戦略はv2以降とする
（[Issue #6での決定](https://github.com/MinamiyamaKotaro/extmd/issues/6#issuecomment-5301777202)）。
