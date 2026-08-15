//! `AnalysisStrategy`の具体的な実装を1ファイル1戦略で保持する
//! （docs/design/analysis/strategies/mod.md）。v1スコープは`grid-paper`/`tabular`の2戦略のみ。

// `Weights`型（grid_paper::Weights/tabular::Weights）は`registry::StrategyConfig`
// （sibling module）から`strategies::grid_paper::Weights`のようにフルパスで参照される
// ため、サブモジュール自体は`analysis`内に限定して公開する。
pub(in crate::analysis) mod grid_paper;
pub(in crate::analysis) mod tabular;

pub use grid_paper::GridPaperStrategy;
pub use tabular::TabularStrategy;
