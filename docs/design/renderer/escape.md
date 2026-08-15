# `renderer::escape` 設計書

対象: [renderer/mod.md](mod.md)の対応表における `escape.rs`。

## 1. 責務

セル本文中の、Markdown記法と衝突しうる文字（改行・パイプ文字・強調記号等）を、
出力コンテキスト（テーブルセル内 / Flowテキスト内）ごとに定めたルールで変換する
純粋関数群。外部ライブラリ・状態を持たず、[flow.rs](flow.md)/[table.rs](table.md)から
呼ばれる（[Issue #8の最初のコメント](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301901971)）。

## 2. `escape_table_cell`

```rust
pub(crate) fn escape_table_cell(text: &str) -> String {
    // 置換順序が重要: バックスラッシュを最初にエスケープしないと、
    // 後続の置換で挿入したバックスラッシュ自身を二重にエスケープしてしまう
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', "<br>")
}
```

- **改行 → `<br>`**: テーブルセル内での生の改行は行の終端とみなされ表が崩れるため、
  HTMLの`<br>`タグに置換する。
- **パイプ（`|`） → `\|`**: テーブルの列境界と誤認識されるため、エスケープする。
- **バックスラッシュ（`\`） → `\\`**: エスケープ文字自体として解釈されるのを防ぐ。

## 3. `escape_flow_text`

```rust
pub(crate) fn escape_flow_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        // バックスラッシュを含む制御文字は `\` を前置してエスケープする。
        // 順序上、バックスラッシュ自身のエスケープが常に他の文字より先に評価される
        // （1文字ずつ処理するため2章のような置換順序の問題は起きない）。
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '#') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped.replace('\n', "  \n")
}
```

- **Markdown制御文字（`\` `*` `_` `` ` `` `[` `]` `#`） → `\`でエスケープ**:
  セルの値がたまたま`*太字*`や`# 見出し`のようなMarkdown記法と衝突する文字列だった場合に、
  意図しない強調・見出し・リンク記法として解釈されるのを防ぐ。
- **改行 → 行末スペース2つ＋改行（`"  \n"`）**: Markdownの明示的な改行記法に変換する
  （生の改行1つだけでは同一段落として連結されてしまうため）。

1文字ずつ走査する実装のため、`escape_table_cell`のような複数回の`replace`呼び出しによる
置換順序の考慮は不要。

## 4. 未確定事項

- エスケープ対象の制御文字セット（3章の`\* _ \` [ ] #`）が過剰/不足でないかは、
  実データでのスナップショットテストを通じて調整する
