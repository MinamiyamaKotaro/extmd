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

    // 6. Renderer: Markdownへのレンダリング & 書き出し
    renderer::render(&documents, config.output_target).map_err(ConvertError::Renderer)?;

    Ok(())
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
}
