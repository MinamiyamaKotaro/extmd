# `domain::sheet` 設計書

対象: [domain/mod.md](mod.md)の対応表における `sheet.rs`。

## 1. `MergeRange`

```rust
pub struct MergeRange {
    pub row_start: usize,
    pub row_end: usize, // inclusive
    pub col_start: usize,
    pub col_end: usize, // inclusive
}

impl MergeRange {
    /// 単一の行、または単一の列にまたがる結合（1×N または N×1）か。
    /// analysis層の merge_irregularity 算出で使う
    /// （[アーキテクチャ設計書 6.1.2](../architecture.md#612-各指標の算出方法)）。
    /// `is_simple_strip` から改名（「単純な矩形」では意図が伝わりにくいという
    /// [PR #3のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/3#issuecomment-5301554119)を反映）。
    pub fn is_single_row_or_column(&self) -> bool {
        self.row_start == self.row_end || self.col_start == self.col_end
    }
}
```

## 2. `Sheet`

```rust
pub struct Sheet {
    pub name: String,
    pub cells: Grid<Cell>,
    pub merges: Vec<MergeRange>,
}
```

`Sheet` は Reader が構築した後は不変（immutable）として扱う。フィールドはすべて
`pub` とし、getter越しではなく直接アクセスする（domain層に振る舞いを持たせない方針のため）。

`SheetMetrics` 等の派生データは一切キャッシュしない、完全なイミュータブルデータ構造とする。
当初 `metrics_cache: OnceCell<SheetMetrics>` を持たせる案があったが、
domainがanalysis層の型を知ってしまう依存方向の誤りだったため撤回した。
詳細は [mod.md 5章「architecture.mdからの変更点」](mod.md#5-architecturemdからの変更点)を参照。

`cells: Grid<Cell>` の型定義は [grid.md](grid.md)、`Cell` は [cell.md](cell.md)を参照。

## 3. 不変条件: `merges` は `cells` の範囲内に収まる

`Sheet.merges` の各 `MergeRange` は、`cells`（`Grid<Cell>`）の `rows`/`cols` の範囲内に
収まっていることを前提とする。この保証は `Sheet` 自身は検証せず、**Reader が `Sheet` を
構築する時点で担保する契約**とする（domain層はI/Oも検証ロジックも持たないという
[mod.md 2章](mod.md#2-設計方針)の方針に従うため）。

Excelファイルのメタデータに壊れた・範囲外の結合セル情報が含まれるケースは実データで
起こりうるため、Readerは `Grid` を構築した後、範囲外の `MergeRange` を破棄（無視）する
バリデーションを行う想定。この責務は `reader/` の設計時に正式に文書化する
（[Issue #1のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/issues/1)での指摘を反映）。
