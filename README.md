# extmd

[![Rust CI](https://github.com/MinamiyamaKotaro/extmd/actions/workflows/rust-ci.yml/badge.svg)](https://github.com/MinamiyamaKotaro/extmd/actions/workflows/rust-ci.yml)
[![extmd on crates.io](https://img.shields.io/crates/v/extmd.svg)](https://crates.io/crates/extmd)
[![extmd on docs.rs](https://docs.rs/extmd/badge.svg)](https://docs.rs/extmd)
[![codecov](https://codecov.io/gh/MinamiyamaKotaro/extmd/branch/master/graph/badge.svg)](https://codecov.io/gh/MinamiyamaKotaro/extmd)

Excelファイル（.xlsx）をMarkdownに変換するCLIツール（Rust製）。

> **ステータス: v1実装完了。** 設計（要件定義・アーキテクチャ設計・詳細設計・セキュリティレビュー）
> から[Issue #17](https://github.com/MinamiyamaKotaro/extmd/issues/17)に基づく実装
> （domain → reader → analysis → renderer → cli）まで完了し、CLIとして動作します。

## 特徴

日本の業務でよく使われる「Excel方眼紙」（セルを方眼紙のマス目のように小さく均一にし、
罫線でレイアウトした文書テンプレート）を単純にセル単位でMarkdownテーブル化すると、
文章がセルごとにブツ切りになり可読性を失います。

extmdは、セルからはみ出して表示されている文字列を検出し、隣接する空セルと結合して
1つの論理的なブロック（見出し・段落など）として扱ってから変換します。
一方で通常の集計表はそのままMarkdownテーブルとして出力します。

シートの構造的特徴からどちらの解析ルールを適用すべきかを自動判定する
（明示指定も可能な）設計を採用しています。

## インストール

### crates.ioから（Rustツールチェーンがある場合）

```sh
cargo install extmd
```

### プリビルドバイナリ（Rustツールチェーン不要）

[Releases](https://github.com/MinamiyamaKotaro/extmd/releases)ページから、お使いのOS向けの
アーカイブ（Linux/macOS/Windows、x86_64/aarch64）をダウンロードし、`extmd`（Windowsは
`extmd.exe`）をPATHの通ったディレクトリに配置してください。

## ドキュメント

- [要件定義書](docs/requirement/requirements.md)
- [アーキテクチャ設計書](docs/design/architecture.md)
- [Domain設計書](docs/design/domain/mod.md)
- [Reader設計書](docs/design/reader/mod.md)
- [Analysis設計書](docs/design/analysis/mod.md)
- [Renderer設計書](docs/design/renderer/mod.md)
- [CLI設計書](docs/design/cli.md)
- [セキュリティ設計レビュー](docs/security/design-review.md)

## 使い方

詳細なCLIオプション一覧は`extmd --help`、または[CLI設計書](docs/design/cli.md)を参照してください。

```sh
# ターミナル画面に変換結果を表示する（-oを指定しない場合。解析戦略は自動判定）
extmd input.xlsx

# 結果をファイルへ保存する（-o 出力先ファイル名）
extmd input.xlsx -o output.md

# シートごとに別々のMarkdownファイルへ分割して保存する（-o 出力先ディレクトリ名）
extmd input.xlsx --split -o output_dir

# 解析戦略を明示的に指定する（省略時はシートの内容から自動判定）
# grid-paper: Excel方眼紙向け（はみ出し文字列を結合してから変換）
# tabular   : 通常の集計表向け（セルをそのままテーブルとして変換）
extmd input.xlsx --strategy grid-paper -o output.md
```

## 利用上の注意

extmdはv1では、**実行者自身が用意した/信頼して受け取った`.xlsx`ファイルをローカルで
変換するCLIとしての利用**を想定しています。不特定多数のユーザーが任意の`.xlsx`を
アップロードするサーバーサイド・マルチテナント環境（Webサービスのバックエンドとして
extmdをそのまま呼び出す等）での利用は、悪意あるファイルに対する耐性が現行設計では
十分でないため非推奨です。詳細は[セキュリティ設計レビュー](docs/security/design-review.md)・
[CLI設計書8章](docs/design/cli.md#8-利用上の注意ローカルcli限定を想定した設計であることの明記)を参照してください。

## ディレクトリ構成

[アーキテクチャ設計書](docs/design/architecture.md)のパイプライン
（Reader → StrategySelector → Analyzer → Renderer）にそのまま対応させた構成にしています。
CLI（`main.rs`）はライブラリ（`lib.rs`）を薄く呼び出すだけにし、変換ロジック本体を
`main.rs`を経由せずに単体テストできるようにします。全レイヤー実装済みです。

```
extmd/
├── Cargo.toml
├── src/
│   ├── main.rs              # 実装済み。薄いバイナリエントリポイント。cli::build_config()を呼び、extmd::convert()を実行するだけ
│   ├── lib.rs                # 実装済み。公開API: convert()、ConvertConfig、ConvertError(docs/design/cli.md 6.1節参照)
│   ├── cli.rs                 # 実装済み。clapによるCLI引数定義（--strategy, --sheet, -o, --split, --clean等）とConvertConfigへの変換（docs/design/cli.md参照）
│   ├── domain/                # 実装済み。コアドメイン型。他のどの層にも依存しない最下層（docs/design/domain/参照）
│   │   ├── mod.rs
│   │   ├── cell.rs               # CellValue, Alignment, FontInfo, Cell
│   │   ├── grid.rs                # Grid<T>（行優先フラット2次元配列）
│   │   ├── sheet.rs                # Sheet, MergeRange
│   │   ├── block.rs                 # Block, BlockSource
│   │   └── document.rs               # RowKind, ResolvedRow, RenderedRow, Document
│   ├── reader/                 # 実装済み。[1] Reader: xlsxファイル → Sheet（docs/design/reader/参照）
│   │   ├── mod.rs
│   │   ├── xlsx.rs               # umya-spreadsheetでの読み込み・他モジュールの統合
│   │   ├── cell_mapper.rs         # umya-spreadsheet::Cell → domain::Cell/CellValueの変換
│   │   ├── date.rs                # Excelシリアル値 → chrono::NaiveDateTime変換
│   │   ├── grid_builder.rs        # 矩形正規化によるdomain::Grid構築
│   │   └── validation.rs          # 結合セル範囲(MergeRange)の境界検証
│   ├── analysis/                # 実装済み。[2]+[3] StrategySelector, Analyzer（docs/design/analysis/参照）
│   │   ├── mod.rs
│   │   ├── strategy.rs            # AnalysisStrategy トレイト定義
│   │   ├── registry.rs            # StrategyRegistry / StrategyConfig / select_auto
│   │   ├── metrics.rs             # SheetMetrics計算（affinity用の特徴量）
│   │   ├── heuristics.rs          # is_overflow_candidate等の共通ロジック
│   │   └── strategies/
│   │       ├── mod.rs
│   │       ├── grid_paper.rs        # GridPaperStrategy（デフォルト、方眼紙向け）
│   │       └── tabular.rs           # TabularStrategy（通常の集計表向け）
│   └── renderer/                # 実装済み。[4] Renderer: Document → Markdown文字列（docs/design/renderer/参照）
│       ├── mod.rs                 # render()公開API、OutputTarget、heading_offset算出、Documentの本文組み立て
│       ├── flow.rs                # RowKind::Flow行の変換（段落・見出し）
│       ├── table.rs               # RowKind::TableRow行のMarkdownパイプテーブル変換
│       ├── escape.rs              # Markdown特殊文字のエスケープ
│       └── output.rs              # OutputTargetに基づく書き込み・ファイル名サニタイズ
├── examples/
│   └── gen_fixtures.rs          # tests/fixtures/*.xlsxを生成するスクリプト（cargo run --example gen_fixtures）
├── tests/
│   ├── reader.rs                # readerの結合テスト（umya-spreadsheetの実writer/readerを介した往復検証）
│   ├── analysis.rs              # tests/fixtures/の実xlsxからselect_autoの戦略選択・ネイティブ結合解決を検証
│   ├── cli.rs                   # CLI全体の結合テスト（コンパイル済みバイナリを実際に起動して検証）
│   └── fixtures/                # 方眼紙/通常表/申請書/議事録/混在ワークブックのサンプルxlsx（tests/analysis.rsが使用）
└── docs/
    ├── requirement/
    ├── design/
    └── security/
```

各ディレクトリと設計書の対応:

| ディレクトリ | 設計書の該当箇所 |
|---|---|
| `domain/` | アーキテクチャ設計書 3. コアドメイン型 / [Domain設計書](docs/design/domain/mod.md)全体（`src/domain/`の各ファイルに`docs/design/domain/`の各mdファイルが対応） |
| `reader/` | アーキテクチャ設計書 2. パイプライン全体像 `[1] Reader` / [Reader設計書](docs/design/reader/mod.md)全体（`src/reader/`の各ファイルに`docs/design/reader/`の各mdファイルが対応） |
| `analysis/` | アーキテクチャ設計書 2. パイプライン全体像 `[2]+[3]` StrategySelector/Analyzer / [Analysis設計書](docs/design/analysis/mod.md)全体（`src/analysis/`の各ファイルに`docs/design/analysis/`の各mdファイルが対応） |
| `renderer/` | アーキテクチャ設計書 2. パイプライン全体像 `[4]` Renderer / [Renderer設計書](docs/design/renderer/mod.md)全体（`src/renderer/`の各ファイルに`docs/design/renderer/`の各mdファイルが対応） |
| `cli.rs`/`main.rs`/`lib.rs` | [CLI設計書](docs/design/cli.md)全体（引数定義・`ConvertConfig`への変換・`reader`→`analysis`→`renderer`パイプラインの結合実行） |

新しいドメイン戦略を追加する際は `analysis/strategies/` に1ファイル追加するだけで、
既存モジュールへの変更が不要になることを意図した構成です（設計書7章）。

## ロードマップ

1. 要件定義（完了）
2. アーキテクチャ設計・詳細設計（完了）
3. セキュリティ観点の整理（完了、[セキュリティ設計レビュー](docs/security/design-review.md)）
4. 実装・テスト（完了、[Issue #17](https://github.com/MinamiyamaKotaro/extmd/issues/17)）
   - [x] `domain`層
   - [x] `reader`層
   - [x] `analysis`層
   - [x] `renderer`層
   - [x] `cli`（`main.rs`/`cli.rs`/`lib.rs`公開API）

## リリース手順（メンテナ向け）

`v[0-9]+.[0-9]+.[0-9]+`形式のタグをpushすると、`.github/workflows/release.yml`が
以下を自動実行します。

1. `fmt`/`clippy`/`test`の再検証（タグが指すコミットがCIを通過済みとは限らないため）
2. GitHub Releaseの作成
3. Linux/macOS/Windows（x86_64/aarch64）向けバイナリのビルドとReleaseへのアップロード
4. crates.ioへの`cargo publish`

手順:

```sh
# 1. Cargo.tomlのversionを更新してコミット
# 2. タグを打ってpush（vプレフィックス必須、Cargo.tomlのversionと一致させる）
git tag v0.1.0
git push origin v0.1.0
```

初回のみ、リポジトリのSecrets（Settings > Secrets and variables > Actions）に
crates.ioの発行したAPIトークンを`CARGO_REGISTRY_TOKEN`として登録しておく必要があります
（crates.ioにログイン後、Account Settings > API Tokensから発行）。

## ライセンス

MIT License
