# extmd

Excelファイル（.xlsx）をMarkdownに変換するCLIツール（Rust製）。

> **ステータス: 実装中。** 設計（要件定義・アーキテクチャ設計・詳細設計・セキュリティレビュー）は
> 完了し、[Issue #17](https://github.com/MinamiyamaKotaro/extmd/issues/17)に基づき
> `docs/design/architecture.md`のパイプライン順（domain → reader → analysis → renderer → cli）で
> 実装を進めています。現時点では`domain`層・`reader`層が実装済みで、CLIとして動作する状態には
> まだ達していません。

## 特徴

日本の業務でよく使われる「Excel方眼紙」（セルを方眼紙のマス目のように小さく均一にし、
罫線でレイアウトした文書テンプレート）を単純にセル単位でMarkdownテーブル化すると、
文章がセルごとにブツ切りになり可読性を失います。

extmdは、セルからはみ出して表示されている文字列を検出し、隣接する空セルと結合して
1つの論理的なブロック（見出し・段落など）として扱ってから変換します。
一方で通常の集計表はそのままMarkdownテーブルとして出力します。

シートの構造的特徴からどちらの解析ルールを適用すべきかを自動判定する
（明示指定も可能な）設計を採用しています。

## ドキュメント

- [要件定義書](docs/requirement/requirements.md)
- [アーキテクチャ設計書](docs/design/architecture.md)
- [Domain設計書](docs/design/domain/mod.md)
- [Reader設計書](docs/design/reader/mod.md)
- [Analysis設計書](docs/design/analysis/mod.md)
- [Renderer設計書](docs/design/renderer/mod.md)
- [CLI設計書](docs/design/cli.md)
- [セキュリティ設計レビュー](docs/security/design-review.md)

## 使い方（予定）

実装完了後、以下のようなCLIを想定しています（詳細は要件定義書 5.1節を参照）。

```sh
extmd input.xlsx --strategy auto -o output.md
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
`main.rs`を経由せずに単体テストできるようにします。`domain/`・`reader/`は実装済み、それ以外
（`analysis/`・`renderer/`・`cli.rs`・`main.rs`）は[Issue #17](https://github.com/MinamiyamaKotaro/extmd/issues/17)で今後実装予定です。

```
extmd/
├── Cargo.toml
├── src/
│   ├── main.rs              # (未実装) バイナリエントリポイント。cli::build_config()を呼び、extmd::convert()を実行するだけ
│   ├── lib.rs                # 公開API。現状はpub mod domain; pub mod reader;のみ（convert()等はcli実装時に追加）
│   ├── cli.rs                 # (未実装) clapによるCLI引数定義（--strategy, --sheet, -o, --split, --clean等）とConvertConfigへの変換（docs/design/cli.md参照）
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
│   ├── analysis/                # (未実装) [2]+[3] StrategySelector, Analyzer
│   │   ├── mod.rs
│   │   ├── strategy.rs            # AnalysisStrategy トレイト定義
│   │   ├── registry.rs            # StrategyRegistry / select_auto
│   │   ├── metrics.rs             # SheetMetrics計算（affinity用の特徴量）
│   │   ├── heuristics.rs          # is_overflow_candidate等の共通ロジック
│   │   └── strategies/
│   │       ├── mod.rs
│   │       ├── grid_paper.rs        # GridPaperStrategy
│   │       └── tabular.rs           # TabularStrategy
│   └── renderer/                # (未実装) [4] Renderer: Document → Markdown文字列
│       ├── mod.rs
│       ├── flow.rs                # RowKind::Flow行の変換（段落・見出し）
│       ├── table.rs               # RowKind::TableRow行のMarkdownパイプテーブル変換
│       ├── escape.rs              # Markdown特殊文字のエスケープ
│       └── output.rs              # OutputTargetに基づく書き込み・ファイル名サニタイズ
├── tests/
│   ├── reader.rs                # readerの結合テスト（umya-spreadsheetの実writer/readerを介した往復検証）
│   └── fixtures/                # (未実装) 方眼紙/通常表のサンプルxlsx
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

新しいドメイン戦略を追加する際は `analysis/strategies/` に1ファイル追加するだけで、
既存モジュールへの変更が不要になることを意図した構成です（設計書7章）。

## ロードマップ

1. 要件定義（完了）
2. アーキテクチャ設計・詳細設計（完了）
3. セキュリティ観点の整理（完了、[セキュリティ設計レビュー](docs/security/design-review.md)）
4. 実装・テスト（進行中、[Issue #17](https://github.com/MinamiyamaKotaro/extmd/issues/17)）
   - [x] `domain`層
   - [x] `reader`層
   - [ ] `analysis`層
   - [ ] `renderer`層
   - [ ] `cli`（`main.rs`/`cli.rs`/`lib.rs`公開API）

## ライセンス

MIT License
