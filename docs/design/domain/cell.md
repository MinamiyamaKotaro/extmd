# `domain::cell` 設計書

対象: [domain/mod.md](mod.md)の対応表における `cell.rs`。

## 1. `CellValue`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Empty,
    String(String),
    Number(f64),
    Date(chrono::NaiveDateTime),
    Bool(bool),
}

impl CellValue {
    pub fn is_empty(&self) -> bool {
        matches!(self, CellValue::Empty)
    }

    /// Markdown出力やはみ出し幅推定に使う文字列表現。
    pub fn display_text(&self) -> String {
        match self {
            CellValue::Empty => String::new(),
            CellValue::String(s) => s.clone(),
            CellValue::Number(n) => format_number(*n),
            CellValue::Date(d) => format_date(*d),
            CellValue::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
        }
    }
}
```

数式セルは reader層（`reader::xlsx`）で計算済みの値に解決してから `CellValue` に格納する。
domainは数式そのものを表現しない（[要件定義書 4.2「対象外」](../../requirement/requirements.md#42-対象外v1では扱わない)と対応）。

**`format_number`/`format_date` の宣言場所:** `cell.rs` 内に定義する非公開（モジュール外に
公開しない）関数とする。`display_text` から見て実装詳細に過ぎず、`CellValue` の外から
直接呼ぶ必要はないため。内部で4章の書式ロジック（`ssfmt`採用が決まればそれへの委譲、
决まらなければ自前実装）を呼び出すラッパーとして働く。呼び出し元（domain外の
reader/analysis/renderer層）から見えるのは `CellValue::display_text()` のみで、
書式ライブラリの選定（[Issue #2](https://github.com/MinamiyamaKotaro/extmd/issues/2)）が
どちらに転んでも `cell.rs` 内の実装だけを差し替えれば済む
（[PR #3のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/3#issuecomment-5301554119)での指摘を反映）。

**日付/日時ライブラリの決定: `chrono`。** Reader候補である`calamine`が公式に
`chrono` フィーチャー（`chrono::NaiveDate`/`NaiveDateTime`/`NaiveTime` への変換ヘルパー）
を提供しており、日付セルの変換をReader層でそのまま利用できるエコシステム親和性を
唯一の決定根拠とする。（`time`クレートとの比較で語られがちなセキュリティ上の優劣は、
過去の関連脆弱性が当時両クレートに同根で存在したものであり、決定根拠には含めない。
[Issue #1のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/issues/1)参照。）

## 2. `Alignment` / `FontInfo`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontInfo {
    pub size_pt: f32,
    pub bold: bool,
}
```

`Alignment` は要件定義書 4.2 により v1では `Right` の情報を保持はするが、
はみ出し判定・レンダリングでは `Left` 以外を特別扱いしない
（右揃えセルの左方向はみ出しはv1スコープ外）。

## 3. `Cell`

```rust
#[derive(Debug, Clone)]
pub struct Cell {
    pub value: CellValue,
    pub column_width: f64,   // 列幅（文字数換算）
    pub wrap_text: bool,
    pub alignment: Alignment,
    pub font: FontInfo,
    pub number_format: Option<String>, // 表示形式コード（例: "#,##0", "yyyy/m/d"）。未設定なら None
}

impl Cell {
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}
```

**`number_format`フィールドについて:** `display_text()`（1章）が呼ぶ `format_number`/`format_date`
がExcelの表示形式（桁区切り・パーセンテージ・和暦等、4章）を再現するには、そのセルの
表示形式コードが必要になる。Readerがこれを抽出して格納できるよう、`Cell`にフィールドとして
追加した（[Issue #4のreader層設計](https://github.com/MinamiyamaKotaro/extmd/issues/4)で決定。
詳細は[reader/cell_mapper.md 6章](../reader/cell_mapper.md#6-number_format-フィールドの追加domain層への変更)参照）。

**設計判断:** `column_width` は本来「列」の属性だが、`Cell` にも複製して持たせる。
はみ出し判定（`is_overflow_candidate(cell, next, threshold)`）が `Cell` 単体を見るだけで
完結でき、判定ロジックが `Sheet` の列メタデータを持ち回らずに済むため。
デメリットは列幅データの重複だが、方眼紙シートの想定規模（数千セル程度、
[非機能要件](../../requirement/requirements.md#6-非機能要件)参照）では無視できるコストと判断した。

座標（行・列インデックス）の扱いは [mod.md 3章](mod.md#3-座標の表現rowindexcolindexの検討)を参照。
`Cell` 自身は自分の座標を持たない（`Grid` 側でインデックス管理する）。

## 4. `format_number` / `format_date` のフォーマット方針

Excelの表示形式（Number Format）は多機能なため、Markdown変換における可読性向上という
本質的価値に焦点を当て、サポート範囲を以下のように優先度分けする。

| 優先度 | 対象 | 方針 |
|---|---|---|
| 必須（v1スコープ） | 桁区切り、小数点以下桁数、パーセンテージ、基本的な日付/時刻（`yyyy/mm/dd hh:mm`） | これらが欠けると生データ（シリアル値等）がそのまま出力され可読性が著しく低下するため必須 |
| v1対象外 | 和暦、通貨記号、テキスト埋め込み等のロケール依存書式 | 下記の理由によりv1では自前実装の対象から外す |
| 対象外 | 文字色指定、条件付き表示、文字埋め等の視覚的装飾 | Markdownのプレーンテキストで再現できないためストリップ（無視）する |

**`ssfmt`クレート（ECMA-376準拠のExcel数値書式パーサ／フォーマッタ）はv1では不採用とし、
自前実装（`Cell::display_text`内の`format_number`/`format_date`）を維持する。**
[Issue #2でのスパイク検証](https://github.com/MinamiyamaKotaro/extmd/issues/2#issuecomment-5304633010)
（crates.io API・GitHubソースの直接調査）の結果は以下の通り。

- 必須スコープ（桁区切り・小数点桁数・パーセンテージ・基本日付/時刻）は`ssfmt`側でも
  サポートされていることを確認した。通貨記号（`[$currency-lcid]`形式の汎用ロケール
  ブラケット解析）・テキスト埋め込み（引用文字列リテラル、ECMA-376の基本機能）も
  ソースコード上サポートを確認した。一方、和暦（`ggge"年"`等）はソース全体を検索しても
  該当ロジックが存在せず、非対応と判明した（ヒジュラ暦は独自実装されているにもかかわらず）。
- 依存クレート数は実測で2つ（`lru`/`thiserror`が必須、`chrono`はデフォルトfeatureだが
  optional）であり、Issue #1時点の「約20」という見積もりは誤りだったと判明した。
- `NumberFormat::parse()`は`Result<_, ParseError>`を返す設計でpanicしない。
- `calamine`とのインターフェース整合性という論点は陳腐化した。実際のreader実装では
  `calamine`ではなく`umya-spreadsheet`を採用しており（[reader/xlsx.md](../reader/xlsx.md)）、
  `style.numbering_format().format_code()`が返す`String`をそのまま`ssfmt::parse(&str)`に
  渡せるため、こちらも互換性上の問題はない。
- 一方で保守状況には無視できないリスクがあった: 作成2026-01-09、最終コミット2026-01-22で
  以降約7ヶ月間コミットがなく、star数6・fork数1・pre-1.0（v0.1.2）。コミット履歴上、
  106コミット中72件がAI（Claude）支援によるものであり、短期集中開発の後に更新が
  止まっている状態と判断した。

必須スコープ自体は現行の自前実装が既にテスト込みで満たしており、和暦・通貨記号は元々
v1スコープ外だったため、`ssfmt`採用によってv1の価値が直接増えるわけではない。上記の
保守リスクと天秤にかけ、**v1では外部依存を追加しない**という判断とした。v2で和暦・
通貨記号対応が必要になった時点で、`ssfmt`のメンテナンス状況を再評価する形で先送りする。

## 5. 未確定事項

- （4章の`ssfmt`採用可否は[Issue #2](https://github.com/MinamiyamaKotaro/extmd/issues/2)で決着済み）
