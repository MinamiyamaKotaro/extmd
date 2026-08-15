# アーキテクチャ設計書: extmd

対象: [要件定義書](../requirement/requirements.md)

## 1. 設計方針

解析ルール（はみ出し判定・行の分類・見出しレベル判定など）は、対象シートの性質
（方眼紙的な文書、通常の集計表、申請書・議事録など特定業務フォーマット）によって
最適なパラメータやロジックが異なる。これを1つの巨大な条件分岐で実装すると、
ルールを追加・調整するたびに既存ロジックへ手を入れることになり壊れやすい。

そこで、解析ルール一式を `AnalysisStrategy` トレイトとして抽象化し、
ドメインごとの解析ロジックを個別の実装（Strategy）として切り出す。
呼び出し側（変換パイプライン）は具体的な戦略を知らずに `dyn AnalysisStrategy`
を介して処理を行う（Strategyパターン）。

これにより、

- 新しいドメイン向けの解析ルールを追加する際、既存の戦略実装に触れず
  新しい struct + トレイト実装を追加するだけで済む（Open-Closed）
- CLIやテストから戦略を差し替えて挙動を比較できる
- 要件定義書 8章「表形式データとの共存」問題を、戦略の切り替え・自動選択という
  形で解決する

## 2. パイプライン全体像

```
xlsxファイル
    │
    ▼
[1] Reader        … calamine/umya-spreadsheet でセル・書式情報を読み込み Sheet を構築
    │
    ▼
[2] StrategySelector … CLI指定 or 自動判定で AnalysisStrategy を選択
    │
    ▼
[3] Analyzer       … 選択された Strategy を使い Sheet → Vec<Block> に変換
    │                  (はみ出し判定・結合セル解決・行の分類・見出しレベル判定)
    ▼
[4] Renderer       … Vec<Block> → Markdown文字列
    │
    ▼
出力(.md)
```

Strategyパターンが担うのは [3] Analyzer の中核ロジックであり、[1] Reader と
[4] Renderer はStrategyに依存しない共通処理とする。

## 3. コアドメイン型

Strategyトレイトのシグネチャで使う入出力型の概要のみ示す。詳細な設計判断
（`Grid`の内部表現、座標をnewtypeにしなかった理由、`Sheet`の不変性など）は
[ドメイン設計書](domain/mod.md)を参照。

```rust
/// Reader が生成する、1シート分の生データ。イミュータブル。
pub struct Sheet {
    pub name: String,
    pub cells: Grid<Cell>,        // 行×列の2次元グリッド
    pub merges: Vec<MergeRange>,  // ネイティブ結合セル範囲
}

pub struct Cell {
    pub value: CellValue,         // 文字列 / 数値 / 日付 / 真偽値 / 空
    pub column_width: f64,        // 列幅（文字数換算）
    pub wrap_text: bool,
    pub alignment: Alignment,
    pub font: FontInfo,           // サイズ・太字など
    pub number_format: Option<String>, // 表示形式コード
}

/// 結合・整形済みの論理ブロック（Analyzerの出力単位）。
pub struct Block {
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub text: String,
    pub font: FontInfo,
    pub source: BlockSource, // Overflow結合 / ネイティブ結合 / 単独セル
    pub heading_level: Option<u8>, // AnalysisStrategy::heading_level の結果
}

/// はみ出し解決後、1行分のブロック列（classify_rowの入力）。
pub struct ResolvedRow<'a> {
    pub blocks: &'a [Block],
}

pub enum RowKind {
    /// 段落・見出しとして出力する（方眼紙の文章行など）。
    Flow,
    /// Markdownテーブルの1行として出力する。
    TableRow,
}

/// Analyzerの最終出力（classify_row適用後）。Rendererへの入力。
pub struct Document {
    pub sheet_name: String,
    pub rows: Vec<RenderedRow>,
}

pub struct RenderedRow {
    pub kind: RowKind,
    pub blocks: Vec<Block>,
}
```

`Sheet` は `SheetMetrics` 等の派生データを一切キャッシュしない、完全なイミュータブル
データ構造とする（旧版では `OnceCell<SheetMetrics>` を持たせていたが、
ドメイン設計書5章の理由により撤回した）。

`detect_overflow` の入出力型 `OverflowContext`/`OverflowDecision` は、当初ここに
domain型として掲載していたが、`AnalysisStrategy::detect_overflow` のためだけに
存在する型であるため [analysis設計書 strategy.md](analysis/strategy.md#1-overflowcontext--overflowdecision-の配置場所についての設計判断)
の通り `analysis::strategy` へ配置先を変更した。

## 4. `AnalysisStrategy` トレイト

```rust
/// ドメインごとの解析ルール一式。Strategyパターンの共通インターフェース。
pub trait AnalysisStrategy {
    /// CLIの `--strategy` で指定するための識別子（例: "grid-paper", "tabular"）。
    fn id(&self) -> &'static str;

    /// このシートに対して自身がどの程度適合しそうかを返す（0.0〜1.0）。
    /// StrategySelector の自動判定で使用する。`metrics` は select_auto が
    /// 一度だけ計算して全戦略に配る（6.1.1参照。Sheet自体はキャッシュを持たない）。
    fn affinity(&self, sheet: &Sheet, metrics: &SheetMetrics) -> f32;

    /// はみ出し判定: 対象セルを右方向の空セルへどこまで結合するか決定する。
    fn detect_overflow(&self, ctx: &OverflowContext) -> OverflowDecision;

    /// 解決済みの1行が「表の行」か「文章の流れ」かを分類する。
    fn classify_row(&self, row: &ResolvedRow) -> RowKind;

    /// ブロックの書式情報から見出しレベル（1〜6）を判定する。見出しでなければ None。
    fn heading_level(&self, block: &Block) -> Option<u8>;
}
```

- `affinity` は自動選択（後述）のためのフック。単一戦略を明示指定する運用のみなら
  常に `1.0` を返す実装でもよい。
- トレイトオブジェクト (`Box<dyn AnalysisStrategy>`) として扱うため、上記メソッドは
  すべて `&self` を取り、内部で可変状態を持たない（ステートレス）方針とする。
  パラメータ調整はコンストラクタ引数として渡す。

## 5. 具体的な戦略実装

### 5.1 `GridPaperStrategy`（デフォルト）

要件定義書 5.3.2 のヒューリスティックをそのまま実装する。方眼紙的な文書向け。

```rust
pub struct GridPaperStrategy {
    /// 推定描画幅の算出に使う全角文字換算係数など、チューニング用パラメータ。
    pub overflow_threshold: f64,
}

impl AnalysisStrategy for GridPaperStrategy {
    fn id(&self) -> &'static str { "grid-paper" }

    fn affinity(&self, _sheet: &Sheet, metrics: &SheetMetrics) -> f32 {
        // 6.1.3節の実装例を参照。metricsの各指標を重み付けして返す。
        estimate_grid_paper_score(metrics)
    }

    fn detect_overflow(&self, ctx: &OverflowContext) -> OverflowDecision {
        // 要件 5.3.2: wrap_text無効 + 推定描画幅 > 列幅 + 右隣が空、で結合
        // ...
    }

    fn classify_row(&self, row: &ResolvedRow) -> RowKind {
        // 1行が少数の大きなブロックで構成される場合は Flow
    }

    fn heading_level(&self, block: &Block) -> Option<u8> {
        // フォントサイズ・太字からマッピング
    }
}
```

### 5.2 `TabularStrategy`

通常の集計表向け。はみ出し結合を行わず、セルをそのままテーブルセルとして扱う。
CLIの `--no-overflow-merge` はこの戦略を強制的に選択することと等価にする。

```rust
pub struct TabularStrategy;

impl AnalysisStrategy for TabularStrategy {
    fn id(&self) -> &'static str { "tabular" }

    fn affinity(&self, _sheet: &Sheet, metrics: &SheetMetrics) -> f32 {
        estimate_tabular_score(metrics) // 規則的に埋まった矩形領域が多いほど高スコア
    }

    fn detect_overflow(&self, _ctx: &OverflowContext) -> OverflowDecision {
        OverflowDecision::NoMerge
    }

    fn classify_row(&self, _row: &ResolvedRow) -> RowKind {
        RowKind::TableRow
    }

    fn heading_level(&self, _block: &Block) -> Option<u8> {
        None
    }
}
```

### 5.3 業務ドメイン特化戦略（将来拡張）

`GridPaperStrategy` のパラメータを業務フォーマットごとにプリセット化したものを
追加していく想定（例: `MeetingMinutesStrategy` 議事録、`ApplicationFormStrategy` 申請書）。
いずれも `AnalysisStrategy` を実装するだけで、Analyzer/Renderer側の変更は不要。

```rust
pub struct MeetingMinutesStrategy(GridPaperStrategy);
pub struct ApplicationFormStrategy(GridPaperStrategy);
```
のように `GridPaperStrategy` を委譲先として持ち、`affinity` と一部メソッドだけ
上書きする実装も可とする。

**v1スコープには含めない。** 要件定義書4.1（v1スコープ）にドメイン特化戦略への
言及はなく、本節も「将来拡張」と明記していることから、v1は5.1/5.2の
`grid-paper`/`tabular` の2戦略のみで構成する
（[Issue #6での決定](https://github.com/MinamiyamaKotaro/extmd/issues/6#issuecomment-5301777202)、
詳細は[analysis/strategies/mod.md 2章](analysis/strategies/mod.md#2-v1スコープ-grid-paper--tabular-の2戦略のみ)）。

## 6. 戦略の選択（`StrategySelector`）

以下は概要のみを示す。`StrategyConfig`によるパラメータ外部注入、`SheetMetrics`の
可視性設計、僅差フォールバックの実装等の詳細は[analysis設計書](analysis/mod.md)
（[registry.md](analysis/registry.md)・[metrics.md](analysis/metrics.md)）を参照。

```rust
pub struct StrategyRegistry {
    strategies: Vec<Box<dyn AnalysisStrategy>>,
}

impl StrategyRegistry {
    pub fn with_defaults() -> Self {
        Self {
            strategies: vec![
                Box::new(GridPaperStrategy::default()),
                Box::new(TabularStrategy),
            ],
        }
    }

    /// CLIで明示指定された場合。
    pub fn get(&self, id: &str) -> Option<&dyn AnalysisStrategy> {
        self.strategies.iter().map(AsRef::as_ref).find(|s| s.id() == id)
    }

    /// `--strategy auto`（デフォルト）の場合、affinity が最大の戦略を選ぶ。
    /// SheetMetricsはここで一度だけ計算し、全戦略のaffinityに配る。
    pub fn select_auto(&self, sheet: &Sheet) -> &dyn AnalysisStrategy {
        let metrics = compute_sheet_metrics(sheet);
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

CLI連携:

```
extmd <INPUT.xlsx> --strategy auto        # デフォルト。シートごとに自動選択
extmd <INPUT.xlsx> --strategy grid-paper  # 明示的に方眼紙戦略を強制
extmd <INPUT.xlsx> --strategy tabular     # 明示的に通常表戦略を強制（--no-overflow-mergeの代替）
```

シートごとに `select_auto` を呼ぶ設計とし、1ファイル内に方眼紙シートと通常表シートが
混在するケース（要件定義書 8章 #2）にも対応できるようにする。

### 6.1 `affinity` のスコアリング設計

#### 6.1.1 前提: `SheetMetrics`（シート単位の一度きりの前処理）

`affinity` は `StrategyRegistry` が登録済みの全戦略に対して呼び出す。各戦略が
シートを毎回スキャンして特徴量を計算すると、戦略数に比例して無駄な走査が発生する。
そこでシートの構造的特徴量を `select_auto` の中で1回だけ計算し、各戦略の
`affinity` に引数として渡す（[ドメイン設計書 mod.md 5章](domain/mod.md#5-architecturemdからの変更点)
の通り、`Sheet` 自体にキャッシュは持たせない。`select_auto` は1シートにつき
1回しか呼ばれないため、`OnceCell`によるキャッシュ機構は不要と判断した）。

```rust
pub struct SheetMetrics {
    pub avg_column_width: f64,
    pub column_width_stddev: f64,
    pub overflow_candidate_rate: f32,   // 非空セルのうち、はみ出し条件を満たす割合
    pub fill_density: f32,              // 非空セルのbounding boxに対する充填率
    pub row_structural_regularity: f32, // 行間で「非空列パターン」が一致する度合い
    pub merge_irregularity: f32,        // ネイティブ結合セルが単純な矩形でない割合
}

pub fn compute_sheet_metrics(sheet: &Sheet) -> SheetMetrics {
    // 6.1.2節の各指標を算出する
}
```

`SheetMetrics` と `compute_sheet_metrics` はいずれも `analysis` 層に置く
（`domain` 層はStrategy選択の都合を一切知らない）。

#### 6.1.2 各指標の算出方法

1. **`avg_column_width` / `column_width_stddev`**
   列幅（文字数換算）の平均・標準偏差。方眼紙は幅が小さく均一（stddev小）になりやすい。

2. **`overflow_candidate_rate`**
   「推定描画幅 > 列幅 かつ 右隣が空」という条件を、戦略固有パラメータに依存しない
   保守的な既定しきい値で全非空セルに適用し、条件を満たすセル数 ÷ 非空セル総数を取る。
   このロジックは `GridPaperStrategy::detect_overflow` と同じ考え方だが、
   二重実装を避けるため共通ユーティリティ関数として切り出す。

   ```rust
   // analysis::heuristics モジュール
   pub fn is_overflow_candidate(cell: &Cell, next: Option<&Cell>, threshold: f64) -> bool {
       !cell.wrap_text
           && next.is_some_and(Cell::is_empty)
           && estimate_render_width(cell) > cell.column_width * threshold
   }
   ```

   `GridPaperStrategy::detect_overflow` は同じ関数を戦略固有の `overflow_threshold`
   で呼び出し、`SheetMetrics` 計算時は既定値（例: `1.0`）で呼び出す。

3. **`fill_density`**
   非空セル全体を包む最小矩形（bounding box）の面積に対する、実際に非空である
   セル数の割合。通常の表はこの値が高く（隙間が少ない）、方眼紙文書はまばらな
   配置になりやすく低くなる傾向がある。

4. **`row_structural_regularity`**
   各行について「非空セルが存在する列インデックスの集合」を求め、行間の
   Jaccard類似度の平均を取る。通常の表は列の意味が行を通じて固定されるため
   高くなり、方眼紙文書は行ごとに異なるレイアウト（タイトル行・本文行など）を
   取るため低くなりやすい。

5. **`merge_irregularity`**
   ネイティブ結合セルの範囲のうち、単純な「1行×複数列」または「複数行×1列」の
   矩形以外の形（大きな矩形ブロック等）を取る割合。フォーム系の方眼紙シートは
   タイトル欄・記入欄で不規則な結合が多い傾向がある。

#### 6.1.3 実装例

```rust
impl AnalysisStrategy for GridPaperStrategy {
    fn affinity(&self, _sheet: &Sheet, m: &SheetMetrics) -> f32 {
        let narrow_columns = normalize_inverse(m.avg_column_width, 2.0, 12.0);
        let uniformity = 1.0 - normalize(m.column_width_stddev, 0.0, m.avg_column_width.max(1.0) as f32);
        let overflow_signal = m.overflow_candidate_rate;

        // 重みは初期値。実データでの検証を経てチューニングする。
        0.3 * narrow_columns + 0.3 * uniformity + 0.4 * overflow_signal
    }
    // ...
}

impl AnalysisStrategy for TabularStrategy {
    fn affinity(&self, _sheet: &Sheet, m: &SheetMetrics) -> f32 {
        let low_overflow = 1.0 - m.overflow_candidate_rate;

        0.4 * m.fill_density + 0.4 * m.row_structural_regularity + 0.2 * low_overflow
    }
    // ...
}
```

`normalize` / `normalize_inverse` は値を指定範囲で `[0.0, 1.0]` にクランプ線形マッピングする
共通ヘルパー（`analysis::heuristics` に配置）。重み定数はハードコードせず、
`Default` 実装や設定ファイルから差し替え可能にしておき、実データでのチューニングを
コード変更なしで行えるようにする。

#### 6.1.4 僅差の扱い

`select_auto` で最大スコアの戦略を選ぶ際、最上位2戦略の差が小さい（例: 0.05未満）
場合は、要件上の主要ユースケースである `grid-paper` を優先してフォールバックする
運用ルールを設ける。この閾値もチューニング対象。

#### 6.1.5 テスト方針

- 実際の方眼紙サンプル／通常表サンプルを複数用意し、`SheetMetrics` の値をスナップショット
  として固定した上で、意図した戦略側が高スコアになることをテストする。
- 境界的なサンプル（方眼紙と通常表の中間的なレイアウト）も用意し、6.1.4の
  フォールバック閾値が妥当かを検証する。

## 7. 新しいドメイン戦略を追加する手順

1. `AnalysisStrategy` を実装する struct を追加する（既存戦略への変更は不要）
2. `affinity` に、そのドメインらしいシートを識別するスコアリングを実装する
3. `StrategyRegistry::with_config()`（[analysis/registry.md](analysis/registry.md)）の
   戦略一覧に追加する
4. 対象ドメインのサンプルファイルでスナップショットテストを追加する

## 8. 未確定事項（要件定義書 8章との対応）

[Issue #6](https://github.com/MinamiyamaKotaro/extmd/issues/6)での検討により、以下の3点は
[analysis設計書](analysis/mod.md)側で確定した。詳細は各リンク先を参照。

- `affinity` のスコアリング方式（各指標の重み・`overflow_candidate_rate` の既定
  しきい値・6.1.4のフォールバック閾値）は、コード内にハードコードせず
  `StrategyConfig`（[analysis/registry.md 1章](analysis/registry.md#1-strategyconfig)）
  を介して外部注入する設計とした。具体的な数値は引き続き実データでの検証が必要
  （要件定義書 #1 と関連）。
- 業務ドメイン特化戦略（5.3）はv1スコープに含めず、`grid-paper`/`tabular` の2戦略のみで
  v1をリリースする（[analysis/strategies/mod.md 2章](analysis/strategies/mod.md#2-v1スコープ-grid-paper--tabular-の2戦略のみ)）。
- 戦略ごとのパラメータはv1ではCLI引数（`--overflow-threshold`等）による上書きのみを
  サポートし、設定ファイル（TOML等）は将来拡張とする
  （[analysis/registry.md 5章](analysis/registry.md#5-cliとの境界)）。

以下は本設計（analysisの詳細設計）を経てなお残る未確定事項:

- `compute_sheet_metrics`（[analysis/metrics.md](analysis/metrics.md)）が
  `StrategyRegistry::select_auto` からのみ呼ばれる契約は、`pub(in crate::analysis)`に
  よる可視性制限とdocstringで明文化する
  （[analysis/metrics.md 4章](analysis/metrics.md#4-可視性の設計-pub--pubin-crateanalysis)、
  [Issue #6でのレビュー議論](https://github.com/MinamiyamaKotaro/extmd/issues/6#issuecomment-5301796041)を反映）

[Issue #8](https://github.com/MinamiyamaKotaro/extmd/issues/8)での検討により、要件定義書8章
項目3「出力の粒度」は[renderer設計書](renderer/mod.md)側で確定した。デフォルトは複数シートを
1つのMarkdown（ファイル指定時 or 標準出力）へ、シート名をH1見出しとして連結する
`OutputTarget::Stdout`/`SingleFile`とし、シートごとの分割出力（`OutputTarget::SplitDirectory`）
はCLI側の明示指定時のみ使う（[renderer/mod.md 5章](renderer/mod.md#5-シート見出しレベルと本文見出しレベルの階層関係heading_offset)、
[renderer/output.md](renderer/output.md)）。具体的なCLIフラグ名は`cli.rs`の設計フェーズで確定する
（[renderer/output.md 6章「CLIとの境界」](renderer/output.md#6-cliとの境界)。
[analysis/registry.md 5章](analysis/registry.md#5-cliとの境界)と同じ考え方）。

本設計（rendererの詳細設計）を経て、要件定義書8章に以下の項目が新たに追加された:

- **出力先クリーンアップオプションの要否**: `SplitDirectory`出力時、既存の同名ファイルは
  上書きするが、無関係な既存ファイル・削除/リネームされたシートの残骸ファイルは自動削除しない
  （[Issue #8での決定](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301948782)）。
  クリーンアップを行いたい場合のオプション（例: `--clean`）の提供要否は未確定
  （[renderer/output.md 7章](renderer/output.md#7-未確定事項)）。
