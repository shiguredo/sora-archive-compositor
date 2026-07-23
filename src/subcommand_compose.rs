use std::{num::NonZeroUsize, path::PathBuf, time::Duration};

use shiguredo_openh264::Openh264Library;

use crate::{
    composer::Composer,
    layout::{DEFAULT_LAYOUT_JSON, Layout},
    stats::{ProcessorStats, Stats},
};

#[derive(Debug)]
struct Args {
    layout_file_path: Option<PathBuf>,
    output_file_path: Option<PathBuf>,
    stats_file_path: Option<PathBuf>,
    openh264: Option<PathBuf>,
    #[cfg(feature = "fdk-aac")]
    fdk_aac: Option<PathBuf>,
    no_progress_bar: bool,
    worker_threads: NonZeroUsize,
    root_dir: PathBuf,
}

impl Args {
    fn parse(raw_args: &mut noargs::RawArgs) -> noargs::Result<Self> {
        Ok(Self {
            layout_file_path: noargs::opt("layout-file")
                .short('l')
                .ty("PATH")
                .env("SORA_ARCHIVE_COMPOSITOR_LAYOUT_FILE_PATH")
                .default("SORA_ARCHIVE_COMPOSITOR_REPO/layout-examples/compose-default.jsonc")
                .doc("合成に使用するレイアウトファイルを指定します")
                .take(raw_args)
                .then(crate::arg_utils::parse_non_default_opt)?,
            output_file_path: noargs::opt("output-file")
                .short('o')
                .ty("PATH")
                .default("ROOT_DIR/output.mp4")
                .doc("合成結果を保存するファイルを指定します")
                .take(raw_args)
                .then(crate::arg_utils::parse_non_default_opt)?,
            stats_file_path: noargs::opt("stats-file")
                .short('s')
                .ty("PATH")
                .doc("合成中に収集した統計情報 (JSON) を保存するファイルを指定します")
                .take(raw_args)
                .present_and_then(|a| a.value().parse())?,
            openh264: noargs::opt("openh264")
                .ty("PATH")
                .env("SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH")
                .doc("OpenH264 の共有ライブラリのパスを指定します")
                .take(raw_args)
                .present_and_then(|a| a.value().parse())?,
            #[cfg(feature = "fdk-aac")]
            fdk_aac: noargs::opt("fdk-aac")
                .ty("PATH")
                .env("SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH")
                .doc("FDK-AAC の共有ライブラリのパスを指定します")
                .take(raw_args)
                .present_and_then(|a| a.value().parse())?,
            no_progress_bar: noargs::flag("no-progress-bar")
                .short('P')
                .doc("指定された場合は、合成の進捗を非表示にします")
                .take(raw_args)
                .is_present(),
            worker_threads: noargs::opt("thread-count")
                .short('T')
                .ty("INTEGER")
                .default("1")
                .env("SORA_ARCHIVE_COMPOSITOR_THREAD_COUNT")
                .doc(concat!(
                    "合成処理に使用するワーカースレッド数を指定します\n",
                    "\n",
                    "なおこれはあくまでも Sora Archive Compositor 自体が起動するスレッドの数であり、\n",
                    "各エンコーダーやデコーダーが内部で起動するスレッドには関与しません",
                ))
                .take(raw_args)
                .then(|a| a.value().parse())?,
            root_dir: noargs::arg("ROOT_DIR")
                .example("/path/to/archive/RECORDING_ID/")
                .doc(concat!(
                    "合成処理を行う際のルートディレクトリを指定します\n",
                    "\n",
                    "レイアウトファイル内に記載された相対パスの基点は、",
                    "このディレクトリとなります。\n",
                    "また、レイアウト内で、",
                    "このディレクトリの外のファイルが参照された場合にはエラーとなります。"
                ))
                .take(raw_args)
                .then(crate::arg_utils::validate_existing_directory_path)?,
        })
    }
}

pub fn run(mut raw_args: noargs::RawArgs) -> noargs::Result<()> {
    let args = Args::parse(&mut raw_args)?;
    if let Some(help) = raw_args.finish()? {
        print!("{help}");
        return Ok(());
    }

    // レイアウトを準備
    let layout = Layout::from_layout_json_file_or_default(
        args.root_dir.clone(),
        args.layout_file_path.as_deref(),
        DEFAULT_LAYOUT_JSON,
    )?;
    tracing::debug!("layout: {layout:?}");

    // 必要に応じて openh264 の共有ライブラリを読み込む
    let openh264_lib = if let Some(path) = args.openh264.as_ref().filter(|_| layout.has_video()) {
        Some(Openh264Library::load(path)?)
    } else {
        None
    };

    // 必要に応じて FDK-AAC の共有ライブラリを読み込む
    #[cfg(feature = "fdk-aac")]
    let fdk_aac_lib = args
        .fdk_aac
        .as_ref()
        .map(shiguredo_fdk_aac::FdkAacLibrary::load)
        .transpose()?;

    // 出力ファイルパスを決定
    let output_file_path = args
        .output_file_path
        .unwrap_or_else(|| args.root_dir.join("output.mp4"));

    // Composer を作成して設定
    let mut composer = Composer::new(layout);
    composer.openh264_lib = openh264_lib;
    #[cfg(feature = "fdk-aac")]
    {
        composer.fdk_aac_lib = fdk_aac_lib;
    }
    composer.show_progress_bar = !args.no_progress_bar;
    composer.worker_threads = args.worker_threads;
    composer.stats_file_path = args.stats_file_path;

    // 合成を実行
    let result = composer.compose(&output_file_path)?;

    if !result.success {
        // エラー発生時は終了コードを変える
        std::process::exit(1);
    }

    crate::json::pretty_print(nojson::json(|f| {
        f.object(|f| {
            if let Some(path) = &args.layout_file_path {
                f.member("layout_file_path", path)?;
            }
            if let Some(path) = &composer.stats_file_path {
                f.member("stats_file_path", path)?;
            }
            f.member("input_root_dir", &args.root_dir)?;
            print_input_stats_summary(f, &result.stats)?;
            f.member("output_file_path", &output_file_path)?;
            print_output_stats_summary(f, &result.stats)?;
            print_time_stats_summary(f, &result.stats)?;

            Ok(())
        })
    }))?;

    Ok(())
}

fn print_input_stats_summary(
    f: &mut nojson::JsonObjectFormatter<'_, '_, '_>,
    stats: &Stats,
) -> std::fmt::Result {
    // NOTE: 個別の reader / decoder の情報を出すと JSON の要素数が可変かつ挙動になる可能性があるので省く
    //（その情報が必要なら stats ファイルを出力して、そっちを参照するのがいい）
    let count = stats
        .processors
        .iter()
        .filter(|s| {
            matches!(
                s,
                ProcessorStats::WebmAudioReader(_) | ProcessorStats::Mp4AudioReader(_)
            )
        })
        .count();
    if count > 0 {
        f.member("input_audio_source_count", count)?;
    }

    let count = stats
        .processors
        .iter()
        .filter(|s| {
            matches!(
                s,
                ProcessorStats::WebmVideoReader(_) | ProcessorStats::Mp4VideoReader(_)
            )
        })
        .count();
    if count > 0 {
        f.member("input_video_source_count", count)?;
    }

    Ok(())
}

fn print_output_stats_summary(
    f: &mut nojson::JsonObjectFormatter<'_, '_, '_>,
    stats: &Stats,
) -> std::fmt::Result {
    let Some(ProcessorStats::Mp4Writer(writer)) = stats
        .processors
        .iter()
        .find(|x| matches!(x, ProcessorStats::Mp4Writer(_)))
    else {
        return Ok(());
    };

    if let Some(codec) = writer.audio_codec.get() {
        f.member("output_audio_codec", codec)?;

        for processor in &stats.processors {
            if let ProcessorStats::AudioEncoder(encoder) = processor {
                f.member("output_audio_encode_engine", encoder.engine)?;
                break;
            }
        }

        f.member(
            "output_audio_duration_seconds",
            writer.total_audio_track_duration.get().as_secs_f32(),
        )?;

        let duration = writer.total_audio_track_duration.get();
        if !duration.is_zero() {
            let bitrate = (writer.total_audio_sample_data_byte_size.get() as f32 * 8.0)
                / duration.as_secs_f32();
            f.member("output_audio_bitrate", bitrate as u64)?;
        }
    }
    if let Some(codec) = writer.video_codec.get() {
        f.member("output_video_codec", codec)?;

        for processor in &stats.processors {
            if let ProcessorStats::VideoEncoder(encoder) = processor {
                f.member("output_video_encode_engine", encoder.engine)?;
                break;
            }
        }

        f.member(
            "output_video_duration_seconds",
            writer.total_video_track_duration.get().as_secs_f32(),
        )?;

        let duration = writer.total_video_track_duration.get();
        if !duration.is_zero() {
            let bitrate = (writer.total_video_sample_data_byte_size.get() as f32 * 8.0)
                / duration.as_secs_f32();
            f.member("output_video_bitrate", bitrate as u64)?;
        }
    }

    for processor in &stats.processors {
        match processor {
            ProcessorStats::AudioMixer(_mixer) => {}
            ProcessorStats::VideoMixer(mixer) => {
                f.member("output_video_width", mixer.output_video_resolution.width)?;
                f.member("output_video_height", mixer.output_video_resolution.height)?;
            }
            _ => {}
        }
    }

    Ok(())
}

/// 指定 processor の処理時間合計を求め、0 でなければ JSON メンバーを出力する
fn print_total_processing_seconds_if_nonzero(
    f: &mut nojson::JsonObjectFormatter<'_, '_, '_>,
    stats: &Stats,
    member_name: &str,
    extract_duration: impl Fn(&ProcessorStats) -> Option<Duration>,
) -> std::fmt::Result {
    let total = stats
        .processors
        .iter()
        .filter_map(extract_duration)
        .sum::<Duration>();
    if !total.is_zero() {
        f.member(member_name, total.as_secs_f64())?;
    }
    Ok(())
}

fn print_time_stats_summary(
    f: &mut nojson::JsonObjectFormatter<'_, '_, '_>,
    stats: &Stats,
) -> std::fmt::Result {
    print_total_processing_seconds_if_nonzero(
        f,
        stats,
        "total_audio_decoder_processing_seconds",
        |processor| match processor {
            ProcessorStats::AudioDecoder(audio_decoder) => {
                Some(audio_decoder.total_processing_duration.get())
            }
            _ => None,
        },
    )?;
    print_total_processing_seconds_if_nonzero(
        f,
        stats,
        "total_video_decoder_processing_seconds",
        |processor| match processor {
            ProcessorStats::VideoDecoder(video_decoder) => {
                Some(video_decoder.total_processing_duration.get())
            }
            _ => None,
        },
    )?;
    print_total_processing_seconds_if_nonzero(
        f,
        stats,
        "total_audio_encoder_processing_seconds",
        |processor| match processor {
            ProcessorStats::AudioEncoder(audio_encoder) => {
                Some(audio_encoder.total_processing_duration.get())
            }
            _ => None,
        },
    )?;
    print_total_processing_seconds_if_nonzero(
        f,
        stats,
        "total_video_encoder_processing_seconds",
        |processor| match processor {
            ProcessorStats::VideoEncoder(video_encoder) => {
                Some(video_encoder.total_processing_duration.get())
            }
            _ => None,
        },
    )?;
    print_total_processing_seconds_if_nonzero(
        f,
        stats,
        "total_audio_mixer_processing_seconds",
        |processor| match processor {
            ProcessorStats::AudioMixer(audio_mixer) => {
                Some(audio_mixer.total_processing_duration.get())
            }
            _ => None,
        },
    )?;
    print_total_processing_seconds_if_nonzero(
        f,
        stats,
        "total_video_mixer_processing_seconds",
        |processor| match processor {
            ProcessorStats::VideoMixer(video_mixer) => {
                Some(video_mixer.total_processing_duration.get())
            }
            _ => None,
        },
    )?;

    f.member("elapsed_seconds", stats.elapsed_duration.as_secs_f32())?;

    Ok(())
}
