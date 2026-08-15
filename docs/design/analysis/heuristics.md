# `analysis::heuristics` 設計書

対象: [analysis/mod.md](mod.md)の対応表における `heuristics.rs`。

## 1. 責務

`metrics.rs`（シート全体の特徴量算出）と`strategies/grid_paper.rs`（個別セルのはみ出し判定）の
両方から呼ばれる、セル単位の判定・計算ロジックを共通化する。
[domain/mod.md 4章](../domain/mod.md#4-analysisheuristics-との境界)の通り、
`domain::{Cell, CellValue}`の公開フィールドのみを参照し、domain→analysis方向の依存は発生しない。

## 2. `is_overflow_candidate`

```rust
pub(in crate::analysis) fn is_overflow_candidate(
    cell: &domain::Cell,
    next: Option<&domain::Cell>,
    threshold: f64,
) -> bool {
    !cell.wrap_text
        && next.is_some_and(domain::Cell::is_empty)
        && estimate_render_width(cell) > cell.column_width * threshold
}
```

呼び出し元ごとに異なる`threshold`を渡す:

- `metrics::compute_sheet_metrics`（[metrics.md 2章](metrics.md#2-各指標の算出方法)）:
  戦略に依存しない保守的な既定値（`StrategyConfig::default().overflow_threshold`、
  [registry.md 1章](registry.md#1-strategyconfig)参照）
- `GridPaperStrategy::detect_overflow`（[strategies/grid_paper.md 3章](strategies/grid_paper.md#3-detect_overflow)）:
  戦略固有の`overflow_threshold`（CLIの`--overflow-threshold`で上書き可能）

可視性は`pub(in crate::analysis)`とし、`SheetMetrics`
（[metrics.md 4章](metrics.md#4-可視性の設計-pub--pubin-crateanalysis)）と同じ理由で
`analysis`外には公開しない。

## 3. `estimate_render_width`

```rust
pub(in crate::analysis) fn estimate_render_width(cell: &domain::Cell) -> f64 {
    let text = cell.value.display_text(); // domain/cell.md 1章
    let base_width: f64 = text.chars()
        .map(|c| if is_full_width(c) { 2.0 } else { 1.0 })
        .sum();

    // Excelの列幅は既定フォントサイズ（11pt）を基準とした単位のため、
    // セルのフォントサイズがそれより大きい/小さい場合は比例して幅を補正する
    // （[PR #7のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/7#issuecomment-5301859980)での指摘を反映。
    // 元の実装は文字数・全角半角のみで、大きい見出しフォントのはみ出しを
    // 過小評価していた）。
    base_width * (cell.font.size_pt as f64 / 11.0)
}
```

要件定義書5.3.2「文字数・全角/半角を考慮した概算幅」に対応する。全角/半角の判定
（`is_full_width`）はUnicode East Asian Widthに基づく簡易判定を想定するが、境界となる
文字幅の係数（全角=2.0固定でよいか、フォントごとの実測値に寄せるか）とフォントサイズ
補正の基準値（11pt）は実データでの検証が必要（6章）。

## 4. `normalize` / `normalize_inverse`

```rust
/// 値を[min, max]の範囲で[0.0, 1.0]にクランプ線形マッピングする。
pub(in crate::analysis) fn normalize(value: f64, min: f64, max: f64) -> f32 {
    (((value - min) / (max - min).max(f64::EPSILON)).clamp(0.0, 1.0)) as f32
}

pub(in crate::analysis) fn normalize_inverse(value: f64, min: f64, max: f64) -> f32 {
    1.0 - normalize(value, min, max)
}
```

両方とも`f64`引数を取り`f32`を返す形に統一する（`SheetMetrics`の`avg_column_width`/
`column_width_stddev`が`f64`であるため。アーキテクチャ設計書6.1.3の擬似コードでは
この点が曖昧だったため、ここで確定させる）。
`GridPaperStrategy::affinity`/`TabularStrategy::affinity`（[strategies/](strategies/mod.md)）
から使う共通ヘルパー。

## 5. 未確定事項

- 全角/半角判定の実装（`unicode-width`クレート採用可否を含む）
- `estimate_render_width`の係数（3章参照）の実データ検証
