# `domain::mod` 設計書

対象: [アーキテクチャ設計書 3章「コアドメイン型」](../architecture.md#3-コアドメイン型)の詳細化。
[README](../../../README.md)のディレクトリ構成における `src/domain/` に対応する。

`docs/design/domain/` は `src/domain/` のファイル構成と1:1で対応させる。
このファイル（`mod.md`）は `mod.rs` に対応し、モジュール全体の設計方針と、
個別ファイルの型定義には属さない横断的な設計判断をまとめる。各型の詳細は
対応するファイルを参照。

## 1. 対応表

| `src/domain/` | `docs/design/domain/` | 内容 |
|---|---|---|
| `mod.rs` | [mod.md](mod.md)（このファイル） | 設計方針・モジュール構成・横断的な設計判断 |
| `cell.rs` | [cell.md](cell.md) | `CellValue`, `Alignment`, `FontInfo`, `Cell` |
| `grid.rs` | [grid.md](grid.md) | `Grid<T>` |
| `sheet.rs` | [sheet.md](sheet.md) | `MergeRange`, `Sheet` |
| `block.rs` | [block.md](block.md) | `Block`, `BlockSource` |
| `document.rs` | [document.md](document.md) | `RowKind`, `ResolvedRow`, `RenderedRow`, `Document` |

以降、`reader/` `analysis/` `renderer/` の設計を進める際も、
`docs/design/<module>/` に同じ1ファイル対1ファイルの対応で書く方針とする。

## 2. 設計方針

- `domain/` はプロジェクトの最下層とし、`reader` / `analysis` / `renderer` の
  いずれにも依存しない。依存の向きは常に `reader/analysis/renderer → domain` の
  一方向とする。
- domain の型は純粋なデータ構造（+ 不変条件を守るための最小限のメソッド）のみを持つ。
  I/O・ヒューリスティック計算・戦略選択といったロジックは一切持たない。
- 上記方針の帰結として、アーキテクチャ設計書 6.1.1で当初示していた
  `Sheet { metrics_cache: OnceCell<SheetMetrics>, .. }` は撤回する
  （理由は4章「architecture.mdからの変更点」を参照）。

## 3. 座標の表現（`RowIndex`/`ColIndex`の検討）

行・列インデックスは newtype（`struct RowIndex(usize)` 等）でラップすることも検討したが、
v1では **見送り、素の `usize` を `row: usize` / `col: usize` という命名で統一する**。
この方針は `cell.rs`（`Cell`は自身の座標を持たない）、`grid.rs`（`Grid::get(row, col)`）、
`block.rs`（`Block::row`/`col_start`/`col_end`）など複数ファイルにまたがるため、
個別ファイルではなくここに記載する。

理由: newtypeは行と列の取り違えバグを型で防げる利点がある一方、`Grid::get(row, col)` など
アクセサが数カ所に閉じているため取り違えのリスクは小さく、`From`/`Into`変換や演算子実装の
ボイラープレートが増えるコストの方が現時点では大きいと判断した。座標の取り違えバグが
実際に発生するようであれば再検討する。

## 4. `analysis::heuristics` との境界

`is_overflow_candidate` や `estimate_render_width` のような計算ロジックは
`Cell` の公開フィールドだけを使って実装でき、`domain` 側に特別なメソッドを
生やす必要はない。したがって `analysis/heuristics.rs` は `domain::{Cell, CellValue}` を
参照するだけで完結し、domain → analysis 方向の依存は発生しない。

## 5. architecture.mdからの変更点

domain設計を詰める過程で、アーキテクチャ設計書 6.1.1 の以下の設計に
層の依存方向の誤り（domainがanalysisの型 `SheetMetrics` を知ってしまう）を見つけた。

```rust
// architecture.md 6.1.1（旧）
pub struct Sheet {
    metrics_cache: OnceCell<SheetMetrics>, // ← domainがanalysisの型に依存してしまう
}
impl Sheet {
    pub fn metrics(&self) -> &SheetMetrics { ... }
}
```

**修正方針:** `Sheet` から `metrics_cache` を削除する。そもそも `SheetMetrics` は
`StrategyRegistry::select_auto` の中で一度だけ計算すれば十分であり
（同一シートに対して`select_auto`を何度も呼ぶユースケースは想定していない）、
`OnceCell`によるキャッシュという仕組み自体が過剰だった。代わりに
`select_auto` が計算した `SheetMetrics` を、各戦略の `affinity` に引数として渡す。

```rust
// 修正後
pub trait AnalysisStrategy {
    fn affinity(&self, sheet: &Sheet, metrics: &SheetMetrics) -> f32;
    // ...
}

impl StrategyRegistry {
    pub fn select_auto(&self, sheet: &Sheet) -> &dyn AnalysisStrategy {
        let metrics = analysis::compute_sheet_metrics(sheet); // 1回だけ計算
        self.strategies
            .iter()
            .map(AsRef::as_ref)
            .max_by(|a, b| {
                a.affinity(sheet, &metrics).total_cmp(&b.affinity(sheet, &metrics))
            })
            .expect("at least one strategy registered")
    }
}
```

この変更により `OnceCell` への依存もなくなり、`Sheet` は完全にイミュータブルな
プレーンデータ構造になる。この修正は[アーキテクチャ設計書](../architecture.md)側にも
反映済み（4章・6.1.1・6.1.3）。
