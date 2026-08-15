# `renderer::output` 設計書

対象: [renderer/mod.md](mod.md)の対応表における `output.rs`。

## 1. 責務

`mod.rs`から`OutputTarget`と、レンダリング済みのMarkdown本文を受け取り、標準出力または
ファイルシステムへ書き出す。`SplitDirectory`時のファイル名生成（サニタイズ・衝突回避）も
このファイルの責務とする。CLIフラグの意味論は一切持たない（[mod.md 3章](mod.md#3-設計方針)）。

## 2. 書き込みAPI

```rust
pub(crate) fn write_stdout(body: &str) -> Result<(), RendererError> {
    use std::io::Write;
    write!(std::io::stdout(), "{body}").map_err(RendererError::Io)
}

pub(crate) fn write_single_file(path: &std::path::Path, body: &str) -> Result<(), RendererError> {
    std::fs::write(path, body).map_err(RendererError::Io) // 5章: 同名なら上書き
}

pub(crate) fn write_split(
    dir: &std::path::Path,
    sheets: Vec<(String, String)>, // (sheet_name, body)
) -> Result<(), RendererError> {
    std::fs::create_dir_all(dir).map_err(RendererError::Io)?;

    let mut used_names = std::collections::HashSet::new();
    for (i, (sheet_name, body)) in sheets.into_iter().enumerate() {
        let base = sanitize_base_name(&sheet_name, i);
        let unique = resolve_unique_filename(&base, &mut used_names);
        let path = dir.join(format!("{unique}.md"));
        std::fs::write(&path, body).map_err(RendererError::Io)?; // 5章: 同名なら上書き
    }
    Ok(())
}
```

## 3. ファイル名サニタイズ（`SplitDirectory`時）

シート名は[要件定義書6章](../../requirement/requirements.md#6-非機能要件)の
「動作環境: macOS / Linux / Windows」というクロスプラットフォーム要件を満たすファイル名に
変換する必要がある。Excel自身のシート名バリデーション（`\ / ? * [ ] :`の禁止・31文字制限）は
Windowsの禁止文字・予約デバイス名のすべてをカバーしないため、`output.rs`側で以下の変換を行う
（[Issue #8での議論](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301935435)〜
[決定](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301939573)）。

```rust
const WINDOWS_FORBIDDEN_CHARS: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn sanitize_base_name(sheet_name: &str, index: usize) -> String {
    let replaced: String = sheet_name
        .chars()
        .map(|c| if WINDOWS_FORBIDDEN_CHARS.contains(&c) { '_' } else { c })
        .collect();
    let trimmed = replaced.trim_end_matches(['.', ' ']);

    if trimmed.is_empty() {
        // 3.1: 末尾ピリオド・空白のみで構成されるシート名（Excelでは禁止されていない）は
        // トリム後に空文字列になりうる。フォールバックとしてシート位置を使う
        format!("sheet_{}", index + 1)
    } else if WINDOWS_RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(trimmed)) {
        format!("{trimmed}_sheet")
    } else {
        trimmed.to_string()
    }
}
```

- **禁止文字の置換**: `\ / : * ? " < > |` を`_`に置換する。このうち`\ / : * ?`はExcel自身も
  禁止しているため実際には到達しないが、`"` `<` `>` `|`はExcelでは許可されている。
- **末尾のピリオド・空白のトリム**: Windowsでファイル名末尾に置けない文字を削除する。
- **予約デバイス名の回避**: 大小文字を区別せず`CON`/`NUL`/`COM1`〜`9`/`LPT1`〜`9`等に完全一致
  する場合、`_sheet`サフィックスを付与する（例: `CON` → `con_sheet`）。

### 3.1 空文字列へのフォールバック

トリムの結果、ベース名が空文字列になるケース（シート名が`"..."`のようにトリム対象の文字のみで
構成される場合）があるため、シートのインデックス（0始まり、`i`）を使った`sheet_{i + 1}`を
デフォルトのベース名として採用する（[Issue #8での指摘](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301935435)）。

## 4. 同一実行内でのファイル名衝突回避

3章のサニタイズ処理は、Excel上は別名だったシート名同士（例: `Sheet"A"`と`Sheet<A>`はいずれも
`Sheet_A_`に収束する）を同じファイル名に収束させうる。これを検知し、一意になるまで連番
サフィックスを付与する（[Issue #8での決定](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301939573)）。

```rust
fn resolve_unique_filename(base: &str, used_names: &mut std::collections::HashSet<String>) -> String {
    let mut candidate = base.to_string();
    let mut suffix = 2;
    // ファイルシステムによっては大文字小文字を区別しない（Windows/macOS既定）ため、
    // 比較は小文字に統一して行う
    while !used_names.insert(candidate.to_lowercase()) {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
    candidate
}
```

`used_names`は`write_split`の呼び出しごとに新規に生成する（当該実行内でのみ有効なスコープ。
実行をまたいだ一意性については5章を参照）。

## 5. 既存ファイル・残骸ファイルの扱い（Option C）

**デフォルトの挙動として、同名ファイルが既に存在する場合は警告なしに上書きし、それ以外の
既存ファイル（無関係なファイル、削除・リネームされたシートの残骸ファイル）には一切手を出さない**
（[Issue #8での決定](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301948782)、
[再確認](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301970163)）。

検討した他の選択肢と不採用の理由:

- **書き込み前に出力先ディレクトリを自動クリーンアップする案**: ユーザーが誤って重要な
  ディレクトリ（例: `-o docs/`）を指定した場合に、無関係な既存ファイルを一括削除してしまう
  致命的なリスクがあるため不採用。
- **出力先が非空なら警告・エラーにする案**: `xlsx`の更新のたびに同じディレクトリへ
  再変換するという日常的な再実行の利便性を著しく損なうため不採用。
- **ファイル名に生成日時タイムスタンプを付与し上書きを回避する案**: 一時検討したが
  （[提案](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301957538)）、
  要件定義書5.4節の「指定パスへ書き出す」という要件と矛盾する・スナップショットテストと
  相性が悪い・再実行のたびにファイルが際限なく増え続けるといった問題が明らかになり
  ([指摘](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301966199))、不採用とした。
  同様の「実行履歴を残したい」ニーズには、6章の通りオプトインのCLIフラグで対応する。

削除された・リネームされたシートに対応する残骸ファイルは自動では削除しない。クリーンアップが
必要な場合は、ユーザー側で出力先ディレクトリを事前にクリアする運用を想定する。

## 6. CLIとの境界

[analysis/registry.md 5章「CLIとの境界」](../analysis/registry.md#5-cliとの境界)と同じ方針で、
`OutputTarget`をどう構築するか（`--split`の判定、`-o`の値の解釈、実行履歴保存のための
オプトインフラグ等）は`renderer`層の外（`cli.rs`/`lib.rs`）の責務とする。

将来、5章で不採用としたタイムスタンプ付与を「オプトインのCLIフラグ」（フラグ名は`cli.rs`の
設計フェーズで確定）として提供する場合も、タイムスタンプの埋め込みは`cli.rs`側で完結させ、
`OutputTarget::SingleFile(path)`/`SplitDirectory(dir)`に渡す時点で**既に確定したパス**にしておく。
`output.rs`はそのパスがどう決まったかを一切意識せず、3〜4章の衝突検知・書き込みにのみ専念する
（[Issue #8での決定](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301974034)、
[合意](https://github.com/MinamiyamaKotaro/extmd/issues/8#issuecomment-5301979135)）。

## 7. 未確定事項

- 3章の禁止文字・予約名リストの妥当性は実データでの検証が必要
- `-v`/`--verbose`指定時に、上書き発生をログ出力するかどうか（[reader/validation.md 3章](../reader/validation.md#3-破棄無視の方針とログ出力)の
  ロギング方針決定と合わせて`cli.rs`側で検討する）
- 出力先クリーンアップオプション（例: `--clean`）の提供要否は未確定
  （[アーキテクチャ設計書8章](../architecture.md#8-未確定事項要件定義書-8章との対応)に
  オープンクエスチョンとして追記済み）
