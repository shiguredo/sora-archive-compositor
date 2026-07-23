use sora_archive_compositor::logger;

const HELP_FLAG: noargs::FlagSpec = noargs::HELP_FLAG
    .doc("このヘルプメッセージを表示します ('--help' なら詳細、'-h' なら簡易版を表示)");
const VERSION_FLAG: noargs::FlagSpec = noargs::VERSION_FLAG.doc("バージョン番号を表示します");
const VERBOSE_FLAG: noargs::FlagSpec =
    noargs::flag("verbose").doc("警告未満のログメッセージも出力します");

const INSPECT_COMMAND: noargs::CmdSpec =
    noargs::cmd("inspect").doc("録画ファイルの情報を取得します");
const LIST_CODECS_COMMAND: noargs::CmdSpec =
    noargs::cmd("list-codecs").doc("利用可能なコーデック一覧を表示します");
const COMPOSE_COMMAND: noargs::CmdSpec = noargs::cmd("compose").doc("録画ファイルの合成を行います");
const VMAF_COMMAND: noargs::CmdSpec =
    noargs::cmd("vmaf").doc("VMAF を用いた映像エンコード品質の評価を行います");
const TUNE_COMMAND: noargs::CmdSpec =
    noargs::cmd("tune").doc("映像エンコードパラメーターの調整を行います");
const GENERATE_ARCHIVE_COMMAND: noargs::CmdSpec =
    noargs::cmd("generate-archive").doc("ダミーの録画データを生成します");

fn main() -> noargs::Result<()> {
    let mut args = noargs::raw_args();
    args.metadata_mut().app_name = env!("CARGO_PKG_NAME");
    args.metadata_mut().app_description = env!("CARGO_PKG_DESCRIPTION");

    // 共通系のフラグ引数は先に処理する
    HELP_FLAG.take_help(&mut args);

    if VERSION_FLAG.take(&mut args).is_present() {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if VERBOSE_FLAG.take(&mut args).is_present() {
        logger::init(tracing::level_filters::LevelFilter::DEBUG);
    } else {
        logger::init(tracing::level_filters::LevelFilter::WARN);
    };

    // サブコマンドで分岐する
    if INSPECT_COMMAND.take(&mut args).is_present() {
        sora_archive_compositor::subcommand_inspect::run(args)?;
    } else if LIST_CODECS_COMMAND.take(&mut args).is_present() {
        sora_archive_compositor::subcommand_list_codecs::run(args)?;
    } else if COMPOSE_COMMAND.take(&mut args).is_present() {
        sora_archive_compositor::subcommand_compose::run(args)?;
    } else if VMAF_COMMAND.take(&mut args).is_present() {
        sora_archive_compositor::subcommand_vmaf::run(args)?;
    } else if TUNE_COMMAND.take(&mut args).is_present() {
        sora_archive_compositor::subcommand_tune::run(args)?;
    } else if GENERATE_ARCHIVE_COMMAND.take(&mut args).is_present() {
        sora_archive_compositor::subcommand_generate_archive::run(args)?;
    } else if let Some(help) = args.finish()? {
        print!("{help}");
    }

    Ok(())
}
