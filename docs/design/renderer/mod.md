# `renderer::mod` 設計書

対象: [アーキテクチャ設計書 2章「パイプライン全体像」](../architecture.md#2-パイプライン全体像) `[4] Renderer` の詳細化。
[README](../../../README.md)のディレクトリ構成における `src/renderer/` に対応する。

`docs/design/renderer/` は `src/renderer/` のファイル構成と1:1で対応させる
（[domain/mod.md 1章](../domain/mod.md#1-対応表)の運用ルールを踏襲）。
本設計は[Issue #8](https://github.com/MinamiyamaKotaro/extmd/issues/8)での検討を反映したもの。

## 1. 対応表

| `src/renderer/` | `docs/design/renderer/` | 内容 |
|---|---|---|
| `mod.rs` | [mod.md](mod.md)（このファイル） | 設計方針・公開API（`render`/`OutputTarget`）・`Document`から本文への組み立て |
| `flow.rs` | [flow.md](flow.md) | `RowKind::Flow`行の変換（段落・見出し） |
| `table.rs` | [table.md](table.md) | `RowKind::TableRow`の連続行のグループ化とMarkdownパイプテーブル構築 |
| `escape.rs` | [escape.md](escape.md) | Markdown特殊文字のエスケープ純粋関数群 |
| `output.rs` | [output.md](output.md) | `OutputTarget`に基づく書き込み、ファイル名サニタイズ・衝突検知 |

## 2. モジュール分割の経緯

[Issue #8の最初のコメント](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301901971)で、
段落・見出しの出力、パイプテーブルの構築、セル結合の表現、特殊文字のエスケープ、出力先の制御を
すべて1ファイルに持たせると責務が肥大化する懸念が指摘され、`reader`/`analysis`層と同様
（[reader/mod.md 2章](../reader/mod.md#2-モジュール分割の経緯)）に上記5ファイルへ分割する方針とした。

## 3. 設計方針

- `renderer`は[domain/mod.md 2章](../domain/mod.md#2-設計方針)の依存方向の方針に従い、
  `domain`にのみ依存する。[アーキテクチャ設計書2章](../architecture.md#2-パイプライン全体像)が明記する
  「RendererはStrategyに依存しない」方針の通り、`analysis`には一切依存しない。
- [analysis/mod.md 2章](../analysis/mod.md#2-設計方針)と同様、内部モジュール
  （`flow.rs`/`table.rs`/`escape.rs`/`output.rs`）は`renderer`外部に直接公開しない。
  外部から見えるのは本ファイルの公開API（4章）のみとする。各ファイルの設計書（[flow.md](flow.md)/
  [table.md](table.md)/[escape.md](escape.md)/[output.md](output.md)）のコード例では
  モジュール間の関数を`pub(crate)`としていたが、これだと`main.rs`等`renderer`の外から
  直接到達できてしまい本方針と矛盾するため、実装では`pub(in crate::renderer)`を採用し
  `renderer`部分木の外には公開されないようにした（[analysis/strategies/mod.md](../analysis/strategies/mod.md)の
  `pub(in crate::analysis)`と同じ考え方）。
- `output.rs`はCLIフラグの意味論（`--split`の有無や`-o`の値の解釈、タイムスタンプ付与オプション等）
  を一切持たない。CLI引数から具体的な出力先を組み立てる処理は`renderer`層の外
  （`cli.rs`/`lib.rs`）の責務とする（[analysis/registry.md 5章「CLIとの境界」](../analysis/registry.md#5-cliとの境界)と
  同じ考え方。[Issue #8での合意](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301979135)）。

## 4. 公開API

```rust
#[derive(Debug)]
pub enum OutputTarget {
    Stdout,
    SingleFile(std::path::PathBuf),
    SplitDirectory(std::path::PathBuf),
}

#[derive(Debug)]
pub enum RendererError {
    Io(std::io::Error),
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RendererError::Io(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for RendererError {}

/// `Analyzer`が生成した各シートの`Document`をMarkdownへ変換し、`target`へ書き出す。
/// `target`の構築（CLI引数の解釈）は呼び出し側（`lib.rs`）の責務とし、
/// `render`は既に決定済みの`OutputTarget`を受け取るだけとする
/// （[reader/mod.md 5章](../reader/mod.md#5-readererror-と公開api)と同じ「エントリポイントは
/// 横断的関心事のみ」という方針を踏襲）。
pub fn render(documents: &[domain::Document], target: OutputTarget) -> Result<(), RendererError> {
    let heading_offset = heading_offset_for(&target);

    match target {
        OutputTarget::Stdout => {
            output::write_stdout(&render_concatenated(documents, heading_offset))
        }
        OutputTarget::SingleFile(path) => {
            output::write_single_file(&path, &render_concatenated(documents, heading_offset))
        }
        OutputTarget::SplitDirectory(dir) => {
            let bodies = documents
                .iter()
                .map(|doc| (doc.sheet_name.clone(), render_body(doc, heading_offset)))
                .collect();
            output::write_split(&dir, bodies)
        }
    }
}
```

## 5. シート見出しレベルと本文見出しレベルの階層関係（`heading_offset`）

`OutputTarget::Stdout`/`SingleFile`（複数シートを1つのMarkdownへ連結する出力）では、
各シートの境界としてシート名をH1見出し（`# シート名`）として挿入する。一方で
`Block.heading_level`（[domain/block.md](../domain/block.md#2-block)、`1〜6`）は
シート名の存在を知らずに算出されるため、そのまま出力すると本文中の`heading_level: Some(1)`が
シート名と同じH1になり、見出し階層（シートタイトル配下に本文見出しがネストする構造）が崩れる。

これを避けるため、出力モードに応じて本文側の見出しレベルに一律のオフセットをかける
（[Issue #8での決定](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301913354)）。

```rust
fn heading_offset_for(target: &OutputTarget) -> u8 {
    match target {
        // シート名がH1を占有するため、本文の見出しは+1オフセットしてH2始まりにする
        OutputTarget::Stdout | OutputTarget::SingleFile(_) => 1,
        // ファイル名自体がシートタイトルの役割を果たすため、本文はheading_levelをそのまま使う
        OutputTarget::SplitDirectory(_) => 0,
    }
}
```

`heading_offset`は`mod.rs`が`OutputTarget`から算出し、[flow.md](flow.md)の変換関数へ引数として
渡す。`flow.rs`自身は`OutputTarget`を知らない（3章の「内部モジュールを外部に公開しない」方針と
同様、モジュール境界を保つため。[Issue #8での決定](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301923152)）。

## 6. `Document`から本文への組み立て

```rust
fn render_concatenated(documents: &[domain::Document], heading_offset: u8) -> String {
    documents
        .iter()
        .map(|doc| {
            let heading = format!("# {}", escape::escape_flow_text(&doc.sheet_name));
            format!("{heading}\n\n{}", render_body(doc, heading_offset))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `doc.rows`（`RowKind`混在）を、連続する`TableRow`をひとまとまりのテーブルとして
/// グループ化しながら本文Markdownへ変換する。
fn render_body(doc: &domain::Document, heading_offset: u8) -> String {
    let mut parts = Vec::new();
    let mut i = 0;

    while i < doc.rows.len() {
        match doc.rows[i].kind {
            domain::RowKind::Flow => {
                parts.push(flow::render_row(&doc.rows[i], heading_offset));
                i += 1;
            }
            domain::RowKind::TableRow => {
                let start = i;
                while i < doc.rows.len() && matches!(doc.rows[i].kind, domain::RowKind::TableRow) {
                    i += 1;
                }
                parts.push(table::render_table(&doc.rows[start..i]));
            }
        }
    }

    parts.join("\n\n")
}
```

連続する`TableRow`をまとめて`table::render_table`に渡す理由は[table.md 1章](table.md#1-責務)を参照。

## 7. 未確定事項

- `render_concatenated`/`render_body`が挟む区切り（`\n\n`）や末尾改行の厳密な仕様は
  実装・スナップショットテストの段階で確定させる
- ~~`SplitDirectory`時、シートが1件も存在しない（空のワークブック）場合の挙動
  （空ディレクトリを作るだけで正常終了するか、エラーにするか）~~
  → `renderer::render`自体は`documents`が空でもエラーにせず空出力/空ディレクトリを
  作成する寛容な実装のまま変更していないが、実際のCLIパイプライン（`lib::convert`）側で
  変換対象シートが0件の場合に`ConvertError::NoSheetsToConvert`を返し、`renderer::render`
  に到達する前に弾くよう決着した（[cli.md 5.2節](../cli.md#52-変換プロセス実行時のエラー-converterror)、Issue #34）。
  `renderer`を直接ライブラリとして呼ぶ他の利用者のために、`render`自体の寛容な挙動は
  意図的に維持している。
