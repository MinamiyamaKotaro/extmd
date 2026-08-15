# `cli` 詳細設計書

対象: [要件定義書 5.1節](../requirement/requirements.md#51-cli仕様案)・[8章](../requirement/requirements.md#8-前提要確認事項オープンクエスチョン)、および各層設計書からの持ち越し事項。
[README](../../README.md)のディレクトリ構成における `src/cli.rs`・`src/main.rs`・`src/lib.rs` の連携に対応する。

本設計は [Issue #10](https://github.com/MinamiyamaKotaro/extmd/issues/10) での検討を反映したものである。

## 1. 責務分担

CLIのインターフェース設計にあたり、テスタビリティとモジュール結合度を下げるため、`main.rs`、`cli.rs`、`lib.rs` の責務を以下のように厳密に分離する。

```
[main.rs (バイナリエントリ)]
   │
   ├─► cli.rs: 引数パース & ConvertConfig への変換
   │
   └─► lib.rs: convert(ConvertConfig) の実行 & エラーハンドリング・終了コード設定
```

- **`src/main.rs`**: 薄いバイナリエントリポイント。
  1. ライブラリ内の `cli` モジュールを用いてコマンドライン引数 (`CliArgs`) をパースし、設定型 (`ConvertConfig`) を構築する。
  2. ログ出力を初期化する。
  3. `extmd::convert(config)` を呼び出す。
  4. 実行結果 (Result) に応じて、適切なエラーメッセージを標準エラー出力へ表示し、プロセス終了コードを設定して終了する。
- **`src/cli.rs`**: CLI引数の定義と、ライブラリ設定型へのマッピング。ライブラリ側のモジュール (`pub mod cli`) として定義する。
  1. `clap` を用いて、コマンドライン引数構造体 (`CliArgs`) を定義する。
  2. `CliArgs` から `ConvertConfig` への変換ロジックを持つ。
- **`src/lib.rs`**: 変換処理のエントリポイント。
  1. `pub mod cli;` を宣言し、`cli.rs` をライブラリの一部としてコンパイル・公開する。
  2. 外部に公開する API (`convert` 関数) と設定型 (`ConvertConfig`)、エラー型 (`ConvertError`) を定義する。
  3. ライブラリとしての変換ロジック本体は、`cli` モジュールで構築された純粋なデータ構造 `ConvertConfig` を受け取って、`reader` -> `analysis` -> `renderer` パイプラインを結合実行する。

---

## 2. CLI引数仕様

`clap` の `derive` マクロを用いて定義する `CliArgs` のフィールドおよびオプション定義は以下の通りとする。

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "extmd",
    author = "MinamiyamaKotaro",
    version = env!("CARGO_PKG_VERSION"),
    about = "Excel (.xlsx) to Markdown converter with overflow-cell merging support.",
    long_about = None
)]
pub struct CliArgs {
    /// 変換対象のExcelファイル (.xlsx) のパス。
    #[arg(value_name = "INPUT.xlsx")]
    pub input: PathBuf,

    /// 出力先ファイルまたはディレクトリのパス。
    /// 指定がない場合は、標準出力（--split 指定時は入力ファイル名ベースのディレクトリ）へ書き出します。
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// 変換対象にするシート名。複数指定された場合はそれらのみを変換します。
    /// 指定がない場合は、ファイル内の全シートを変換します。
    #[arg(short, long, value_name = "NAME")]
    pub sheet: Vec<String>,

    /// 解析戦略を指定します。"auto" の場合はシート構造から自動選択します。
    #[arg(
        long,
        value_name = "STRATEGY",
        default_value = "auto",
        value_parser = ["auto", "grid-paper", "tabular"]
    )]
    pub strategy: String,

    /// はみ出し判定の感度調整パラメータ（grid-paper戦略で有効）。
    /// 値が小さいほど結合されやすく、大きいほど結合されにくくなります。
    #[arg(long, value_name = "N", default_value_t = 1.0)]
    pub overflow_threshold: f64,

    /// はみ出し結合を無効化し、セル単位でそのまま変換します（--strategy tabular の別名）。
    #[arg(long, conflicts_with = "strategy")]
    pub no_overflow_merge: bool,

    /// シートごとに別々のMarkdownファイルに分割して出力します。
    #[arg(long)]
    pub split: bool,

    /// 出力先ディレクトリ内の既存の Markdown ファイル (.md) を書き込み前に削除します。
    /// （--split 指定時のみ有効）
    #[arg(long, requires = "split")]
    pub clean: bool,

    /// 出力ファイル名またはディレクトリ名に実行日時のタイムスタンプを付与します。
    #[arg(long)]
    pub timestamp: bool,

    /// 詳細なログ（デバッグ情報など）を標準エラー出力へ表示します。
    #[arg(short, long)]
    pub verbose: bool,

    /// 1シートあたりに許容する最大セル数（rows × cols）。
    /// 悪意ある/破損したxlsxファイルがシートの座標情報のみを巨大化させることで
    /// 発生するメモリ枯渇 (DoS) を防ぐための上限（[reader/mod.md 4.1節](reader/mod.md#41-依存ライブラリのセキュリティ検証と監査方針)参照）。
    #[arg(long, value_name = "N", default_value_t = 1_000_000)]
    pub max_cells: usize,
}
```

---

## 3. 設定オブジェクトへのマッピングロジック

`cli.rs` は、パースした `CliArgs` から `lib::ConvertConfig` へマッピングする。マッピングにあたり、各層から持ち越された「CLIとの境界」に関するドメイン知識をここで解決する。

### 3.1. `StrategyConfig` の構築

[analysis::registry.md 1章](analysis/registry.md#1-strategyconfig) の `StrategyConfig` を以下のように組み立てる。

- **解析戦略 (`strategy_id`) の決定**:
  - `no_overflow_merge` が `true` の場合、強制的に `"tabular"` とする。
  - それ以外の場合、`strategy` オプションの値 (`"auto"`, `"grid-paper"`, `"tabular"`) をそのまま指定する。
- **`overflow_threshold`**:
  - 引数の `--overflow-threshold` で指定された値を `StrategyConfig::overflow_threshold` へ渡す。
- **重み設定**:
  - `grid_paper_weights` および `tabular_weights` は、v1ではデフォルト値 (`Default::default()`) を適用する。
- **フォールバックマージン**:
  - `affinity_fallback_margin` は、デフォルト値の `0.05` を適用する。

`StrategyConfig`/`StrategyRegistry`への参照パスは、本節を含む本ドキュメント全体・6章の
コード例では当初`analysis::registry::StrategyConfig`としていたが、`analysis::registry`は
[analysis/mod.md 2章](analysis/mod.md#2-設計方針)の方針により`analysis`外部に公開しない
private moduleであり、`cli`層（`analysis`の外）からは到達できない。実際に`cli`/`lib.rs`から
参照できるのは`analysis/mod.rs`が再エクスポートする`analysis::StrategyConfig`/
`analysis::StrategyRegistry`のみのため、実装ではこちらのパスに統一した
（[analysis/strategies/mod.md](analysis/strategies/mod.md)で`strategies`モジュールの
可視性を`pub(in crate::analysis)`に修正したのと同種の、モジュール外部からの到達可能性に
関する実装時の補正）。`overflow_threshold`以外のフィールドは`StrategyConfig::default()`から
構造体更新構文（`..StrategyConfig::default()`）で引き継ぐため、`grid_paper_weights`/
`tabular_weights`の具体的な型（`analysis::strategies::grid_paper::Weights`等、`analysis`
内部にのみ公開）を`cli`層が直接名指しする必要はない。

### 3.2. `OutputTarget` の構築とタイムスタンプ・クリーンアップ

[renderer/output.md 6章](renderer/output.md#6-cliとの境界) の設計方針に基づき、パスの決定とタイムスタンプの付与は `cli.rs`/`main.rs` (呼び出し側) で確定させ、`OutputTarget` を構築する。

#### A) タイムスタンプサフィックスの生成

`timestamp` フラグが `true` の場合、実行時のローカル日時から `_YYYYMMDD_HHMMSS` 形式のサフィックス文字列を生成する（例: `_20260815_203000`）。

#### B) `split` が `true` の場合 (`OutputTarget::SplitDirectory`)

- **出力先ディレクトリパスの決定**:
  - `output` ( `-o` ) が指定されている場合: 指定されたパスをベースディレクトリとする。
  - `output` ( `-o` ) が指定されていない場合: 入力ファイル名 (`INPUT.xlsx`) の拡張子を除いたベース名 (例: `INPUT`) をベースディレクトリ名とする。
- **タイムスタンプの適用**:
  - `timestamp` が `true` の場合、決定したベースディレクトリ名の末尾にタイムスタンプサフィックスを付与する (例: `output_20260815_203000/`)。
- **クリーンアップ (`clean`)**:
  - `--clean` が `true` の場合、出力先ディレクトリの**ディレクトリ直下の拡張子 `.md` を持つファイルのみ**をすべて削除する。ディレクトリ全体の削除 (`remove_dir_all`) は、ユーザーが誤って重要なフォルダ（例: `/` や `docs/`）を指定した場合の全削除リスクを防ぐため禁止し、対象拡張子のファイル削除に留める。
  - **実行タイミング**: 当初案では`cli::build_config`（`OutputTarget`組み立てと同じタイミング、`convert`呼び出しより前）でこの削除を実行していたが、これだと`convert`が入力ファイル未検出・サイズ上限超過・無効な戦略指定などで後から失敗した場合でも、出力先の既存ファイルが既に削除された後になってしまうという副作用があった（[PR #23レビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/23#pullrequestreview-4944072390)での指摘）。実装では`clean: bool`を`ConvertConfig`のフィールドとして持ち回し、`cli::build_config`は削除処理を一切行わない純粋なマッピングに留める。実際の削除は`lib::convert`が入力の妥当性確認・Reader・Analyzerの全ステップを成功させた後、`renderer::render`を呼ぶ直前（6.1節参照）に実行する。

#### C) `split` が `false` の場合 (`OutputTarget::SingleFile` または `OutputTarget::Stdout`)

- **`output` が指定されている場合 (`OutputTarget::SingleFile`)**:
  - `timestamp` が `true` の場合、出力ファイルの拡張子 (`.md` 等) の直前にタイムスタンプサフィックスを挿入する（例: `-o out.md` -> `out_20260815_203000.md`）。
- **`output` が指定されていない場合 (`OutputTarget::Stdout`)**:
  - 標準出力へ書き出す。
  - `timestamp` が `true` の場合、標準出力は名前を持たないため警告ログを標準エラー出力に出力した上で、タイムスタンプ指定は無視する。

### 3.3. `heading_offset` はCLI側で扱わない

[renderer/mod.md 5章](renderer/mod.md#5-シート見出しレベルと本文見出しレベルの階層関係heading_offset)の`heading_offset_for`が示す通り、シート見出しと本文見出しのオフセットは`renderer::render`が`OutputTarget`から自動算出する内部実装であり、`render(documents: &[domain::Document], target: OutputTarget) -> Result<(), RendererError>`のシグネチャにオフセットを外部注入する引数は存在しない。

`cli.rs`はこの値を算出・上書きせず、`OutputTarget`を組み立てて渡すだけに徹する。CLI側での明示的なオフセット上書き（例: `--heading-offset`）は本設計のv1スコープには含めない（§7参照）。

### 3.4. `max_cells` のマッピングと入力ファイルサイズの上限

[reader/mod.md 4.1節](reader/mod.md#41-依存ライブラリのセキュリティ検証と監査方針)・
[reader/mod.md 5章](reader/mod.md#5-readererror-と公開api)で導入した`max_cells`は、
`--max-cells`の値（デフォルト`1_000_000`）をそのまま`ConvertConfig::max_cells`へ渡す
（6.1節参照）。

これとは別に、ZIP展開・XMLパース自体（`reader::read_sheets`呼び出し）の前段で、
入力ファイルの物理サイズが`MAX_INPUT_FILE_SIZE_BYTES`（100MB）を超える場合は
`lib::convert`が`ConvertError::InputFileTooLarge`で早期に拒否する（6.1節）。
これは`max_cells`（パース後のセル数）とは独立した対策であり、パースが完了する前の
段階で単純に巨大なファイルを弾くための粗いフィルタである
（[reader/mod.md 4.1節](reader/mod.md#41-依存ライブラリのセキュリティ検証と監査方針)の
「多層防御」のMitigation 1に対応。圧縮後サイズしか制限できないため、圧縮率の高い
Zip Bombそのものは防げない残存リスクである点は同節を参照）。

---

## 4. ロギング方針

[reader/validation.md 3章](reader/validation.md#3-破棄無視の方針とログ出力) などのロギング要件を満たすため、標準の `log` クレートと `env_logger` クレートを採用する。

- **ログ出力先**:
  - すべてのログメッセージ (INFO, DEBUG, WARN, ERROR) は**標準エラー出力 (`stderr`)** へ出力する。
  - これにより、`extmd input.xlsx > output.md` のように標準出力をファイルへリダイレクトした際に、ログや警告メッセージが Markdown 本文へ混入するのを防ぐ。
- **ログレベル制御**:
  - デフォルト (`verbose` が `false`): ログレベルは `WARN` とする。エラーや境界外セルの警告 (validation) のみを出力する。
  - `--verbose` 指定時: ログレベルを `DEBUG` とする。解析プロセスのトレースや、適用された戦略の決定ログ等の詳細情報を出力する。なお、出力ファイル上書き時のログ出力要否は[renderer/output.md 7章](renderer/output.md#7-未確定事項)の未確定事項であり、本設計では解決しない（`output.rs`側で決定され次第、`DEBUG`ログとして自然に含まれる想定）。

---

## 5. エラーハンドリングと終了コード

### 5.1. 引数パース時のエラー (Clapによる制御)

引数の不足、不正な型、または衝突するオプション (例: `--strategy` と `--no-overflow-merge` の同時指定) が渡された場合、`clap` が自動でエラーメッセージと使い方 (Usage) を標準エラー出力に表示し、**終了コード `2`** で即座にプロセスを終了する。

### 5.2. 変換プロセス実行時のエラー (`ConvertError`)

`lib::convert` が返すエラー型 `ConvertError` について、`main.rs` は以下の形式で標準エラー出力へメッセージを出力し、**終了コード `1`** で終了する。

| エラー種別 | 原因 | エラーメッセージ形式 (stderr) |
|---|---|---|
| `ConvertError::InputFileNotFound(path)` | 入力ファイルが存在しない | `Error: Input file not found: <path>` |
| `ConvertError::InputFileTooLarge { path, size, limit }` | 入力ファイルの物理サイズが上限（100MB）を超過 | `Error: Input file too large: <path> (<size> bytes, limit: <limit> bytes)` |
| `ConvertError::Reader(err)` | Excelファイルの読込・解析エラー（`ReaderError::SheetTooLarge`によるセル数上限超過を含む、[reader/mod.md 5章](reader/mod.md#5-readererror-と公開api)） | `Error: Failed to read Excel file: <err>` |
| `ConvertError::Renderer(err)` | Markdownファイル書込、ディレクトリ作成等のI/Oエラー | `Error: Failed to write Markdown output: <err>` |
| `ConvertError::InvalidStrategy(name)` | 無効な戦略IDが指定された場合 | `Error: Invalid strategy specified: <name>` |

---

## 6. 公開APIおよびコードシグネチャ

### 6.1. `src/lib.rs`

```rust
pub mod cli; // ライブラリ配下に cli モジュールを含める
pub mod domain;
pub mod reader;
pub mod analysis;
pub mod renderer;

use std::path::PathBuf;

/// 入力ファイルの物理サイズの上限（100MB）。ZIP展開・XMLパース自体が完了する前に
/// 単純に巨大なファイルを弾くための粗いフィルタ（[3.4節](#34-max_cells-のマッピングと入力ファイルサイズの上限)参照）。
const MAX_INPUT_FILE_SIZE_BYTES: u64 = 100 * 1024 * 1024;

/// 変換処理全体を制御する設定オブジェクト。
/// CLI引数に依存しない純粋なデータ型として定義し、単体テストを可能にする。
pub struct ConvertConfig {
    pub input_path: PathBuf,
    pub sheet_names: Vec<String>,
    pub strategy_id: String,
    pub strategy_config: analysis::StrategyConfig,
    pub output_target: renderer::OutputTarget,
    /// `--split`時、出力先ディレクトリ内の既存`.md`ファイルを書き込み前に削除するか
    /// （`--clean`、3.2節B「クリーンアップ」参照）。`cli::build_config`では実行せず、
    /// `convert`が入力の妥当性確認をすべて成功させた後、書き込み直前に実行する。
    pub clean: bool,
    /// 1シートあたりに許容する最大セル数（`--max-cells`、[3.4節](#34-max_cells-のマッピングと入力ファイルサイズの上限)参照）。
    pub max_cells: usize,
}

#[derive(Debug)]
pub enum ConvertError {
    InputFileNotFound(PathBuf),
    InputFileTooLarge { path: PathBuf, size: u64, limit: u64 },
    Reader(reader::ReaderError),
    Renderer(renderer::RendererError),
    InvalidStrategy(String),
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::InputFileNotFound(p) => write!(f, "Error: Input file not found: {}", p.display()),
            ConvertError::InputFileTooLarge { path, size, limit } => write!(
                f,
                "Error: Input file too large: {} ({} bytes, limit: {} bytes)",
                path.display(), size, limit
            ),
            ConvertError::Reader(e) => write!(f, "Error: Failed to read Excel file: {}", e), // {:?} ではなく {} (Display) を使用
            ConvertError::Renderer(e) => write!(f, "Error: Failed to write Markdown output: {}", e), // {:?} ではなく {} (Display) を使用
            ConvertError::InvalidStrategy(s) => write!(f, "Error: Invalid strategy specified: {}", s),
        }
    }
}

impl std::error::Error for ConvertError {}

/// Excelファイルを読み込み、戦略に沿って解析し、Markdownとして書き出す一連のパイプラインを実行します。
pub fn convert(config: ConvertConfig) -> Result<(), ConvertError> {
    // 1. 入力ファイルの存在チェック
    let metadata = std::fs::metadata(&config.input_path)
        .map_err(|_| ConvertError::InputFileNotFound(config.input_path.clone()))?;

    // 1.1. 入力ファイルサイズの上限チェック（3.4節）
    if metadata.len() > MAX_INPUT_FILE_SIZE_BYTES {
        return Err(ConvertError::InputFileTooLarge {
            path: config.input_path,
            size: metadata.len(),
            limit: MAX_INPUT_FILE_SIZE_BYTES,
        });
    }

    // 2. Reader: xlsxの読み込み（max_cellsによるシートサイズ上限チェックを含む、5章参照）
    let all_sheets = reader::read_sheets(&config.input_path, config.max_cells)
        .map_err(ConvertError::Reader)?;

    // 3. 変換対象シートのフィルタリング
    let target_sheets = if config.sheet_names.is_empty() {
        all_sheets
    } else {
        // 指定されたシート名がブック内に存在しない場合、タイポに気付けるよう警告する
        // （デフォルトのログレベルはWARNのため--verbose指定なしでも届く）。
        for name in &config.sheet_names {
            if !all_sheets.iter().any(|s| &s.name == name) {
                log::warn!("Sheet '{name}' not found in the workbook");
            }
        }
        all_sheets
            .into_iter()
            .filter(|s| config.sheet_names.contains(&s.name))
            .collect()
    };

    // 4. StrategyRegistry の初期化
    let registry = analysis::StrategyRegistry::with_config(config.strategy_config);

    // 5. 各シートの変換処理 (Analyzer)
    let mut documents = Vec::new();
    for sheet in target_sheets {
        // 戦略の決定
        let strategy = if config.strategy_id == "auto" {
            registry.select_auto(&sheet)
        } else {
            registry.get(&config.strategy_id)
                .ok_or_else(|| ConvertError::InvalidStrategy(config.strategy_id.clone()))?
        };

        log::info!("Applied strategy '{}' to sheet '{}'", strategy.id(), sheet.name);

        // 分析の実行（analysis/mod.md 3章の通り、heading_offsetはanalyzeの関心事ではない）
        let doc = analysis::analyze(&sheet, strategy);
        documents.push(doc);
    }

    // 6. --clean: 入力の妥当性確認（1〜5）がすべて成功した後、書き込み直前に実行する
    // （3.2節B「クリーンアップ」の実行タイミングに関する補足参照）。
    if config.clean {
        if let renderer::OutputTarget::SplitDirectory(ref dir) = config.output_target {
            clean_split_directory(dir);
        }
    }

    // 7. Renderer: Markdownへのレンダリング & 書き出し
    renderer::render(&documents, config.output_target)
        .map_err(ConvertError::Renderer)?;

    Ok(())
}

/// `--split`の出力先ディレクトリ直下にある拡張子`.md`のファイルのみを削除する。
fn clean_split_directory(dir: &std::path::Path) {
    if !dir.exists() || !dir.is_dir() {
        return;
    }
    log::info!("Cleaning up markdown files in output directory: {}", dir.display());
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Err(err) = std::fs::remove_file(&path) {
                    log::warn!("Failed to remove stale file {}: {}", path.display(), err);
                }
            }
        }
    }
}
```

### 6.2. `src/cli.rs`

`main.rs` から呼ばれる、引数のパースと `ConvertConfig` へのマッピング用のインターフェース。

```rust
use crate::ConvertConfig;
use crate::analysis::StrategyConfig;
use crate::renderer::OutputTarget;
use std::path::PathBuf;

/// CliArgs をパースし、ライブラリで利用可能な ConvertConfig に変換します。
/// 副作用（ファイルシステムへの書き込み・削除）は一切持たない、純粋なマッピング処理と
/// する（`--clean`のファイル削除タイミングに関する3.2節Bの補足を参照。当初案では
/// この関数内で削除を実行していたが、`convert`呼び出し前に削除してしまうと、
/// `convert`が後から失敗した場合に既存の出力ファイルが失われるため`lib::convert`側へ移した）。
pub fn build_config(args: CliArgs) -> Result<ConvertConfig, String> {
    // A) StrategyConfig の組み立て
    let strategy_config = StrategyConfig {
        overflow_threshold: args.overflow_threshold,
        ..StrategyConfig::default()
    };

    let strategy_id = if args.no_overflow_merge {
        "tabular".to_string()
    } else {
        args.strategy
    };

    // B) タイムスタンプ文字列の生成
    let timestamp_suffix = if args.timestamp {
        let now = chrono::Local::now();
        Some(now.format("_%Y%m%d_%H%M%S").to_string())
    } else {
        None
    };

    // C) OutputTarget の組み立て（パスの決定のみ。ファイル削除は行わない）
    let output_target = if args.split {
        let mut base_dir = match args.output {
            Some(out) => out,
            None => {
                let stem = args.input.file_stem()
                    .ok_or_else(|| "Failed to get input file stem".to_string())?;
                PathBuf::from(stem)
            }
        };

        if let Some(ref suffix) = timestamp_suffix {
            let mut name = base_dir.file_name()
                .ok_or_else(|| "Failed to get base dir name".to_string())?
                .to_os_string();
            name.push(suffix);
            base_dir.set_file_name(name);
        }

        OutputTarget::SplitDirectory(base_dir)
    } else {
        // 単一出力モード (連結)
        match args.output {
            Some(mut path) => {
                // タイムスタンプ付与
                if let Some(ref suffix) = timestamp_suffix {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("md");
                        path.set_file_name(format!("{}{}.{}", stem, suffix, ext));
                    }
                }
                OutputTarget::SingleFile(path)
            }
            None => {
                if args.timestamp {
                    log::warn!("--timestamp was specified but outputting to stdout. The timestamp will be ignored.");
                }
                OutputTarget::Stdout
            }
        }
    };

    Ok(ConvertConfig {
        input_path: args.input,
        sheet_names: args.sheet,
        strategy_id,
        strategy_config,
        output_target,
        clean: args.clean,
        max_cells: args.max_cells,
    })
}
```

### 6.3. `src/main.rs`

```rust
use clap::Parser;
use extmd::cli; // ライブラリクレートから cli モジュールを参照

fn main() {
    // 1. コマンドライン引数のパース
    let args = cli::CliArgs::parse();

    // 2. ロギングの初期化
    let log_level = if args.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    };
    
    env_logger::Builder::new()
        .filter(None, log_level)
        .target(env_logger::Target::Stderr) // ログはすべて stderr へ出力
        .init();

    // 3. 設定オブジェクトの構築
    let config = match cli::build_config(args) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    };

    // 4. パイプラインの実行
    if let Err(err) = extmd::convert(config) {
        eprintln!("{}", err);
        std::process::exit(1);
    }
}
```

---

## 7. 未確定事項・考慮点

- **タイムスタンプのタイムゾーン**: `chrono::Local::now()` を用いてローカル時刻で付与するが、コンテナ環境等で UTC となる可能性をドキュメント等で注意喚起する。
- **エラー出力フォーマット**: 本設計では単純な `eprintln!("{}", err)` としているが、より詳細なスタックトレースやコンテキストが必要な場合は将来的に `anyhow` 等の導入を検討する。
- **`heading_offset`のCLI明示指定**: `renderer::render`（[renderer/mod.md 4章](renderer/mod.md#4-公開api)）は`OutputTarget`から`heading_offset`を自動算出する内部実装であり、外部から上書きする引数を持たない。CLIから明示指定できるようにする場合は`render`のシグネチャ拡張が必要になるため、既存のrenderer詳細設計（Issue #8）の再検討を伴う。v1では需要が確認できていないためスコープ外とし、将来必要になった時点で別Issueとして起票する。

---

## 8. 利用上の注意（ローカルCLI限定を想定した設計であることの明記）

[reader/mod.md 4.1節](reader/mod.md#41-依存ライブラリのセキュリティ検証と監査方針)の通り、
extmdが読み込む`.xlsx`のZIP展開・XMLパースは`umya-spreadsheet`のEagerパースに委ねており、
展開後のデータ量に上限を設ける手段が現行ライブラリ構成にはない（Zip Bombに対する残存リスク）。
3.4節の`max_cells`・入力ファイルサイズ上限はいずれもパース完了後、または圧縮後サイズのみに
効く対策であり、パース処理自体の実行中に発生するリソース消費を完全には防げない。

そのため、**extmdはv1では「実行者自身が用意した/信頼して受け取ったファイルをローカルで
変換する」CLIとしての利用を想定する**。不特定多数のユーザーが任意の`.xlsx`をアップロードする
サーバーサイド・マルチテナント環境（Webサービスのバックエンド等）でextmdをそのまま
呼び出す用途は、現行の設計では安全性を保証できないため非推奨とする
（[docs/security/design-review.md](../security/design-review.md)、[Issue #14](https://github.com/MinamiyamaKotaro/extmd/issues/14)）。
この注意は[README](../../README.md)にも記載する。
