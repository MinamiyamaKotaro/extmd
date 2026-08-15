# `analysis::strategies::tabular` 設計書

対象: [analysis/strategies/mod.md](mod.md)の対応表における `tabular.rs`。

## 1. `Weights` と `TabularStrategy`

```rust
#[derive(Clone, Copy)]
pub struct Weights {
    pub fill_density: f32,
    pub row_structural_regularity: f32,
    pub low_overflow: f32,
}

impl Default for Weights {
    fn default() -> Self {
        Self { fill_density: 0.4, row_structural_regularity: 0.4, low_overflow: 0.2 }
    }
}

pub struct TabularStrategy {
    weights: Weights,
}

impl TabularStrategy {
    pub(in crate::analysis) fn new(weights: Weights) -> Self {
        Self { weights }
    }
}
```

[`GridPaperStrategy`](grid_paper.md#1-weights-と-gridpaperstrategy)と異なり、
`overflow_threshold`を持たない。`detect_overflow`が常に`NoMerge`を返すため不要（2章）。

## 2. トレイト実装

```rust
impl AnalysisStrategy for TabularStrategy {
    fn id(&self) -> &'static str { "tabular" }

    fn affinity(&self, _sheet: &Sheet, m: &SheetMetrics) -> f32 {
        let low_overflow = 1.0 - m.overflow_candidate_rate;
        self.weights.fill_density * m.fill_density
            + self.weights.row_structural_regularity * m.row_structural_regularity
            + self.weights.low_overflow * low_overflow
    }

    fn detect_overflow(&self, _ctx: &OverflowContext) -> OverflowDecision {
        OverflowDecision::NoMerge
    }

    fn classify_row(&self, row: &ResolvedRow) -> RowKind {
        // 全セルが空の行（blocksが空）は、TableRowにすると
        // renderer::table::render_tableでcol_countが0になり内容のない退化した
        // テーブル行（`| |`等）が出力されてしまうためFlowとする
        // （GridPaperStrategy::classify_row、PR #21と同じ理由。Issue #33で判明）。
        if row.blocks.is_empty() {
            RowKind::Flow
        } else {
            RowKind::TableRow
        }
    }

    fn heading_level(&self, _block: &Block) -> Option<u8> {
        None
    }
}
```

`--no-overflow-merge`（要件定義書5.1）は`TabularStrategy`を強制選択することと等価にする
（CLI層の責務、[registry.md 5章](../registry.md#5-cliとの境界)）。

## 3. 未確定事項

- `Weights`の初期値の実データ検証（[grid_paper.md 6章](grid_paper.md#6-未確定事項)と同様）
