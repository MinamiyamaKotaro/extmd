# `analysis::mod` 設計書

対象: [アーキテクチャ設計書 2章「パイプライン全体像」](../architecture.md#2-パイプライン全体像)
`[2] StrategySelector` + `[3] Analyzer` の詳細化。
[README](../../../README.md)のディレクトリ構成における `src/analysis/` に対応する。

`docs/design/analysis/` は `src/analysis/` のファイル構成と1:1で対応させる
（[domain/mod.md 1章](../domain/mod.md#1-対応表)の運用ルールを踏襲）。

## 1. 対応表

| `src/analysis/` | `docs/design/analysis/` | 内容 |
|---|---|---|
| `mod.rs` | [mod.md](mod.md)（このファイル） | 設計方針・`analyze`公開API・Analyzerのオーケストレーション |
| `strategy.rs` | [strategy.md](strategy.md) | `AnalysisStrategy`トレイト、`OverflowContext`、`OverflowDecision` |
| `registry.rs` | [registry.md](registry.md) | `StrategyRegistry`、`StrategyConfig`、`select_auto` |
| `metrics.rs` | [metrics.md](metrics.md) | `SheetMetrics`、`compute_sheet_metrics` |
| `heuristics.rs` | [heuristics.md](heuristics.md) | `is_overflow_candidate`、`estimate_render_width`、`normalize`系ヘルパー |
| `strategies/mod.rs` | [strategies/mod.md](strategies/mod.md) | サブモジュールの再エクスポート、v1スコープの方針 |
| `strategies/grid_paper.rs` | [strategies/grid_paper.md](strategies/grid_paper.md) | `GridPaperStrategy` |
| `strategies/tabular.rs` | [strategies/tabular.md](strategies/tabular.md) | `TabularStrategy` |

## 2. 設計方針

- `analysis`は[domain/mod.md 2章](../domain/mod.md#2-設計方針)の依存方向の方針に従い、
  `domain`にのみ依存する。`reader`/`renderer`には依存しない。
- [reader/mod.md 3章](../reader/mod.md#3-設計方針)と同様、内部モジュール
  （`registry.rs`/`metrics.rs`/`heuristics.rs`/`strategy.rs`/`strategies/*`）は
  `analysis`外部（`renderer`/`main.rs`/`lib.rs`）に直接公開しない。`analysis`の外部から
  見えるのは本ファイルの公開API（3章）と、トレイトオブジェクトとして返る
  `AnalysisStrategy`（[strategy.md 2章](strategy.md#2-analysisstrategy-トレイト)）のみとする。
- `SheetMetrics`/`compute_sheet_metrics`は`analysis`内であっても`registry.rs`以外から
  呼ばれることを想定しない。可視性による強制は
  [metrics.md 4章](metrics.md#4-可視性の設計-pub--pubin-crateanalysis)を参照。

## 3. 公開API: `analyze`

```rust
/// 1シートを、指定された戦略で `domain::Document` に変換する。
/// 戦略の選択（CLI指定 or 自動判定）は呼び出し側（`lib.rs`）の責務とし、
/// `analyze` は既に決定済みの `strategy` を受け取るだけとする
/// （[reader/mod.md 5章](../reader/mod.md#5-readererror-と公開api)の
/// 「エントリポイントは横断的関心事のみ」という方針を踏襲）。
pub fn analyze(sheet: &domain::Sheet, strategy: &dyn AnalysisStrategy) -> domain::Document {
    let rows = (0..sheet.cells.rows())
        .map(|row_idx| analyze_row(sheet, row_idx, strategy))
        .collect();

    domain::Document { sheet_name: sheet.name.clone(), rows }
}
```

`StrategyRegistry`（[registry.md](registry.md)）を`analyze`の引数に取らない設計とする。
戦略の選択（`select_auto`/`get`）と選択後の変換（`analyze`）を分離することで、CLIが
「まず戦略を決定し、決定した戦略名をログ出力する」（要件定義書5.1 `-v`/`--verbose`）
といった用途に対応しやすくする。

## 4. 行単位のオーケストレーション: はみ出し・ネイティブ結合・見出し判定

```rust
fn analyze_row(sheet: &domain::Sheet, row_idx: usize, strategy: &dyn AnalysisStrategy) -> domain::RenderedRow {
    let blocks = resolve_blocks(sheet, row_idx, strategy);
    let resolved = domain::ResolvedRow { blocks: &blocks };
    let kind = strategy.classify_row(&resolved);
    domain::RenderedRow { kind, blocks } // clone不要（domain/document.md 5章と同じ理由）
}

fn resolve_blocks(sheet: &domain::Sheet, row_idx: usize, strategy: &dyn AnalysisStrategy) -> Vec<domain::Block> {
    let row = sheet.cells.row(row_idx);
    let mut blocks = Vec::new();
    let mut col = 0;

    while col < row.len() {
        if let Some(merge) = native_merge_at(sheet, row_idx, col) {
            // 要件定義書5.3.3: ネイティブ結合セル内ではみ出し判定を行わない
            blocks.push(build_block(&row[col], row_idx, col, merge.col_end, domain::BlockSource::NativeMerge, strategy));
            col = merge.col_end + 1;
            continue;
        }

        if row[col].is_empty() {
            // 縦方向のネイティブ結合セル（rowspan相当）の、左上以外の行に達した場合。
            // Markdownのパイプテーブルはrowspanを持たないため、結合範囲の左上セルの値を
            // この行にも複製して出力する（Issue #46。空欄のままだとデータが欠けて見える）。
            if let Some(merge) = covering_merge(sheet, row_idx, col) {
                let top_left = sheet.cells.get(merge.row_start, merge.col_start)
                    .expect("MergeRange must be within cells bounds (Sheet invariant)");
                blocks.push(build_block(top_left, row_idx, merge.col_start, merge.col_end, domain::BlockSource::NativeMerge, strategy));
                col = merge.col_end + 1;
                continue;
            }

            col += 1;
            continue;
        }

        let following = trailing_empty(sheet, row_idx, &row[col + 1..], col + 1);
        let ctx = OverflowContext { source: &row[col], following_empty_cells: following };
        let (source, col_end) = match strategy.detect_overflow(&ctx) {
            OverflowDecision::NoMerge => (domain::BlockSource::Single, col),
            OverflowDecision::MergeCells { count } => (domain::BlockSource::OverflowMerge, col + count),
        };
        blocks.push(build_block(&row[col], row_idx, col, col_end, source, strategy));
        col = col_end + 1;
    }

    blocks
}

/// `following`（対象セルの右隣から続くセル列）のうち、「連続する空セル」を返す。
/// ネイティブ結合セルの範囲（左上セルに限らない）に達した時点で打ち切る。
/// 結合範囲の左上以外のセルもグリッド上は空（`CellValue::Empty`）として表現される
/// （[domain/sheet.md](../domain/sheet.md)）ため、単純に`is_empty()`だけで判定すると
/// 結合セルの領域まではみ出し結合の対象に含めてしまう
/// （[PR #7のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/7#issuecomment-5301859980)での指摘を反映）。
fn trailing_empty<'a>(
    sheet: &domain::Sheet,
    row_idx: usize,
    following: &'a [domain::Cell],
    start_col: usize,
) -> &'a [domain::Cell] {
    let mut len = 0;
    for (i, cell) in following.iter().enumerate() {
        if !cell.is_empty() || is_in_native_merge(sheet, row_idx, start_col + i) {
            break;
        }
        len = i + 1;
    }
    &following[..len]
}

/// 指定座標がいずれかのネイティブ結合セルの範囲内（左上セルに限らない）に含まれるか。
fn is_in_native_merge(sheet: &domain::Sheet, row: usize, col: usize) -> bool {
    covering_merge(sheet, row, col).is_some()
}

/// 指定座標を含む（左上セルに限らない）ネイティブ結合セルの範囲を返す。
fn covering_merge(sheet: &domain::Sheet, row: usize, col: usize) -> Option<&domain::MergeRange> {
    sheet.merges.iter().find(|m| {
        (m.row_start..=m.row_end).contains(&row) && (m.col_start..=m.col_end).contains(&col)
    })
}

/// 指定座標が、いずれかのネイティブ結合セル範囲の左上セルであれば、その範囲を返す。
fn native_merge_at(sheet: &domain::Sheet, row: usize, col: usize) -> Option<&domain::MergeRange> {
    sheet.merges.iter().find(|m| m.row_start == row && m.col_start == col)
}

/// `Block`を構築し、その場で`heading_level`を確定させて格納する（5章）。
/// text/fontは結合・はみ出し範囲の左上セル（`source_cell`）の値をそのまま使う。
fn build_block(
    source_cell: &domain::Cell,
    row: usize,
    col_start: usize,
    col_end: usize,
    source: domain::BlockSource,
    strategy: &dyn AnalysisStrategy,
) -> domain::Block {
    let mut block = domain::Block { row, col_start, col_end, text: source_cell.display_text(), font: source_cell.font.clone(), source, heading_level: None };
    block.heading_level = strategy.heading_level(&block);
    block
}
```

- `native_merge_at`は`sheet.merges`（[domain/sheet.md](../domain/sheet.md)）から、その行・列が
  結合範囲の左上セルであるものを探すヘルパー（このファイル内のプライベート関数）。
- `covering_merge`は`native_merge_at`と異なり、左上セルに限らず指定座標を含む結合範囲を
  返す。`is_in_native_merge`（`trailing_empty`用の真偽値判定）と、縦方向の結合セルの
  複製（`resolve_blocks`本体、Issue #46）の両方から使われる共通ヘルパー。
- `trailing_empty`は「右隣から連続する空セル、ただしネイティブ結合セルの領域は除く」に絞る
  ヘルパー（[strategy.md 1章](strategy.md#1-overflowcontext--overflowdecision-の配置場所についての設計判断)の
  `OverflowContext::following_empty_cells`の契約を満たすため）。ネイティブ結合セルの左上が
  空文字（値なし）の場合、単純な空セル判定だけでは結合範囲の内部まで空セル列として
  収集してしまうため、`is_in_native_merge`で境界チェックする。
- 結合範囲の左上以外のセル（`row[col].is_empty()`で検出）に達した場合、単に読み飛ばすと
  Markdownのパイプテーブルにはその行だけ値が欠けて見える（rowspanを表現できないため）。
  そこで`covering_merge`でその行が縦方向の結合範囲内かどうかを判定し、範囲内であれば
  左上セルの値を複製した`Block`をこの行にも生成する（[Issue #46](https://github.com/MinamiyamaKotaro/extmd/issues/46)。
  [renderer/table.md 3章](../renderer/table.md#3-結合セルcol_startcol_endの表現)参照）。

## 5. domain層への変更: `Block::heading_level` の追加

アーキテクチャ設計書2章は「Rendererは`analysis`（Strategy）に依存しない」と明記している。
一方で`AnalysisStrategy::heading_level`（[strategy.md 2章](strategy.md#2-analysisstrategy-トレイト)）は
見出しレベルを判定するメソッドであり、Rendererがこの結果を必要とする。Rendererが
`AnalysisStrategy`を直接呼べない以上、見出し判定はAnalyzer側（`analyze`実行中）で完了させ、
結果を`Block`に焼き込んでおく必要がある。

[domain/block.md](../domain/block.md)の`Block`にはこのためのフィールドがなかったため、
本設計にあわせて`heading_level: Option<u8>`を追加する（[domain/block.md](../domain/block.md)側にも反映済み）。

```rust
pub struct Block {
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub text: String,
    pub font: FontInfo,
    pub source: BlockSource,
    pub heading_level: Option<u8>, // AnalysisStrategy::heading_level の結果（analysis層が確定させる）
}
```

## 6. 未確定事項

- `native_merge_at`の探索方法（`sheet.merges`を毎行線形探索するか、事前にインデックスを
  構築するか）はパフォーマンス次第で実装フェーズに詰める
- シート内に複数シートがある場合の`Document`集約（1ファイル1Markdown or シートごとの扱い）は
  要件定義書5.4「複数シートを変換する場合の出力形態」の未確定と連動し、`lib.rs`側の設計に委ねる
