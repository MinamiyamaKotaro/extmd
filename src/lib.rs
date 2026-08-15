//! 変換処理のエントリポイント: `reader` → `analysis` → `renderer` パイプラインの結合実行
//! (docs/design/cli.md 6.1節)。

pub mod analysis;
pub mod cli;
pub mod domain;
pub mod reader;
pub mod renderer;

use std::path::PathBuf;

/// 入力ファイルの物理サイズの上限(100MB)。ZIP展開・XMLパース自体が完了する前に
/// 単純に巨大なファイルを弾くための粗いフィルタ(`max_cells`とは独立した対策)。
const MAX_INPUT_FILE_SIZE_BYTES: u64 = 100 * 1024 * 1024;

/// 変換処理全体を制御する設定オブジェクト。CLI引数に依存しない純粋なデータ型として
/// 定義し、単体テストを可能にする。
pub struct ConvertConfig {
    pub input_path: PathBuf,
    pub sheet_names: Vec<String>,
    pub strategy_id: String,
    pub strategy_config: analysis::StrategyConfig,
    pub output_target: renderer::OutputTarget,
    /// `--split`時、出力先ディレクトリ内の既存`.md`ファイルを書き込み前に削除するか
    /// (`--clean`)。`convert`が入力の妥当性確認をすべて成功させた後、書き込み直前に
    /// 実行する(`build_config`実行時点で削除すると、後続の`convert`が失敗した場合でも
    /// 出力先の既存ファイルが消えてしまうため)。
    pub clean: bool,
    /// 1シートあたりに許容する最大セル数(`--max-cells`)。
    pub max_cells: usize,
}

#[derive(Debug)]
pub enum ConvertError {
    InputFileNotFound(PathBuf),
    InputFileTooLarge {
        path: PathBuf,
        size: u64,
        limit: u64,
    },
    Reader(reader::ReaderError),
    Renderer(renderer::RendererError),
    InvalidStrategy(String),
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::InputFileNotFound(p) => {
                write!(f, "Error: Input file not found: {}", p.display())
            }
            ConvertError::InputFileTooLarge { path, size, limit } => write!(
                f,
                "Error: Input file too large: {} ({size} bytes, limit: {limit} bytes)",
                path.display()
            ),
            ConvertError::Reader(e) => write!(f, "Error: Failed to read Excel file: {e}"),
            ConvertError::Renderer(e) => write!(f, "Error: Failed to write Markdown output: {e}"),
            ConvertError::InvalidStrategy(s) => {
                write!(f, "Error: Invalid strategy specified: {s}")
            }
        }
    }
}

impl std::error::Error for ConvertError {}

/// Excelファイルを読み込み、戦略に沿って解析し、Markdownとして書き出す一連の
/// パイプラインを実行する。
pub fn convert(config: ConvertConfig) -> Result<(), ConvertError> {
    // 1. 入力ファイルの存在チェック
    let metadata = std::fs::metadata(&config.input_path)
        .map_err(|_| ConvertError::InputFileNotFound(config.input_path.clone()))?;

    // 1.1. 入力ファイルサイズの上限チェック
    if metadata.len() > MAX_INPUT_FILE_SIZE_BYTES {
        return Err(ConvertError::InputFileTooLarge {
            path: config.input_path,
            size: metadata.len(),
            limit: MAX_INPUT_FILE_SIZE_BYTES,
        });
    }

    // 2. Reader: xlsxの読み込み(max_cellsによるシートサイズ上限チェックを含む)
    let all_sheets =
        reader::read_sheets(&config.input_path, config.max_cells).map_err(ConvertError::Reader)?;

    // 3. 変換対象シートのフィルタリング
    let target_sheets = if config.sheet_names.is_empty() {
        all_sheets
    } else {
        // 指定されたシート名がブック内に存在しない場合、タイポ等に気付けるよう警告する
        // (デフォルトのログレベルはWARNのため、--verbose指定なしでもユーザーに届く)。
        for name in &config.sheet_names {
            if !all_sheets.iter().any(|s| &s.name == name) {
                log::warn!("Sheet '{name}' not found in the workbook");
            }
        }
        all_sheets
            .into_iter()
            .filter(|s| config.sheet_names.contains(&s.name))
            .collect()
    };

    // 4. StrategyRegistry の初期化
    let registry = analysis::StrategyRegistry::with_config(config.strategy_config);

    // 5. 各シートの変換処理 (Analyzer)
    let mut documents = Vec::new();
    for sheet in target_sheets {
        let strategy = if config.strategy_id == "auto" {
            registry.select_auto(&sheet)
        } else {
            registry
                .get(&config.strategy_id)
                .ok_or_else(|| ConvertError::InvalidStrategy(config.strategy_id.clone()))?
        };

        log::info!(
            "Applied strategy '{}' to sheet '{}'",
            strategy.id(),
            sheet.name
        );

        let doc = analysis::analyze(&sheet, strategy);
        documents.push(doc);
    }

    // 6. --clean: 入力の妥当性確認(1〜5)がすべて成功した後、書き込み直前に実行する。
    if config.clean {
        if let renderer::OutputTarget::SplitDirectory(ref dir) = config.output_target {
            clean_split_directory(dir);
        }
    }

    // 7. Renderer: Markdownへのレンダリング & 書き出し
    renderer::render(&documents, config.output_target).map_err(ConvertError::Renderer)?;

    Ok(())
}

/// `--split`の出力先ディレクトリ直下にある拡張子`.md`のファイルのみを削除する
/// (ディレクトリ全体の`remove_dir_all`は、誤って重要なフォルダを指定した場合の
/// 全削除リスクを防ぐため行わない)。
fn clean_split_directory(dir: &std::path::Path) {
    if !dir.exists() || !dir.is_dir() {
        return;
    }
    log::info!(
        "Cleaning up markdown files in output directory: {}",
        dir.display()
    );
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                // 削除失敗（権限不足等）を握りつぶさず警告する。残骸ファイルが
                // 気付かれないまま残ることを防ぐため。
                if let Err(err) = std::fs::remove_file(&path) {
                    log::warn!("Failed to remove stale file {}: {}", path.display(), err);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_xlsx_path(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "extmd-lib-test-{name}-{nanos}-{}.xlsx",
            std::process::id()
        ))
    }

    /// `umya_spreadsheet::new_file()`は既定で"Sheet1"という名前のシートを1つ持つ。
    fn write_minimal_workbook(path: &std::path::Path, value: &str) {
        let mut book = umya_spreadsheet::new_file();
        book.sheet_by_name_mut("Sheet1")
            .unwrap()
            .cell_mut("A1")
            .set_value(value);
        umya_spreadsheet::writer::xlsx::write(&book, path).unwrap();
    }

    fn default_config(input_path: PathBuf, output_target: renderer::OutputTarget) -> ConvertConfig {
        ConvertConfig {
            input_path,
            sheet_names: vec![],
            strategy_id: "auto".to_string(),
            strategy_config: analysis::StrategyConfig::default(),
            output_target,
            clean: false,
            max_cells: 1_000_000,
        }
    }

    #[test]
    fn convert_returns_input_file_not_found_for_missing_path() {
        let config = default_config(
            PathBuf::from("/nonexistent/does-not-exist.xlsx"),
            renderer::OutputTarget::Stdout,
        );
        let err = convert(config).unwrap_err();
        assert!(matches!(err, ConvertError::InputFileNotFound(_)));
    }

    #[test]
    fn convert_returns_invalid_strategy_for_unknown_strategy_id() {
        let path = temp_xlsx_path("invalid-strategy");
        write_minimal_workbook(&path, "hello");

        let mut config = default_config(path.clone(), renderer::OutputTarget::Stdout);
        config.strategy_id = "nonexistent".to_string();

        let err = convert(config).unwrap_err();
        assert!(matches!(err, ConvertError::InvalidStrategy(s) if s == "nonexistent"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn convert_writes_markdown_for_a_minimal_workbook() {
        let input = temp_xlsx_path("round-trip");
        write_minimal_workbook(&input, "hello");
        let output = input.with_extension("md");

        let config = default_config(
            input.clone(),
            renderer::OutputTarget::SingleFile(output.clone()),
        );
        convert(config).unwrap();

        let body = std::fs::read_to_string(&output).unwrap();
        assert!(body.contains("Sheet1"));
        assert!(body.contains("hello"));

        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
    }

    #[test]
    fn convert_filters_sheets_by_sheet_names() {
        let input = temp_xlsx_path("filter");
        write_minimal_workbook(&input, "hello");
        let output = input.with_extension("md");

        let mut config = default_config(
            input.clone(),
            renderer::OutputTarget::SingleFile(output.clone()),
        );
        config.sheet_names = vec!["NoSuchSheet".to_string()];
        convert(config).unwrap();

        let body = std::fs::read_to_string(&output).unwrap();
        assert!(!body.contains("Sheet1"));

        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
    }

    fn temp_dir_path(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "extmd-lib-test-dir-{name}-{nanos}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn convert_clean_removes_stale_md_files_on_success() {
        let input = temp_xlsx_path("clean-success");
        write_minimal_workbook(&input, "hello");
        let dir = temp_dir_path("clean-success");
        std::fs::create_dir_all(&dir).unwrap();
        let stale = dir.join("stale.md");
        std::fs::write(&stale, "old content").unwrap();

        let mut config = default_config(
            input.clone(),
            renderer::OutputTarget::SplitDirectory(dir.clone()),
        );
        config.clean = true;
        convert(config).unwrap();

        assert!(!stale.exists());
        assert!(dir.join("Sheet1.md").exists());

        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// レビュー指摘の再発防止: `--clean`は`convert`が入力の妥当性確認をすべて成功させた
    /// 後にのみ実行されるべきで、`InputFileNotFound`のような早期エラーの前に出力先を
    /// クリーンアップしてはならない（既存の出力ファイルが変換失敗時に消失するため）。
    #[test]
    fn convert_clean_does_not_delete_existing_files_when_input_is_missing() {
        let dir = temp_dir_path("clean-failure");
        std::fs::create_dir_all(&dir).unwrap();
        let existing = dir.join("existing.md");
        std::fs::write(&existing, "must survive").unwrap();

        let mut config = default_config(
            PathBuf::from("/nonexistent/does-not-exist.xlsx"),
            renderer::OutputTarget::SplitDirectory(dir.clone()),
        );
        config.clean = true;

        let err = convert(config).unwrap_err();
        assert!(matches!(err, ConvertError::InputFileNotFound(_)));
        assert!(existing.exists());
        assert_eq!(std::fs::read_to_string(&existing).unwrap(), "must survive");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn convert_warns_but_succeeds_when_requested_sheet_name_is_missing() {
        let input = temp_xlsx_path("missing-sheet-warning");
        write_minimal_workbook(&input, "hello");
        let output = input.with_extension("md");

        let mut config = default_config(
            input.clone(),
            renderer::OutputTarget::SingleFile(output.clone()),
        );
        config.sheet_names = vec!["Sheet1".to_string(), "Typo".to_string()];
        // タイポしたシート名が含まれていても、実在するシートは変換され正常終了する
        // （警告ログが出るだけでエラーにはしない）。
        convert(config).unwrap();

        let body = std::fs::read_to_string(&output).unwrap();
        assert!(body.contains("hello"));

        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
    }
}
