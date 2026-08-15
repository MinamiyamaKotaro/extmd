# `analysis::metrics` 設計書

対象: [analysis/mod.md](mod.md)の対応表における `metrics.rs`。

## 1. `SheetMetrics`

```rust
/// シートの構造的特徴量。`StrategyRegistry::select_auto`が1シートにつき1回だけ
/// 計算し、登録された全戦略の`affinity`に配る（registry.md 2章）。
pub struct SheetMetrics {
    pub(in crate::analysis) avg_column_width: f64,
    pub(in crate::analysis) column_width_stddev: f64,
    pub(in crate::analysis) overflow_candidate_rate: f32,
    pub(in crate::analysis) fill_density: f32,
    pub(in crate::analysis) row_structural_regularity: f32,
    pub(in crate::analysis) merge_irregularity: f32,
}
```

## 2. 各指標の算出方法

1. **`avg_column_width` / `column_width_stddev`**
   列幅（文字数換算、[domain/cell.md](../domain/cell.md)の`Cell::column_width`）の平均・標準偏差。
   方眼紙は幅が小さく均一（stddev小）になりやすい。

2. **`overflow_candidate_rate`**
   「推定描画幅 > 列幅 かつ 右隣が空」という条件を、戦略固有パラメータに依存しない
   保守的な既定しきい値で全非空セルに適用し、条件を満たすセル数 ÷ 非空セル総数を取る。
   [heuristics.md 2章](heuristics.md#2-is_overflow_candidate)の`is_overflow_candidate`を、
   `GridPaperStrategy::detect_overflow`とは別の既定しきい値で呼び出す。

3. **`fill_density`**
   非空セル全体を包む最小矩形（bounding box）の面積に対する、実際に非空であるセル数の割合。
   通常の表はこの値が高く（隙間が少ない）、方眼紙文書はまばらな配置になりやすく低くなる傾向がある。

4. **`row_structural_regularity`**
   各行について「非空セルが存在する列インデックスの集合」を求め、行間のJaccard類似度の
   平均を取る。通常の表は列の意味が行を通じて固定されるため高くなり、方眼紙文書は
   行ごとに異なるレイアウト（タイトル行・本文行など）を取るため低くなりやすい。

5. **`merge_irregularity`**
   ネイティブ結合セル（[domain/sheet.md](../domain/sheet.md)の`MergeRange`）のうち、
   `MergeRange::is_single_row_or_column()`が`false`となる割合。フォーム系の方眼紙シートは
   タイトル欄・記入欄で不規則な結合が多い傾向がある。**`sheet.merges`が空（結合セルが
   1つもない、最も一般的なケース）の場合はゼロ除算になるため`0.0`を返す。**
   これは3章の非空セル数によるガードとは独立した条件（結合セルの有無と非空セルの有無は
   無関係）のため、`merge_irregularity`の算出箇所で個別にガードする
   （[PR #7のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/7#issuecomment-5301859980)での指摘を受けて洗い出した、同種の問題）。

## 3. `compute_sheet_metrics`

```rust
/// `StrategyRegistry::select_auto`（registry.rs）からのみ呼ばれる。
pub(in crate::analysis) fn compute_sheet_metrics(sheet: &domain::Sheet) -> SheetMetrics {
    let non_empty_count = sheet.cells.iter_rows().flatten().filter(|c| !c.is_empty()).count();

    // 非空セルが1つもない場合、2章の各指標はいずれもゼロ除算になる
    // （fill_density: bounding boxの面積が0、row_structural_regularity: 非空行が0、
    // overflow_candidate_rate: 非空セル総数が0での除算）。reader/xlsx.md 3章の
    // 「列数0のシート」（rows=0, cols=1で構築される空Grid）はこの条件を満たす
    // 正当な入力であり、パニックさせずにデフォルト値のSheetMetricsを返す
    // （[PR #7のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/7#issuecomment-5301859980)での指摘を反映）。
    if non_empty_count == 0 {
        return SheetMetrics {
            avg_column_width: 0.0,
            column_width_stddev: 0.0,
            overflow_candidate_rate: 0.0,
            fill_density: 0.0,
            row_structural_regularity: 0.0,
            merge_irregularity: 0.0,
        };
    }

    // 2章の各指標を算出する
}
```

## 4. 可視性の設計: `pub` + `pub(in crate::analysis)`

Rustのモジュール可視性は「無指定(private)なら定義モジュールとその子孫のみ」という規則があり、
`GridPaperStrategy`等の戦略実装は`analysis::strategies::grid_paper`という`metrics`から見て
**兄弟**モジュールに置かれる（[strategies/mod.md](strategies/mod.md)）。フィールドを無指定
privateのままにすると、戦略実装から`SheetMetrics`の値を読めなくなってしまう。

そのため:

- **`SheetMetrics`構造体自体: `pub`。** `AnalysisStrategy::affinity`
  （[strategy.md 2章](strategy.md#2-analysisstrategy-トレイト)）というpublicトレイトの
  メソッド引数に現れるため、それと同じかそれ以上の可視性が必要
  （さもないと"private type in public interface"エラーになる）。
- **各フィールド・`compute_sheet_metrics`関数: `pub(in crate::analysis)`。**
  `analysis`モジュールとその子孫（`registry.rs`/`strategy.rs`/`strategies/*`/`heuristics.rs`）
  からは読み書き・呼び出しができる一方、`reader`/`renderer`/`main.rs`など`analysis`の外からは
  一切アクセスできない。

これにより「`SheetMetrics`は`select_auto`が1回だけ計算し、全戦略に配る」という契約を、
ドキュメントだけでなくコンパイルエラーとして強制できる
（[Issue #6でのレビュー議論](https://github.com/MinamiyamaKotaro/extmd/issues/6#issuecomment-5301796041)、
[修正案](https://github.com/MinamiyamaKotaro/extmd/issues/6#issuecomment-5301803968)で確定）。

## 5. テスト方針

- `tests/fixtures/`に方眼紙レイアウト・通常表レイアウトのサンプルxlsxを追加し、`select_auto`
  経由で意図した戦略が選ばれることをアサートするスナップショットテストを整備する
  （[Issue #6での提案](https://github.com/MinamiyamaKotaro/extmd/issues/6#issuecomment-5301777202)）。
  4章の可視性制限により`SheetMetrics`を`analysis`外から直接構築できないため、
  テストは常に`select_auto`（`analysis`の公開APIの一部）経由で行う。
- 各指標の重み・しきい値のチューニングは[registry.md 1章](registry.md#1-strategyconfig)の
  `StrategyConfig`経由で行い、`metrics.rs`自体のコード変更を伴わない。

## 6. 未確定事項

- 各指標の具体的な計算式の係数（正規化範囲など）は実データでの検証が必要
- スナップショットテスト用の実データサンプル収集
