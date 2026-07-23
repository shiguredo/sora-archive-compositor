use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::Duration,
};

use shiguredo_openh264::Openh264Library;
use shiguredo_vmaf::{BuiltinModel, Context, ContextConfig, Model, Picture, PoolingMethod};

use crate::{
    decoder::{VideoDecoder, VideoDecoderOptions},
    encoder::{VideoEncoder, VideoEncoderOptions},
    layout::Layout,
    media::{MediaSample, MediaStreamId},
    mixer_video::{VideoMixer, VideoMixerSpec},
    processor::{
        MediaProcessor, MediaProcessorInput, MediaProcessorOutput, MediaProcessorSpec,
        MediaProcessorWorkloadHint,
    },
    reader::VideoReader,
    scheduler::Scheduler,
    stats::ProcessorStats,
    types::EngineName,
    video::FrameRate,
    yuv::{YuvReader, YuvWriter},
};

const DEFAULT_LAYOUT_JSON: &str = include_str!("../layout-examples/vmaf-default.jsonc");

#[derive(Debug)]
struct Args {
    layout_file_path: Option<PathBuf>,
    reference_yuv_file_path: Option<PathBuf>,
    distorted_yuv_file_path: Option<PathBuf>,
    openh264: Option<PathBuf>,
    frame_count: usize,
    timeout: Option<Duration>,
    root_dir: PathBuf,
}

impl Args {
    fn parse(raw_args: &mut noargs::RawArgs) -> noargs::Result<Self> {
        Ok(Self {
            layout_file_path: noargs::opt("layout-file")
                .short('l')
                .ty("PATH")
                .env("SORA_ARCHIVE_COMPOSITOR_LAYOUT_FILE_PATH")
                .default("SORA_ARCHIVE_COMPOSITOR_REPO/layout-examples/vmaf-default.jsonc")
                .doc("合成に使用するレイアウトファイルを指定します")
                .take(raw_args)
                .then(crate::arg_utils::parse_non_default_opt)?,
            reference_yuv_file_path: noargs::opt("reference-yuv-file")
                .ty("PATH")
                .default("ROOT_DIR/reference.yuv")
                .doc("参照映像の YUV ファイルの出力先を指定します")
                .take(raw_args)
                .then(crate::arg_utils::parse_non_default_opt)?,
            distorted_yuv_file_path: noargs::opt("distorted-yuv-file")
                .ty("PATH")
                .default("ROOT_DIR/distorted.yuv")
                .doc("歪み映像の YUV ファイルの出力先を指定します")
                .take(raw_args)
                .then(crate::arg_utils::parse_non_default_opt)?,
            openh264: noargs::opt("openh264")
                .ty("PATH")
                .env("SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH")
                .doc("OpenH264 の共有ライブラリのパスを指定します")
                .take(raw_args)
                .present_and_then(|a| a.value().parse())?,
            frame_count: noargs::opt("frame-count")
                .short('f')
                .ty("FRAMES")
                .default("1000")
                .doc("変換するフレーム数を指定します")
                .take(raw_args)
                .then(|a| a.value().parse())?,
            timeout: noargs::opt("timeout")
                .ty("SECONDS")
                .doc("処理のタイムアウト時間（秒）を指定します（超過した場合は失敗扱い）")
                .take(raw_args)
                .present_and_then(|a| crate::arg_utils::parse_duration_secs(a.value()))?,
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

    // レイアウトを準備（音声処理は無効化）
    let mut layout = Layout::from_layout_json_file_or_default(
        args.root_dir.clone(),
        args.layout_file_path.as_deref(),
        DEFAULT_LAYOUT_JSON,
    )?;
    layout.audio_source_ids.clear();
    tracing::debug!("layout: {layout:?}");
    if !layout.has_video() {
        return Err(crate::Error::new("no video sources").into());
    }

    // 必要に応じて openh264 の共有ライブラリを読み込む
    let openh264_lib = if let Some(path) = args.openh264.as_ref().filter(|_| layout.has_video()) {
        Some(Openh264Library::load(path)?)
    } else {
        None
    };

    // プロセッサを準備
    let mut scheduler = Scheduler::new();
    let mut next_stream_id = MediaStreamId::new(0);

    // リーダーとデコーダーを登録
    let mut mixer_input_stream_ids = Vec::new();
    let decoder_options = VideoDecoderOptions {
        openh264_lib: openh264_lib.clone(),
        decode_params: layout.decode_params.clone(),
        engines: None,
    };
    for (source_id, source_info) in &layout.sources {
        if layout.video_source_ids().all(|id| id != source_id) {
            continue;
        }

        let reader_output_stream_id = next_stream_id.fetch_add(1);
        let reader = VideoReader::from_source_info(reader_output_stream_id, source_info)?;
        scheduler.register(reader)?;

        let decoder_output_stream_id = next_stream_id.fetch_add(1);
        let decoder = VideoDecoder::new(
            reader_output_stream_id,
            decoder_output_stream_id,
            decoder_options.clone(),
        );
        scheduler.register(decoder)?;

        mixer_input_stream_ids.push(decoder_output_stream_id);
    }

    // ミキサーを登録
    let mixer_output_stream_id = next_stream_id.fetch_add(1);
    let mixer = VideoMixer::new(
        VideoMixerSpec::from_layout(&layout),
        mixer_input_stream_ids,
        mixer_output_stream_id,
    );
    scheduler.register(mixer)?;

    // フレーム数を制限する
    let limiter_output_stream_id = next_stream_id.fetch_add(1);
    let limiter = FrameCountLimiter::new(
        mixer_output_stream_id,
        limiter_output_stream_id,
        args.frame_count,
    );
    scheduler.register(limiter)?;

    // エンコード前の画像の YUV 書き込みを登録
    let distorted_yuv_file_path = args
        .distorted_yuv_file_path
        .unwrap_or_else(|| args.root_dir.join("distorted.yuv"));
    let writer = YuvWriter::new(limiter_output_stream_id, &distorted_yuv_file_path)?;
    scheduler.register(writer)?;

    // エンコーダーを登録
    let encoder_output_stream_id = next_stream_id.fetch_add(1);
    let encoder = VideoEncoder::new(
        &VideoEncoderOptions::from_layout(&layout),
        limiter_output_stream_id,
        encoder_output_stream_id,
        openh264_lib.clone(),
    )?;
    let encode_engine = encoder.name();
    let encoder_stats = encoder.encoder_stats().clone();
    scheduler.register(encoder)?;

    // エンコード後の画像（のデコード結果）の YUV 書き込みを登録
    let decoder_output_stream_id = next_stream_id.fetch_add(1);
    let decoder = VideoDecoder::new(
        encoder_output_stream_id,
        decoder_output_stream_id,
        decoder_options.clone(),
    );
    scheduler.register(decoder)?;

    let reference_yuv_file_path = args
        .reference_yuv_file_path
        .unwrap_or_else(|| args.root_dir.join("reference.yuv"));
    let writer = YuvWriter::new(decoder_output_stream_id, &reference_yuv_file_path)?;
    scheduler.register(writer)?;

    // プログレスバーを準備
    let progress = ProgressBar::new(decoder_output_stream_id, args.frame_count as u64);
    scheduler.register(progress)?;

    // 合成処理を実行
    eprintln!("# Compose for VMAF");
    let (timeout_expired, stats) = if let Some(timeout) = args.timeout {
        scheduler.run_timeout(timeout)?
    } else {
        (false, scheduler.run()?)
    };
    if stats.error.get() {
        return Err(crate::Error::new(format!(
            "video composition process failed{}",
            if timeout_expired { " (timeout)" } else { "" }
        ))
        .into());
    }

    // VMAF の下準備としての処理は全て完了した
    eprintln!("=> done\n");

    // VMAF 評価を実行
    eprintln!("# Run VMAF evaluation");
    let vmaf = run_vmaf_evaluation(&reference_yuv_file_path, &distorted_yuv_file_path, &layout)?;
    eprintln!("=> done\n");

    // 実行結果の要約を標準出力に出力する
    let output = Output {
        layout_file_path: args.layout_file_path,
        reference_yuv_file_path,
        distorted_yuv_file_path,
        encode_engine,
        width: layout.resolution.width().get(),
        height: layout.resolution.height().get(),
        frame_rate: layout.frame_rate,
        encoded_frame_count: encoder_stats.total_output_video_frame_count.get() as usize,
        elapsed_duration: stats.elapsed_duration,
        vmaf,
    };
    println!(
        "{}",
        nojson::json(|f| {
            f.set_indent_size(2);
            f.set_spacing(true);
            f.value(&output)
        })
    );

    Ok(())
}

// 参照・劣化の YUV ファイルを読み込んで shiguredo_vmaf で VMAF スコアを評価する
//
// 固定で 8-bit I420、libvmaf のデフォルトモデル相当 (V061) を使う。
fn run_vmaf_evaluation(
    reference_yuv_file_path: &Path,
    distorted_yuv_file_path: &Path,
    layout: &Layout,
) -> crate::Result<VmafScoreStats> {
    let width = layout.resolution.width().get();
    let height = layout.resolution.height().get();
    let width_u32 =
        u32::try_from(width).map_err(|_| crate::Error::new("video width is too large"))?;
    let height_u32 =
        u32::try_from(height).map_err(|_| crate::Error::new("video height is too large"))?;

    let mut context = Context::new(ContextConfig::default())
        .map_err(|e| crate::Error::new(format!("failed to create VMAF context: {e}")))?;
    let model = Model::load_builtin(BuiltinModel::V061)
        .map_err(|e| crate::Error::new(format!("failed to load VMAF model: {e}")))?;
    context
        .use_model(&model)
        .map_err(|e| crate::Error::new(format!("failed to set VMAF model: {e}")))?;

    let mut reference_reader = YuvReader::new(reference_yuv_file_path, width, height)?;
    let mut distorted_reader = YuvReader::new(distorted_yuv_file_path, width, height)?;

    let mut frame_count: u32 = 0;
    loop {
        let reference_frame = reference_reader.read_frame()?;
        let distorted_frame = distorted_reader.read_frame()?;
        match (reference_frame, distorted_frame) {
            (Some(reference), Some(distorted)) => {
                let reference_picture = Picture::from_i420(
                    reference.y(),
                    reference.u(),
                    reference.v(),
                    width_u32,
                    height_u32,
                )
                .map_err(|e| {
                    crate::Error::new(format!("failed to build reference picture: {e}"))
                })?;
                let distorted_picture = Picture::from_i420(
                    distorted.y(),
                    distorted.u(),
                    distorted.v(),
                    width_u32,
                    height_u32,
                )
                .map_err(|e| {
                    crate::Error::new(format!("failed to build distorted picture: {e}"))
                })?;
                context
                    .read_pictures(
                        Some(reference_picture),
                        Some(distorted_picture),
                        frame_count,
                    )
                    .map_err(|e| crate::Error::new(format!("failed to read pictures: {e}")))?;
                frame_count += 1;
            }
            (None, None) => break,
            _ => {
                return Err(crate::Error::new(
                    "reference and distorted YUV files have different frame counts",
                ));
            }
        }
    }
    if frame_count == 0 {
        return Err(crate::Error::new("no frames to evaluate for VMAF"));
    }

    // 全フレーム登録後に両方 None を渡して評価を確定させる (flush)。flush 時は index は参照されない
    context
        .read_pictures(None, None, 0)
        .map_err(|e| crate::Error::new(format!("failed to flush VMAF pictures: {e}")))?;

    let last_index = frame_count - 1;
    let score_pooled = |method| {
        context
            .score_pooled(&model, method, 0, last_index)
            .map_err(|e| crate::Error::new(format!("failed to compute pooled VMAF score: {e}")))
    };
    Ok(VmafScoreStats {
        min: score_pooled(PoolingMethod::Min)?,
        max: score_pooled(PoolingMethod::Max)?,
        mean: score_pooled(PoolingMethod::Mean)?,
        harmonic_mean: score_pooled(PoolingMethod::HarmonicMean)?,
    })
}

#[derive(Debug)]
struct Output {
    layout_file_path: Option<PathBuf>,
    reference_yuv_file_path: PathBuf,
    distorted_yuv_file_path: PathBuf,
    encode_engine: EngineName,
    width: usize,
    height: usize,
    frame_rate: FrameRate,
    encoded_frame_count: usize,
    elapsed_duration: Duration,
    vmaf: VmafScoreStats,
}

impl nojson::DisplayJson for Output {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            if let Some(path) = &self.layout_file_path {
                f.member("layout_file_path", path)?;
            }
            f.member("reference_yuv_file_path", &self.reference_yuv_file_path)?;
            f.member("distorted_yuv_file_path", &self.distorted_yuv_file_path)?;
            f.member("encode_engine", self.encode_engine)?;
            f.member("width", self.width)?;
            f.member("height", self.height)?;
            f.member("frame_rate", self.frame_rate)?;
            f.member("encoded_frame_count", self.encoded_frame_count)?;
            f.member("elapsed_seconds", self.elapsed_duration.as_secs_f32())?;
            f.member("vmaf_min", self.vmaf.min)?;
            f.member("vmaf_max", self.vmaf.max)?;
            f.member("vmaf_mean", self.vmaf.mean)?;
            f.member("vmaf_harmonic_mean", self.vmaf.harmonic_mean)?;

            Ok(())
        })
    }
}

#[derive(Debug)]
struct VmafScoreStats {
    min: f64,
    max: f64,
    mean: f64,
    harmonic_mean: f64,
}

// 処理対象のフレーム数を制限するためのプロセッサ
#[derive(Debug)]
struct FrameCountLimiter {
    input_stream_id: MediaStreamId,
    output_stream_id: MediaStreamId,
    remaining_frame_count: usize,
    queue: VecDeque<MediaSample>,
}

impl FrameCountLimiter {
    fn new(
        input_stream_id: MediaStreamId,
        output_stream_id: MediaStreamId,
        total_frame_count: usize,
    ) -> Self {
        Self {
            input_stream_id,
            output_stream_id,
            remaining_frame_count: total_frame_count,
            queue: VecDeque::new(),
        }
    }
}

impl MediaProcessor for FrameCountLimiter {
    fn spec(&self) -> MediaProcessorSpec {
        MediaProcessorSpec {
            input_stream_ids: vec![self.input_stream_id],
            output_stream_ids: vec![self.output_stream_id],
            stats: ProcessorStats::other("frame-count-limiter"),
            workload_hint: MediaProcessorWorkloadHint::CPU_MISC,
        }
    }

    fn process_input(&mut self, input: MediaProcessorInput) -> crate::Result<()> {
        if let Some(sample) = input.sample
            && let Some(n) = self.remaining_frame_count.checked_sub(1)
        {
            self.queue.push_back(sample);
            self.remaining_frame_count = n;
        } else {
            // 指定数フレームを処理した or 入力が EOS に達した
            self.remaining_frame_count = 0;
        };
        Ok(())
    }

    fn process_output(&mut self) -> crate::Result<MediaProcessorOutput> {
        if let Some(sample) = self.queue.pop_front() {
            Ok(MediaProcessorOutput::Processed {
                stream_id: self.output_stream_id,
                sample,
            })
        } else if self.remaining_frame_count == 0 {
            Ok(MediaProcessorOutput::Finished)
        } else {
            Ok(MediaProcessorOutput::pending(self.input_stream_id))
        }
    }
}

#[derive(Debug)]
struct ProgressBar {
    input_stream_id: MediaStreamId,
    eos: bool,
    bar: crate::progress::ProgressBar,
}

impl ProgressBar {
    fn new(input_stream_id: MediaStreamId, total_frame_count: u64) -> Self {
        Self {
            input_stream_id,
            eos: false,
            bar: crate::progress::ProgressBar::new(
                total_frame_count,
                crate::progress::ProgressKind::Frame,
            ),
        }
    }
}

impl MediaProcessor for ProgressBar {
    fn spec(&self) -> MediaProcessorSpec {
        MediaProcessorSpec {
            input_stream_ids: vec![self.input_stream_id],
            output_stream_ids: Vec::new(),
            stats: ProcessorStats::other("progress_bar"),
            workload_hint: MediaProcessorWorkloadHint::WRITER,
        }
    }

    fn process_input(&mut self, input: MediaProcessorInput) -> crate::Result<()> {
        if input.sample.is_some() {
            self.bar.inc(1);
        } else {
            self.eos = true;
            self.bar.finish();
        };
        Ok(())
    }

    fn process_output(&mut self) -> crate::Result<MediaProcessorOutput> {
        if self.eos {
            Ok(MediaProcessorOutput::Finished)
        } else {
            Ok(MediaProcessorOutput::pending(self.input_stream_id))
        }
    }
}
