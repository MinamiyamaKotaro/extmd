# `analysis::strategies::mod` 設計書

対象: [analysis/mod.md](../mod.md)の対応表における `strategies/mod.rs`。

## 1. 責務

`AnalysisStrategy`の具体的な実装を1ファイル1戦略で保持する。`mod.rs`自体は
サブモジュールの`pub use`による再エクスポートのみを行う。

```rust
pub(in crate::analysis) mod grid_paper;
pub(in crate::analysis) mod tabular;

pub use grid_paper::GridPaperStrategy;
pub use tabular::TabularStrategy;
```

各戦略の`Weights`型（[grid_paper.md](grid_paper.md)/[tabular.md](tabular.md)）は
`registry::StrategyConfig`（[registry.md 1章](../registry.md#1-strategyconfig)）から
参照される必要があるため、`mod.rs`では再エクスポートせず、`strategies::grid_paper::Weights`/
`strategies::tabular::Weights`とフルパスで参照する。

サブモジュール宣言自体は単純な`mod grid_paper;`（無指定 = private）ではなく
`pub(in crate::analysis)`とする必要がある。Rustのモジュール可視性は「無指定なら定義
モジュールとその子孫のみ」に限られ、`registry.rs`は`strategies`モジュールの子孫ではなく
兄弟（いずれも`analysis`直下）であるため、`mod grid_paper;`のままでは`registry.rs`から
`strategies::grid_paper::Weights`という経路を解決できずコンパイルエラーになる。
`pub(in crate::analysis)`にすることで、`analysis`モジュール内であれば`strategies`の
子孫でなくても参照でき、かつ`analysis`外部（`renderer`/`main.rs`/`lib.rs`）には引き続き
公開されない（実装時の技術的制約により確定。設計時点のコード例からの変更）。

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
