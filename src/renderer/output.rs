//! `OutputTarget`に基づく書き込み、ファイル名サニタイズ・衝突検知
//! （docs/design/renderer/output.md）。CLIフラグの意味論は一切持たない。

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

use super::RendererError;

const WINDOWS_FORBIDDEN_CHARS: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub(in crate::renderer) fn write_stdout(body: &str) -> Result<(), RendererError> {
    write!(std::io::stdout(), "{body}").map_err(RendererError::Io)
}

/// 同名ファイルが既に存在する場合は警告なしに上書きする（5章）。
pub(in crate::renderer) fn write_single_file(path: &Path, body: &str) -> Result<(), RendererError> {
    std::fs::write(path, body).map_err(RendererError::Io)
}

pub(in crate::renderer) fn write_split(
    dir: &Path,
    sheets: Vec<(String, String)>,
) -> Result<(), RendererError> {
    std::fs::create_dir_all(dir).map_err(RendererError::Io)?;

    let mut used_names = HashSet::new();
    for (i, (sheet_name, body)) in sheets.into_iter().enumerate() {
        let base = sanitize_base_name(&sheet_name, i);
        let unique = resolve_unique_filename(&base, &mut used_names);
        let path = dir.join(format!("{unique}.md"));
        std::fs::write(&path, body).map_err(RendererError::Io)?;
    }
    Ok(())
}

/// シート名をクロスプラットフォームで安全なファイル名(拡張子抜き)に変換する。
/// Excel自身のシート名バリデーションはWindowsの禁止文字・予約デバイス名の
/// すべてをカバーしないため、ここで追加の変換を行う。
fn sanitize_base_name(sheet_name: &str, index: usize) -> String {
    let replaced: String = sheet_name
        .chars()
        .map(|c| {
            if WINDOWS_FORBIDDEN_CHARS.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = replaced.trim_end_matches(['.', ' ']);

    if trimmed.is_empty() {
        // 末尾ピリオド・空白のみで構成されるシート名（Excelでは禁止されていない）は
        // トリム後に空文字列になりうるため、シート位置をフォールバックに使う。
        format!("sheet_{}", index + 1)
    } else if WINDOWS_RESERVED_NAMES
        .iter()
        .any(|r| r.eq_ignore_ascii_case(trimmed))
    {
        format!("{trimmed}_sheet")
    } else {
        trimmed.to_string()
    }
}

/// サニタイズ後に別シート名同士が同じファイル名へ収束するケースを検知し、
/// 一意になるまで連番サフィックスを付与する。比較はファイルシステムの大文字小文字
/// 非区別（Windows/macOS既定）を考慮し小文字に統一して行う。
fn resolve_unique_filename(base: &str, used_names: &mut HashSet<String>) -> String {
    let mut candidate = base.to_string();
    let mut suffix = 2;
    while !used_names.insert(candidate.to_lowercase()) {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "extmd-renderer-test-{name}-{nanos}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn sanitize_base_name_replaces_forbidden_chars() {
        assert_eq!(sanitize_base_name("Sheet\"A\"<B>|", 0), "Sheet_A__B__");
    }

    #[test]
    fn sanitize_base_name_trims_trailing_dot_and_space() {
        assert_eq!(sanitize_base_name("Report. ", 0), "Report");
    }

    #[test]
    fn sanitize_base_name_falls_back_to_index_when_empty_after_trim() {
        assert_eq!(sanitize_base_name("...", 2), "sheet_3");
    }

    #[test]
    fn sanitize_base_name_avoids_windows_reserved_names_case_insensitively() {
        assert_eq!(sanitize_base_name("con", 0), "con_sheet");
        assert_eq!(sanitize_base_name("NUL", 0), "NUL_sheet");
    }

    #[test]
    fn sanitize_base_name_keeps_normal_names_unchanged() {
        assert_eq!(sanitize_base_name("Sheet1", 0), "Sheet1");
    }

    #[test]
    fn resolve_unique_filename_appends_suffix_on_collision() {
        let mut used = HashSet::new();
        assert_eq!(resolve_unique_filename("a", &mut used), "a");
        assert_eq!(resolve_unique_filename("a", &mut used), "a_2");
        assert_eq!(resolve_unique_filename("a", &mut used), "a_3");
    }

    #[test]
    fn resolve_unique_filename_is_case_insensitive() {
        let mut used = HashSet::new();
        assert_eq!(resolve_unique_filename("Sheet", &mut used), "Sheet");
        // 比較は小文字化して行うため、大文字小文字違いの"sheet"も既出扱いになり
        // サフィックスが付く（連番自体は2回目の呼び出し時の`base`("sheet")を使う）。
        assert_eq!(resolve_unique_filename("sheet", &mut used), "sheet_2");
    }

    #[test]
    fn write_single_file_writes_body_to_path() {
        let dir = temp_dir("single-file");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.md");

        write_single_file(&path, "# hello").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# hello");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_split_writes_one_file_per_sheet_and_dedupes_collisions() {
        let dir = temp_dir("split");
        // 両方とも `sanitize_base_name` で "Sheet_A_" に収束する組み合わせ
        // （output.md 4章の例と同じ）。
        let sheets = vec![
            ("Sheet\"A\"".to_string(), "body-a".to_string()),
            ("Sheet<A>".to_string(), "body-b".to_string()),
        ];

        write_split(&dir, sheets).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.join("Sheet_A_.md")).unwrap(),
            "body-a"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("Sheet_A__2.md")).unwrap(),
            "body-b"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_single_file_overwrites_existing_file() {
        let dir = temp_dir("overwrite");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.md");

        write_single_file(&path, "first").unwrap();
        write_single_file(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
