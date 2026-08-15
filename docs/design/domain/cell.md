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
}

impl Cell {
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}
```

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
| 検証待ち（未確定） | 和暦、通貨記号、テキスト埋め込み等のロケール依存書式 | 下記の理由により実装フェーズでの検証待ちとする |
| 対象外 | 文字色指定、条件付き表示、文字埋め等の視覚的装飾 | Markdownのプレーンテキストで再現できないためストリップ（無視）する |

**実装候補: `ssfmt`クレート**（ECMA-376準拠のExcel数値書式パーサ／フォーマッタ）。
自前でExcelの書式コード（セクション分岐等を含む）をパースするより工数・バグリスクが
小さいため候補とする。ただし以下の理由から、実装フェーズでの検証（スパイク）を経てから
正式採用を決定する。

- v0.1.2・pre-1.0・依存クレート数約20と実績が浅く、API安定性・保守継続性にリスクがある
- 和暦（`ggge"年"`）等ロケール依存書式のサポート状況がドキュメント上確認できておらず、
  「必須」項目（桁区切り・パーセンテージ・基本日付）以外の対応可否は未検証

（[Issue #1のレビューコメント](https://github.com/MinamiyamaKotaro/extmd/issues/1)での検証結果を反映）

## 5. 未確定事項

- `ssfmt`クレートの採用可否（4章参照）。実装フェーズでのスパイク検証が必要
- 和暦・通貨記号等ロケール依存書式のサポート範囲（同上、`ssfmt`の対応状況次第）
