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

### 4.1 依存ライブラリのセキュリティ検証と監査方針

`.xlsx`はZIP+XML形式であり、入力ファイルの生成者と実行者が異なりうる
（[要件定義書2章](../../requirement/requirements.md#2-背景課題)の通り、社外から受け取った
申請書等を変換する用途を主要ユースケースとして含む）ため、悪意あるファイルへの耐性を
`umya-spreadsheet`の実依存ライブラリに基づいて評価する
（[docs/security/design-review.md #5](../../security/design-review.md#5-xmlzipパーサ依存によるサプライチェーンリスク)、
[Issue #14](https://github.com/MinamiyamaKotaro/extmd/issues/14)での検討を反映）。

**XML外部エンティティ解決（XXE）の排除**: `umya-spreadsheet`がXMLパースに採用している
`quick-xml`クレートは、入力ストリームからXMLトークンを順次切り出すPull型のパーサである。
本パーサはDTD宣言を単なる生テキストイベント（`quick_xml::events::Event::DocType`）として
呼び出し元に返すのみであり、`&entity;`のようなエンティティ参照も独立したイベント
（`quick_xml::events::Event::GeneralRef`）としてそのまま渡すだけで、一般エンティティ・
外部エンティティを解釈・解決して外部リソースへネットワーク接続やファイルアクセスを行う
機能（エンティティリゾルバ）を実装していない。`umya-spreadsheet`側もこれらのイベントを
特別扱いして外部解決するコードを持たない。**これは`quick-xml`の設計から導かれる推論であり、
公式ドキュメントにXXE非対応を明言する記述があるわけではない**。したがって、パース処理を
介したXXEインジェクションのリスクは構造的に排除されていると判定できる。

**Zip Bombに対する残存リスクと暫定対策**: `umya-spreadsheet`による各シートの読み込みは、
`BufReader`を介した`quick-xml`のイベント駆動ストリーミングパース（`read_event_into`を
用いた逐次解析）として実装されており、ZIP展開エントリの生テキスト全体を一括バッファリング
する処理は行わない。しかし、パース処理の進行に伴い、抽出されたセルや行のデータ構造
（`Cell`/`Row`等）はメモリ上に逐次アキュムレートされていく。そのため、圧縮率は非常に
高いが大量のセル要素を含む悪意あるXML（Zip Bomb）を入力した場合、パース完了後の
`max_cells`（5章参照）の検証に到達する前の**パース実行中の段階で結果構造体のメモリ累積により
OOM（プロセスクラッシュ/DoS）を引き起こすことが可能**である。

この制約を受け、v1では以下の**多層防御**と**残存リスクの受容**を設計前提とする。

1. **入力ファイルサイズ制限**: CLI入口でのファイルサイズチェック（例: 100MB以下）により、
   巨大な圧縮ファイルによる単純なリソース消費を防ぐ（[cli.md](../cli.md)）。
2. **最大セル数検証**: パース自体が成功した後の疎な巨大座標指定（座標操作による
   `domain::Cell`メモリ大量確保）は5章の`max_cells`ガードで防ぐ。
3. **残存リスクの明示**: 展開・パース段階での結果構造体累積によるリソース枯渇は
   上記2つの対策では防げないため、extmdをサーバーサイド等のマルチテナント環境
   （不特定多数のユーザーが任意の`.xlsx`をアップロードする環境）で動かすことはv1の
   安全設計上は非推奨とし、ローカルCLIとして利用されることを想定する
   （[README](../../../README.md)・[cli.md](../cli.md)の「利用上の注意」に明記する）。
4. **将来の改善策**: マルチテナント環境への対応等でZip Bombの完全排除が必要となる場合は、
   ZIP展開処理をextmd側でハンドリングして`Read::take`等で展開サイズを制限しつつ、
   `quick-xml`を用いた自前の低メモリ型（ストリーミングベース）パースへの移行を検討する。

**継続的な依存クレート監査**: 上記はレビュー時点（[Issue #14](https://github.com/MinamiyamaKotaro/extmd/issues/14)）の
`umya-spreadsheet`の依存構成（`quick-xml`/`zip`クレート）に基づく評価であり、依存バージョンが
変わると前提が崩れうる。CI/CDへの`cargo audit`（既知CVEの継続監視）・`cargo deny`
（ライセンス/アドバイザリチェック）の導入を将来のCI設計で検討する（6章）。

## 5. `ReaderError` と公開API

```rust
#[derive(Debug)]
pub enum ReaderError {
    /// ファイルが存在しない、権限がない等のI/Oエラー。
    Io(std::io::Error),
    /// xlsxとして不正な形式・破損したファイル（umya-spreadsheetのパースエラーをラップ）。
    Parse(String),
    /// シートの `rows * cols` が `max_cells` を超過した（4.1節参照）。
    /// 悪意ある/破損したファイルが座標だけを巨大な値に細工しているケースを含め、
    /// 疎な巨大シートによるメモリ枯渇 (DoS) を未然に防ぐための拒否。
    SheetTooLarge { name: String, rows: usize, cols: usize, limit: usize },
}

impl std::fmt::Display for ReaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReaderError::Io(e) => write!(f, "{}", e),
            ReaderError::Parse(msg) => write!(f, "{}", msg),
            ReaderError::SheetTooLarge { name, rows, cols, limit } => write!(
                f,
                "Sheet '{name}' has {rows} x {cols} cells, which exceeds the limit of {limit} cells"
            ),
        }
    }
}

impl std::error::Error for ReaderError {}

/// 指定した `.xlsx` ファイルの全シートを読み込み、`domain::Sheet` の列へ変換する。
/// シートの絞り込み（要件定義書 5.1 `-s`/`--sheet`）は呼び出し側（`lib.rs`）の責務とし、
/// `reader` は常に全シートを返す（umya-spreadsheetはEagerパースのため、
/// 読み込み時点でのフィルタリングによる性能上の利点がないため）。
///
/// `max_cells` は1シートあたりの `rows * cols` の上限（CLIの`--max-cells`、
/// [cli.md](../cli.md)参照）。超過したシートが1つでもあれば `ReaderError::SheetTooLarge`
/// を返し、処理全体を打ち切る（4章「シート単位のエラー伝播」と同じ「部分成功はv1では
/// 扱わない」方針に従う）。
pub fn read_sheets(path: &std::path::Path, max_cells: usize) -> Result<Vec<domain::Sheet>, ReaderError> {
    xlsx::read_sheets(path, max_cells)
}
```

`read_sheets` は `xlsx.rs` の実装へ薄く委譲するだけとし、`mod.rs` 自体はロジックを持たない（モジュールの公開エントリポイントである `mod.rs` は処理の委譲や横断的関心事のみを扱い、ロジックを中に持たないという設計方針を適用）。

## 6. 未確定事項

- 存在しない/破損している/非対応形式のファイルに対するエラーメッセージの具体的な文面
  （[要件定義書 5.2](../../requirement/requirements.md#52-入力)「原因が特定しやすいメッセージ」との整合は
  実装フェーズで詰める）
- `ReaderError::Parse` が保持する文字列の情報量（umya-spreadsheet側のエラー型をどこまで
  透過的に保持するか）
- `max_cells`（4.1節・5章）のデフォルト値の妥当性（実データでの検証が必要、[cli.md](../cli.md)参照）
- `cargo audit`/`cargo deny`のCI導入方針（4.1節、CI設計書が存在しないため別途起票が必要）
