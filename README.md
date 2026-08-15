# extmd

Excelファイル（.xlsx）をMarkdownに変換するCLIツール（Rust製）。

> **ステータス: 設計段階。** まだ実装（Cargoプロジェクト）は存在せず、
> `docs/` 配下の要件定義・アーキテクチャ設計を進めている段階です。

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
- [ドメイン設計書](docs/design/domain/mod.md)
- [Reader設計書](docs/design/reader/mod.md)

## 使い方（予定）

実装完了後、以下のようなCLIを想定しています（詳細は要件定義書 5.1節を参照）。

```sh
extmd input.xlsx --strategy auto -o output.md
```

## ディレクトリ構成（予定）

[アーキテクチャ設計書](docs/design/architecture.md)のパイプライン
（Reader → StrategySelector → Analyzer → Renderer）にそのまま対応させた構成を想定しています。
CLI（`main.rs`）はライブラリ（`lib.rs`）を薄く呼び出すだけにし、変換ロジック本体を
`main.rs`を経由せずに単体テストできるようにします。

```
extmd/
├── Cargo.toml
├── src/
│   ├── main.rs              # バイナリエントリポイント。cli.rsをパースしてlib.rsを呼ぶだけ
│   ├── lib.rs                # 公開API（convert()等）。テストや他クレートからの利用を想定
│   ├── cli.rs                 # clapによるCLI引数定義（--strategy, --sheet, -o など）
│   ├── domain/                # コアドメイン型。他のどの層にも依存しない最下層（docs/design/domain/参照）
│   │   ├── mod.rs
│   │   ├── cell.rs               # CellValue, Alignment, FontInfo, Cell
│   │   ├── grid.rs                # Grid<T>（行優先フラット2次元配列）
│   │   ├── sheet.rs                # Sheet, MergeRange
│   │   ├── block.rs                 # Block, BlockSource
│   │   └── document.rs               # RowKind, ResolvedRow, RenderedRow, Document
│   ├── reader/                 # [1] Reader: xlsxファイル → Sheet（docs/design/reader/参照）
│   │   ├── mod.rs
│   │   ├── xlsx.rs               # umya-spreadsheetでの読み込み・他モジュールの統合
│   │   ├── cell_mapper.rs         # umya-spreadsheet::Cell → domain::Cell/CellValueの変換
│   │   ├── date.rs                # Excelシリアル値 → chrono::NaiveDateTime変換
│   │   ├── grid_builder.rs        # 矩形正規化によるdomain::Grid構築
│   │   └── validation.rs          # 結合セル範囲(MergeRange)の境界検証
│   ├── analysis/                # [2]+[3] StrategySelector, Analyzer
│   │   ├── mod.rs
│   │   ├── strategy.rs            # AnalysisStrategy トレイト定義
│   │   ├── registry.rs            # StrategyRegistry / select_auto
│   │   ├── metrics.rs             # SheetMetrics計算（affinity用の特徴量）
│   │   ├── heuristics.rs          # is_overflow_candidate等の共通ロジック
│   │   └── strategies/
│   │       ├── mod.rs
│   │       ├── grid_paper.rs        # GridPaperStrategy
│   │       └── tabular.rs           # TabularStrategy
│   └── renderer/                # [4] Renderer: Vec<Block> → Markdown文字列
│       ├── mod.rs
│       └── markdown.rs
├── tests/
│   ├── fixtures/                # 方眼紙/通常表のサンプルxlsx
│   └── conversion.rs             # 結合テスト・スナップショットテスト
└── docs/
    ├── requirement/
    └── design/
```

各ディレクトリと設計書の対応:

| ディレクトリ | 設計書の該当箇所 |
|---|---|
| `domain/` | アーキテクチャ設計書 3. コアドメイン型 / [ドメイン設計書](docs/design/domain/mod.md)全体（`src/domain/`の各ファイルに`docs/design/domain/`の各mdファイルが対応） |
| `reader/` | アーキテクチャ設計書 2. パイプライン全体像 `[1] Reader` / [Reader設計書](docs/design/reader/mod.md)全体（`src/reader/`の各ファイルに`docs/design/reader/`の各mdファイルが対応） |
| `analysis/` | アーキテクチャ設計書 2. パイプライン全体像 `[2]+[3]` StrategySelector/Analyzer / [Analysis設計書](docs/design/analysis/mod.md)全体（`src/analysis/`の各ファイルに`docs/design/analysis/`の各mdファイルが対応） |

新しいドメイン戦略を追加する際は `analysis/strategies/` に1ファイル追加するだけで、
既存モジュールへの変更が不要になることを意図した構成です（設計書7章）。

## ロードマップ

1. 要件定義（完了）
2. アーキテクチャ設計（進行中）
3. セキュリティ観点の整理
4. 実装・テスト

## ライセンス

MIT License
