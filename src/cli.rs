//! CLI引数の定義(`CliArgs`)と、ライブラリ設定型(`ConvertConfig`)へのマッピング
//! (docs/design/cli.md)。

use std::path::PathBuf;

use clap::Parser;

use crate::analysis::StrategyConfig;
use crate::renderer::OutputTarget;
use crate::ConvertConfig;

#[derive(Parser, Debug)]
#[command(
    name = "extmd",
    author = "MinamiyamaKotaro",
    version = env!("CARGO_PKG_VERSION"),
    about = "Excel (.xlsx) to Markdown converter with overflow-cell merging support.",
    long_about = None
)]
pub struct CliArgs {
    /// 変換対象のExcelファイル (.xlsx) のパス。
    #[arg(value_name = "INPUT.xlsx")]
    pub input: PathBuf,

    /// 出力先ファイルまたはディレクトリのパス。
    /// 指定がない場合は、標準出力（--split 指定時は入力ファイル名ベースのディレクトリ）へ書き出します。
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// 変換対象にするシート名。複数指定された場合はそれらのみを変換します。
    /// 指定がない場合は、ファイル内の全シートを変換します。
    #[arg(short, long, value_name = "NAME")]
    pub sheet: Vec<String>,

    /// 解析戦略を指定します。"auto" の場合はシート構造から自動選択します。
    #[arg(
        long,
        value_name = "STRATEGY",
        default_value = "auto",
        value_parser = ["auto", "grid-paper", "tabular"]
    )]
    pub strategy: String,

    /// はみ出し判定の感度調整パラメータ（grid-paper戦略で有効）。
    /// 値が小さいほど結合されやすく、大きいほど結合されにくくなります。
    #[arg(long, value_name = "N", default_value_t = 1.0)]
    pub overflow_threshold: f64,

    /// はみ出し結合を無効化し、セル単位でそのまま変換します（--strategy tabular の別名）。
    #[arg(long, conflicts_with = "strategy")]
    pub no_overflow_merge: bool,

    /// シートごとに別々のMarkdownファイルに分割して出力します。
    #[arg(long)]
    pub split: bool,

    /// 出力先ディレクトリ内の既存の Markdown ファイル (.md) を書き込み前に削除します。
    /// （--split 指定時のみ有効）
    #[arg(long, requires = "split")]
    pub clean: bool,

    /// 出力ファイル名またはディレクトリ名に実行日時のタイムスタンプを付与します。
    #[arg(long)]
    pub timestamp: bool,

    /// 詳細なログ（デバッグ情報など）を標準エラー出力へ表示します。
    #[arg(short, long)]
    pub verbose: bool,

    /// 1シートあたりに許容する最大セル数（rows × cols）。
    /// 悪意ある/破損したxlsxファイルがシートの座標情報のみを巨大化させることで
    /// 発生するメモリ枯渇 (DoS) を防ぐための上限。
    #[arg(long, value_name = "N", default_value_t = 1_000_000)]
    pub max_cells: usize,
}

/// `CliArgs`をパースし、ライブラリで利用可能な`ConvertConfig`に変換する。
pub fn build_config(args: CliArgs) -> Result<ConvertConfig, String> {
    // A) StrategyConfig の組み立て
    let strategy_config = StrategyConfig {
        overflow_threshold: args.overflow_threshold,
        ..StrategyConfig::default()
    };

    let strategy_id = if args.no_overflow_merge {
        "tabular".to_string()
    } else {
        args.strategy
    };

    // B) タイムスタンプ文字列の生成
    let timestamp_suffix = if args.timestamp {
        let now = chrono::Local::now();
        Some(now.format("_%Y%m%d_%H%M%S").to_string())
    } else {
        None
    };

    // C) OutputTarget の組み立て
    let output_target = if args.split {
        build_split_target(&args.input, args.output, &timestamp_suffix)?
    } else {
        build_single_target(args.output, &timestamp_suffix, args.timestamp)
    };

    Ok(ConvertConfig {
        input_path: args.input,
        sheet_names: args.sheet,
        strategy_id,
        strategy_config,
        output_target,
        // `--clean`によるファイル削除は`build_config`(設定構築フェーズ)では実行しない。
        // ここで実行すると、後続の`convert`が入力ファイル未検出等で失敗した場合でも
        // 出力先の既存ファイルが消えてしまうため、`convert`側で入力の妥当性確認が
        // すべて成功した後、書き込み直前に実行する（PR #23レビューコメントでの指摘を反映）。
        clean: args.clean,
        max_cells: args.max_cells,
    })
}

/// `--split`指定時の出力先ディレクトリパスを組み立てる。既存ファイルの削除等の副作用は
/// 一切持たない、純粋なマッピング処理に留める。
fn build_split_target(
    input: &std::path::Path,
    output: Option<PathBuf>,
    timestamp_suffix: &Option<String>,
) -> Result<OutputTarget, String> {
    let mut base_dir = match output {
        Some(out) => out,
        None => {
            let stem = input
                .file_stem()
                .ok_or_else(|| "Failed to get input file stem".to_string())?;
            PathBuf::from(stem)
        }
    };

    if let Some(suffix) = timestamp_suffix {
        let mut name = base_dir
            .file_name()
            .ok_or_else(|| "Failed to get base dir name".to_string())?
            .to_os_string();
        name.push(suffix);
        base_dir.set_file_name(name);
    }

    Ok(OutputTarget::SplitDirectory(base_dir))
}

fn build_single_target(
    output: Option<PathBuf>,
    timestamp_suffix: &Option<String>,
    timestamp_requested: bool,
) -> OutputTarget {
    match output {
        Some(mut path) => {
            if let Some(suffix) = timestamp_suffix {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("md");
                    path.set_file_name(format!("{stem}{suffix}.{ext}"));
                }
            }
            OutputTarget::SingleFile(path)
        }
        None => {
            if timestamp_requested {
                log::warn!(
                    "--timestamp was specified but outputting to stdout. The timestamp will be ignored."
                );
            }
            OutputTarget::Stdout
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args() -> CliArgs {
        CliArgs {
            input: PathBuf::from("input.xlsx"),
            output: None,
            sheet: vec![],
            strategy: "auto".to_string(),
            overflow_threshold: 1.0,
            no_overflow_merge: false,
            split: false,
            clean: false,
            timestamp: false,
            verbose: false,
            max_cells: 1_000_000,
        }
    }

    #[test]
    fn build_config_defaults_to_stdout() {
        let config = build_config(base_args()).unwrap();
        assert!(matches!(config.output_target, OutputTarget::Stdout));
        assert_eq!(config.strategy_id, "auto");
    }

    #[test]
    fn build_config_no_overflow_merge_forces_tabular_strategy() {
        let mut args = base_args();
        args.no_overflow_merge = true;
        let config = build_config(args).unwrap();
        assert_eq!(config.strategy_id, "tabular");
    }

    #[test]
    fn build_config_output_without_split_is_single_file() {
        let mut args = base_args();
        args.output = Some(PathBuf::from("out.md"));
        let config = build_config(args).unwrap();
        match config.output_target {
            OutputTarget::SingleFile(path) => assert_eq!(path, PathBuf::from("out.md")),
            other => panic!("expected SingleFile, got {other:?}"),
        }
    }

    #[test]
    fn build_config_split_without_output_uses_input_file_stem() {
        let mut args = base_args();
        args.input = PathBuf::from("path/to/report.xlsx");
        args.split = true;
        let config = build_config(args).unwrap();
        match config.output_target {
            OutputTarget::SplitDirectory(dir) => assert_eq!(dir, PathBuf::from("report")),
            other => panic!("expected SplitDirectory, got {other:?}"),
        }
    }

    #[test]
    fn build_config_split_with_output_uses_given_directory() {
        let mut args = base_args();
        args.split = true;
        args.output = Some(PathBuf::from("out_dir"));
        let config = build_config(args).unwrap();
        match config.output_target {
            OutputTarget::SplitDirectory(dir) => assert_eq!(dir, PathBuf::from("out_dir")),
            other => panic!("expected SplitDirectory, got {other:?}"),
        }
    }

    #[test]
    fn build_config_timestamp_appends_suffix_before_extension_for_single_file() {
        let mut args = base_args();
        args.output = Some(PathBuf::from("out.md"));
        args.timestamp = true;
        let config = build_config(args).unwrap();
        match config.output_target {
            OutputTarget::SingleFile(path) => {
                let name = path.file_name().unwrap().to_str().unwrap();
                assert!(name.starts_with("out_"));
                assert!(name.ends_with(".md"));
                assert_ne!(name, "out.md");
            }
            other => panic!("expected SingleFile, got {other:?}"),
        }
    }

    #[test]
    fn build_config_timestamp_appends_suffix_to_split_directory_name() {
        let mut args = base_args();
        args.split = true;
        args.output = Some(PathBuf::from("out_dir"));
        args.timestamp = true;
        let config = build_config(args).unwrap();
        match config.output_target {
            OutputTarget::SplitDirectory(dir) => {
                let name = dir.file_name().unwrap().to_str().unwrap();
                assert!(name.starts_with("out_dir_"));
            }
            other => panic!("expected SplitDirectory, got {other:?}"),
        }
    }

    #[test]
    fn build_config_timestamp_without_output_falls_back_to_stdout() {
        let mut args = base_args();
        args.timestamp = true;
        let config = build_config(args).unwrap();
        assert!(matches!(config.output_target, OutputTarget::Stdout));
    }

    #[test]
    fn build_config_overflow_threshold_is_forwarded() {
        let mut args = base_args();
        args.overflow_threshold = 2.5;
        let config = build_config(args).unwrap();
        assert_eq!(config.strategy_config.overflow_threshold, 2.5);
    }
}
