# `renderer::flow` 設計書

対象: [renderer/mod.md](mod.md)の対応表における `flow.rs`。

## 1. 責務

`RowKind::Flow`（[domain/document.md 1章](../domain/document.md#1-rowkind)）と分類された
1行分の`RenderedRow`を、段落テキストまたは見出しのMarkdownへ変換する。
出力先モードによって変わる見出しレベルのオフセットは`mod.rs`から引数で受け取り、
`flow.rs`自身は`OutputTarget`を知らない（[mod.md 5章](mod.md#5-シート見出しレベルと本文見出しレベルの階層関係heading_offset)）。

## 2. 変換アルゴリズム

```rust
pub(crate) fn render_row(row: &domain::RenderedRow, heading_offset: u8) -> String {
    row.blocks
        .iter()
        .map(|block| render_block(block, heading_offset))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_block(block: &domain::Block, heading_offset: u8) -> String {
    let text = escape::escape_flow_text(&block.text);

    match block.heading_level {
        Some(level) => {
            // domain/block.md 2章の契約により heading_level は常に 1..=6。
            // 契約違反はレンダラー側でクランプ等のフォールバックを行わず、
            // デバッグビルドで早期検出する（3章）。
            debug_assert!((1..=6).contains(&level), "heading_level must be 1..=6");
            let level = (level + heading_offset).min(6);
            format!("{} {text}", "#".repeat(level as usize))
        }
        None => text,
    }
}
```

`RenderedRow.blocks`は`RowKind::Flow`であっても複数の`Block`を持ちうる
（[grid_paper.md 4章](../analysis/strategies/grid_paper.md#4-classify_row)の
`classify_row`は最大3ブロックまでFlowと判定する）。各`Block`は独立したフォント・
見出しレベルを持ちうるため、1行にまとめず**`Block`ごとに1行として出力する**
（複数ブロックを1行に連結すると、先頭以外のブロックの`heading_level`を
表現できなくなるため）。

## 3. `heading_level`の範囲外値をレンダラーで扱わない理由

`domain::Block.heading_level`は`Option<u8>`型で、`Some`の場合は常に`1〜6`
（[domain/block.md 2章](../domain/block.md#2-block)の型コメント）であることが契約として
確定しており、この契約を満たすのは`AnalysisStrategy::heading_level`の実装
（[grid_paper.md 5章](../analysis/strategies/grid_paper.md#5-heading_level)/
[tabular.md](../analysis/strategies/tabular.md)）のみである。

`renderer`側で`Some(0)`や`Some(n >= 7)`のような契約外の値をクランプ・フォールバックする
防御コードは持たない。これは`analysis`層が採用した「契約は型・可視性・docstringで担保し、
ランタイムの防御コードで二重に担保しない」という方針
（[analysis/metrics.md 4章](../analysis/metrics.md#4-可視性の設計-pub--pubin-crateanalysis)）と
一貫させるための判断であり、[Issue #8での議論](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301923152)を経て、
最終的に[この方針](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301926596)で確定した。

## 4. `heading_offset`加算後のクランプ（`.min(6)`）について

`level + heading_offset`はMarkdownの見出し上限（`######`、レベル6）を超えうる一般式のため
`.min(6)`でクランプする。ただし、v1でスコープに入る2戦略
（[analysis/strategies/mod.md 2章](../analysis/strategies/mod.md#2-v1スコープ-grid-paper--tabular-の2戦略のみ)）の
`heading_level`実装では、`GridPaperStrategy`が返す最大値は`Some(4)`、`TabularStrategy`は常に
`None`であるため、`heading_offset`（最大`1`）を加算しても最大`5`にしかならず、
このクランプはv1では実質発火しない
（[Issue #8での指摘](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301923152)と
[反映](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301926596)）。
将来、より高い`heading_level`を返す戦略が追加された場合に備えた一般式として残す。

## 5. 未確定事項

- 複数`Block`を持つFlow行における、見出しでないブロック同士の区切り（現在は改行のみ）が
  実データで読みやすいか、実装後にスナップショットテストで確認する
