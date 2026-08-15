# `renderer::escape` 設計書

対象: [renderer/mod.md](mod.md)の対応表における `escape.rs`。

## 1. 責務

セル本文中の、Markdown記法と衝突しうる文字（改行・パイプ文字・強調記号等）を、
出力コンテキスト（テーブルセル内 / Flowテキスト内）ごとに定めたルールで変換する
純粋関数群。外部ライブラリ・状態を持たず、[flow.rs](flow.md)/[table.rs](table.md)から
呼ばれる（[Issue #8の最初のコメント](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301901971)）。

## 2. `escape_table_cell`

当初案は複数回の`.replace`呼び出しを順序依存で連鎖させる実装だったが、呼び出しのたびに
中間`String`がヒープ確保される・置換順序を将来のコード修正で誤って壊しやすいという
2つの課題が実装時のPRレビューで指摘され（[PR #22レビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/22#pullrequestreview-4944043192)）、
1文字ずつの単一パス走査＋`match`式に変更した（3章の`escape_flow_text`と同じ方式に統一）。

```rust
fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n")
}

pub(in crate::renderer) fn escape_table_cell(text: &str) -> String {
    let normalized = normalize_line_endings(text);
    let mut escaped = String::with_capacity(normalized.len());
    for c in normalized.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\\' => escaped.push_str("\\\\"),
            '|' => escaped.push_str("\\|"),
            '\n' => escaped.push_str("<br>"),
            _ => escaped.push(c),
        }
    }
    escaped
}
```

- **改行 → `<br>`**: テーブルセル内での生の改行は行の終端とみなされ表が崩れるため、
  HTMLの`<br>`タグに置換する。変換前に`normalize_line_endings`でCRLF（`\r\n`）をLF（`\n`）に
  正規化する。Windows環境や一部のライブラリで作成された`.xlsx`はセル内改行を`\r\n`で
  格納している場合があり、これを考慮しないと変換後に`\r`が残留してしまう
  （[PR #22レビューコメント](https://github.com/MinamiyamaKotaro/extmd/pull/22#pullrequestreview-4944043192)での指摘を反映）。
- **パイプ（`|`） → `\|`**: テーブルの列境界と誤認識されるため、エスケープする。
- **バックスラッシュ（`\`） → `\\`**: エスケープ文字自体として解釈されるのを防ぐ。
- **`&` `<` `>` → HTMLエンティティ**: 4章参照。

1文字ずつの単一パス走査のため、旧実装で必要だった置換順序（`&`を`<`/`>`より先に、
`\`を`|`より先に、`\n`を最後に、という制約）は構造的に発生しない。

## 3. `escape_flow_text`

```rust
pub(in crate::renderer) fn escape_flow_text(text: &str) -> String {
    let normalized = normalize_line_endings(text);
    let mut escaped = String::with_capacity(normalized.len());
    for c in normalized.chars() {
        match c {
            '\\' | '*' | '_' | '`' | '[' | ']' | '#' | '&' | '<' | '>' => {
                escaped.push('\\');
                escaped.push(c);
            }
            '\n' => escaped.push_str("  \n"),
            _ => escaped.push(c),
        }
    }
    escaped
}
```

- **Markdown制御文字（`\` `*` `_` `` ` `` `[` `]` `#`） → `\`でエスケープ**:
  セルの値がたまたま`*太字*`や`# 見出し`のようなMarkdown記法と衝突する文字列だった場合に、
  意図しない強調・見出し・リンク記法として解釈されるのを防ぐ。
- **`&` `<` `>` → `\`でエスケープ**: 4章参照。CommonMarkの仕様上、バックスラッシュエスケープされた
  文字はMarkdown記法として再解釈されず、後続のHTML変換時にテキストノードとして
  エンティティ化されるため、`escape_table_cell`と同じ実体参照への置換ではなく
  既存の1文字ずつのバックスラッシュエスケープ方式に統一する。
- **改行 → 行末スペース2つ＋改行（`"  \n"`）**: Markdownの明示的な改行記法に変換する
  （生の改行1つだけでは同一段落として連結されてしまうため）。2章と同じ理由で
  `normalize_line_endings`によるCRLF正規化を先に行う。

## 4. `&` `<` `>` をエスケープする理由（生HTML混入によるインジェクション対策）

CommonMark/GFM等の多くのMarkdown実装は、埋め込まれた生HTMLタグをそのままHTMLとして
出力する（サニタイズするかどうかは処理系依存）。セル値に`<img src=x onerror=...>`や
`<script>`のような文字列が含まれる`.xlsx`を変換した場合、生成した`.md`を生HTMLを
サニタイズせずレンダリングするビューア（社内Wiki・静的サイトジェネレータ等）で
任意のHTML/JSが実行されうる（[docs/security/design-review.md #1](../../security/design-review.md#1-出力markdownへの生html混入によるストアド型インジェクション)、
[Issue #14](https://github.com/MinamiyamaKotaro/extmd/issues/14)）。

このリスクを構造的に塞ぐため、`escape_table_cell`/`escape_flow_text`のいずれも
`&` `<` `>` をエスケープ対象に追加する。

## 5. 未確定事項

- エスケープ対象の制御文字セット（3章の`\* _ \` [ ] # & < >`）が過剰/不足でないかは、
  実データでのスナップショットテストを通じて調整する
