//! `AnalysisStrategy`トレイト、`OverflowContext`、`OverflowDecision`
//! （docs/design/analysis/strategy.md）。

use crate::domain;

use super::metrics::SheetMetrics;

/// はみ出し判定の対象となる1セルとその右方向の空セル列。
pub struct OverflowContext<'a> {
    pub source: &'a domain::Cell,
    /// 右隣から連続する空セルのみ（ネイティブ結合セルの領域は含まない）。
    pub following_empty_cells: &'a [domain::Cell],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowDecision {
    /// はみ出しなし。単独セルとして扱う。
    NoMerge,
    /// 右方向に`count`個の空セルまで結合する。
    MergeCells { count: usize },
}

/// ドメインごとの解析ルール一式。Strategyパターンの共通インターフェース。
pub trait AnalysisStrategy {
    /// CLIの `--strategy` で指定するための識別子（例: "grid-paper", "tabular"）。
    fn id(&self) -> &'static str;

    /// このシートに対して自身がどの程度適合しそうかを返す（0.0〜1.0）。
    /// `metrics`は`StrategyRegistry::select_auto`が一度だけ計算して全戦略に配る。
    fn affinity(&self, sheet: &domain::Sheet, metrics: &SheetMetrics) -> f32;

    /// はみ出し判定: 対象セルを右方向の空セルへどこまで結合するか決定する。
    fn detect_overflow(&self, ctx: &OverflowContext) -> OverflowDecision;

    /// 解決済みの1行が「表の行」か「文章の流れ」かを分類する。
    fn classify_row(&self, row: &domain::ResolvedRow) -> domain::RowKind;

    /// ブロックの書式情報から見出しレベル（1〜6）を判定する。見出しでなければ`None`。
    fn heading_level(&self, block: &domain::Block) -> Option<u8>;
}
