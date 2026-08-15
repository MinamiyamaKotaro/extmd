//! 薄いバイナリエントリポイント(docs/design/cli.md 6.3節)。引数パース・ロギング初期化・
//! `extmd::convert`の呼び出しとエラー時の終了コード設定のみを担う。

use clap::Parser;
use extmd::cli;

fn main() {
    let args = cli::CliArgs::parse();

    let log_level = if args.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    };

    env_logger::Builder::new()
        .filter(None, log_level)
        .target(env_logger::Target::Stderr)
        .init();

    let config = match cli::build_config(args) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = extmd::convert(config) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
