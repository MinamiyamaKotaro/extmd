# `analysis::strategies::grid_paper` 設計書

対象: [analysis/strategies/mod.md](mod.md)の対応表における `grid_paper.rs`。

## 1. `Weights` と `GridPaperStrategy`

```rust
#[derive(Clone, Copy)]
pub struct Weights {
    pub narrow_columns: f32,
    pub uniformity: f32,
    pub overflow_signal: f32,
}

impl Default for Weights {
    fn default() -> Self {
        // 初期値。実データでの検証を経てチューニングする（6章）。
        Self { narrow_columns: 0.3, uniformity: 0.3, overflow_signal: 0.4 }
    }
}

pub struct GridPaperStrategy {
    overflow_threshold: f64,
    weights: Weights,
}

impl GridPaperStrategy {
    pub(in crate::analysis) fn new(overflow_threshold: f64, weights: Weights) -> Self {
        Self { overflow_threshold, weights }
    }
}
```

`new`の可視性を`pub(in crate::analysis)`とするのは、`GridPaperStrategy`の構築が
`registry::StrategyRegistry::with_config`（[registry.md 2章](../registry.md#2-strategyregistry)）
からのみ行われる想定であるため（`analysis`外から個別に構築させず、必ず`StrategyConfig`経由にする）。

## 2. `affinity`

```rust
impl AnalysisStrategy for GridPaperStrategy {
    fn id(&self) -> &'static str { "grid-paper" }

    fn affinity(&self, _sheet: &Sheet, m: &SheetMetrics) -> f32 {
        let narrow_columns = heuristics::normalize_inverse(m.avg_column_width, 2.0, 12.0);
        let uniformity = 1.0 - heuristics::normalize(m.column_width_stddev, 0.0, m.avg_column_width.max(1.0));
        let overflow_signal = m.overflow_candidate_rate;

        self.weights.narrow_columns * narrow_columns
            + self.weights.uniformity * uniformity
            + self.weights.overflow_signal * overflow_signal
    }
    // detect_overflow/classify_row/heading_levelは3〜5章
}
```

各指標の意味は[metrics.md 2章](../metrics.md#2-各指標の算出方法)を参照。重みの初期値・
実データでのチューニングは6章の通り未確定。

## 3. `detect_overflow`

```rust
fn detect_overflow(&self, ctx: &OverflowContext) -> OverflowDecision {
    if !heuristics::is_overflow_candidate(ctx.source, ctx.following_empty_cells.first(), self.overflow_threshold) {
        return OverflowDecision::NoMerge;
    }

    let estimated_width = heuristics::estimate_render_width(ctx.source);
    let mut consumed = ctx.source.column_width;
    let mut count = 0;

    for cell in ctx.following_empty_cells {
        if consumed >= estimated_width {
            break;
        }
        consumed += cell.column_width;
        count += 1;
    }

    OverflowDecision::MergeCells { count }
}
```

要件定義書5.3.2「推定描画幅を使い切るまで、または空でないセルに到達するまで」に対応する。
`following_empty_cells`は呼び出し元（[mod.md 4章](../mod.md#4-行単位のオーケストレーション-はみ出し・ネイティブ結合・見出し判定)の
`resolve_blocks`）が既に「右隣から連続する空セル」のみに絞って渡すため、この関数自身は
「空でないセルに到達したら止める」判定を明示的に行う必要はない（スライスの境界が
そのまま停止条件になる）。

## 4. `classify_row`

```rust
fn classify_row(&self, row: &ResolvedRow) -> RowKind {
    // 要件定義書5.3.4「1行が単一の大きなテキストブロックで構成される場合」に対応し、
    // ブロックが1つだけの行は、はみ出し・結合の有無（span）によらずFlowとする。
    // これがないと、はみ出しも結合もしていない単独セルの短い見出し・段落
    // （1列のみの"TableRow"に見えてしまう）を誤ってテーブル扱いしてしまう
    // （[PR #7のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/7#issuecomment-5301859980)での指摘を反映）。
    if row.blocks.len() == 1 {
        return RowKind::Flow;
    }

    // 複数ブロックの行は、少数の大きなブロック（結合・はみ出し結合により
    // 複数列にまたがる）で構成される場合はFlow、そうでなければTableRowとする。
    let flow_like = row.blocks.iter().filter(|b| b.span() > 1).count();
    if row.blocks.len() <= 3 && flow_like > 0 {
        RowKind::Flow
    } else {
        RowKind::TableRow
    }
}
```

「少数」（3ブロック以下）・「大きなブロック」（`span() > 1`、[domain/block.md](../../domain/block.md)）
の具体的な基準値は暫定であり、実データでの検証が必要（6章）。

## 5. `heading_level`

```rust
fn heading_level(&self, block: &Block) -> Option<u8> {
    let size = block.font.size_pt;

    // 14pt以上は太字不問で見出しとみなす（Excelでは太字にせずフォントサイズだけで
    // 見出しを表現するケースが多いため）。14pt未満は通常の強調表示と区別するため
    // 太字を必須条件とする
    // （[PR #7のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/7#issuecomment-5301859980)での指摘を反映）。
    if size < 14.0 && !block.font.bold {
        return None;
    }

    match size {
        s if s >= 18.0 => Some(1),
        s if s >= 16.0 => Some(2),
        s if s >= 14.0 => Some(3),
        s if s >= 12.0 => Some(4),
        _ => None, // 通常の太字テキストは見出しとみなさない
    }
}
```

フォントサイズが一定以上の場合は太字不問、それ未満は太字必須で見出しとみなす。
サイズ境界値（14pt/12pt等）は暫定であり、実データでの検証が必要（6章）。

## 6. 未確定事項

- `classify_row`のブロック数・スパンのしきい値（4章）
- `heading_level`のフォントサイズ境界値・太字要否（5章）
- `Weights`の初期値・`overflow_threshold`の実データ検証
