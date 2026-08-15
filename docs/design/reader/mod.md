# `reader::mod` 設計書

対象: [アーキテクチャ設計書 2章「パイプライン全体像」](../architecture.md#2-パイプライン全体像) `[1] Reader` の詳細化。
[README](../../../README.md)のディレクトリ構成における `src/reader/` に対応する。

`docs/design/reader/` は `src/reader/` のファイル構成と1:1で対応させる
（[domain/mod.md 1章](../domain/mod.md#1-対応表)の運用ルールを踏襲）。
このファイル（`mod.md`）は `mod.rs` に対応し、モジュール全体の設計方針と、
個別ファイルの型定義には属さない横断的な設計判断をまとめる。

## 1. 対応表

| `src/reader/` | `docs/design/reader/` | 内容 |
|---|---|---|
| `mod.rs` | [mod.md](mod.md)（このファイル） | 設計方針・モジュール構成・`ReaderError`・公開API |
| `xlsx.rs` | [xlsx.md](xlsx.md) | ファイル・ワークブック・ワークシート操作のライフサイクル管理、他モジュールの統合 |
| `cell_mapper.rs` | [cell_mapper.md](cell_mapper.md) | `umya_spreadsheet::Cell` → `domain::Cell`/`CellValue` の変換 |
| `date.rs` | [date.md](date.md) | Excelシリアル値 → `chrono::NaiveDateTime` の変換 |
| `grid_builder.rs` | [grid_builder.md](grid_builder.md) | 矩形正規化による `domain::Grid<Cell>` の構築 |
| `validation.rs` | [validation.md](validation.md) | `MergeRange` の境界検証 |

## 2. モジュール分割の経緯

[Issue #4](https://github.com/MinamiyamaKotaro/extmd/issues/4)の検討過程で、
`xlsx.rs` 1ファイルにファイルI/O・値と書式のマッピング・日付変換・矩形正規化・
結合セル検証のすべてを持たせると責務が肥大化することが判明したため、
上記6ファイルに分割する方針とした。`xlsx.rs` は他の子モジュールを呼び出して
処理を統合する薄い層とし、変換ロジック自体（日付変換・矩形正規化・境界検証）は
Excelファイルを読み込まずに純粋な値・座標のモックだけで単体テスト可能な形にする。

## 3. 設計方針

- `reader/` は [domain/mod.md 2章](../domain/mod.md#2-設計方針)の依存方向の方針に従い、
  `domain` にのみ依存する。`analysis`/`renderer` には依存しない。
- `reader` はI/O層であるため、`domain` とは異なりエラー処理・外部ライブラリ呼び出しを
  正当に持つ。ただし変換ロジック（`date.rs`/`grid_builder.rs`/`validation.rs`）は
  umya-spreadsheetの型に直接依存させず、素の値・座標を受け取る形にして
  テスト容易性を確保する（2章）。
- `cell_mapper.rs`/`date.rs`/`grid_builder.rs`/`validation.rs` はいずれも
  `xlsx.rs` からのみ呼ばれる内部モジュールとし、`reader` の外部（`analysis`/`main.rs`等）
  に公開するのは `mod.rs` の公開APIと `ReaderError` のみとする。

## 4. 使用ライブラリの決定: `umya-spreadsheet`

[要件定義書 7章](../../requirement/requirements.md#7-技術スタック候補)で候補として挙げた
`calamine`/`umya-spreadsheet`のうち、**`umya-spreadsheet`を採用する。**

理由: 本ツールの中核機能である「セルのはみ出し判定」（要件定義書 5.3.2）には、
列幅（`Column::width()`）・折り返し設定（`Alignment::wrap_text()`）・
フォントサイズ/太字（`Font::size()`/`Font::bold()`）・文字揃え（`Alignment::horizontal()`）の
取得が不可欠である。標準の `calamine` は値のみの高速抽出に特化しており、これらの
スタイル情報を取得できない。`umya-spreadsheet` はスタイル情報を`Cell::style()`経由で
詳細に取得できるため、要件を満たす。

Eagerパース（ファイル全体を一括読み込み）によるパフォーマンス懸念はあるが、
対象となる方眼紙シートの規模（[非機能要件](../../requirement/requirements.md#6-非機能要件)より
数千セル程度）を考慮すると、CLI変換ツールとして実用上問題ないと判断する。

## 5. `ReaderError` と公開API

```rust
#[derive(Debug)]
pub enum ReaderError {
    /// ファイルが存在しない、権限がない等のI/Oエラー。
    Io(std::io::Error),
    /// xlsxとして不正な形式・破損したファイル（umya-spreadsheetのパースエラーをラップ）。
    Parse(String),
}

/// 指定した `.xlsx` ファイルの全シートを読み込み、`domain::Sheet` の列へ変換する。
/// シートの絞り込み（要件定義書 5.1 `-s`/`--sheet`）は呼び出し側（`lib.rs`）の責務とし、
/// `reader` は常に全シートを返す（umya-spreadsheetはEagerパースのため、
/// 読み込み時点でのフィルタリングによる性能上の利点がないため）。
pub fn read_sheets(path: &std::path::Path) -> Result<Vec<domain::Sheet>, ReaderError> {
    xlsx::read_sheets(path)
}
```

`read_sheets` は `xlsx.rs` の実装へ薄く委譲するだけとし、`mod.rs` 自体はロジックを持たない
（[domain/mod.md 2章](../domain/mod.md#2-設計方針)の「エントリポイントは横断的関心事のみ」という
方針を `reader` にも適用する）。

## 6. 未確定事項

- 存在しない/破損している/非対応形式のファイルに対するエラーメッセージの具体的な文面
  （[要件定義書 5.2](../../requirement/requirements.md#52-入力)「原因が特定しやすいメッセージ」との整合は
  実装フェーズで詰める）
- `ReaderError::Parse` が保持する文字列の情報量（umya-spreadsheet側のエラー型をどこまで
  透過的に保持するか）
