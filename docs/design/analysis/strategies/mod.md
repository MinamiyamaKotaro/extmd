# `analysis::strategies::mod` 設計書

対象: [analysis/mod.md](../mod.md)の対応表における `strategies/mod.rs`。

## 1. 責務

`AnalysisStrategy`の具体的な実装を1ファイル1戦略で保持する。`mod.rs`自体は
サブモジュールの`pub use`による再エクスポートのみを行う。

```rust
mod grid_paper;
mod tabular;

pub use grid_paper::GridPaperStrategy;
pub use tabular::TabularStrategy;
```

各戦略の`Weights`型（[grid_paper.md](grid_paper.md)/[tabular.md](tabular.md)）は
`registry::StrategyConfig`（[registry.md 1章](../registry.md#1-strategyconfig)）から
参照される必要があるため、`mod.rs`では再エクスポートせず、`strategies::grid_paper::Weights`/
`strategies::tabular::Weights`とフルパスで参照する。

## 2. v1スコープ: `grid-paper` / `tabular` の2戦略のみ

要件定義書4.1（v1スコープ）にドメイン特化戦略への言及はなく、アーキテクチャ設計書5.3
「業務ドメイン特化戦略」自体が将来拡張と明記されているため、v1はこの2戦略のみを実装する
（[Issue #6での決定](https://github.com/MinamiyamaKotaro/extmd/issues/6#issuecomment-5301777202)）。

## 3. 拡張手順（v2以降）

新しいドメイン戦略（例: `MeetingMinutesStrategy`）を追加する場合:

1. `strategies/meeting_minutes.rs`を追加し`AnalysisStrategy`を実装する
   （`GridPaperStrategy`への委譲も可、アーキテクチャ設計書5.3）
2. `mod.rs`に`mod meeting_minutes; pub use meeting_minutes::MeetingMinutesStrategy;`を追加
3. `registry::StrategyRegistry::with_config`の戦略一覧に追加

既存の`grid_paper.rs`/`tabular.rs`/`registry.rs`本体のロジックには変更が及ばない
（Open-Closed、アーキテクチャ設計書7章）。
