use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use shiguredo_openh264::Openh264Library;

use crate::{
    encoder::{VideoEncoder, VideoEncoderOptions},
    layout::Resolution,
    layout_encode_params::LayoutEncodeParams,
    media::MediaStreamId,
    processor::{MediaProcessor, MediaProcessorInput, MediaProcessorOutput},
    progress::{ProgressBar, ProgressKind},
    types::CodecName,
    video::{FrameRate, VideoFormat, VideoFrame},
    writer_mp4::{Mp4Writer, Mp4WriterOptions},
};

#[derive(Debug)]
struct Args {
    output_dir: PathBuf,
    connection_id: String,
    resolution: Resolution,
    frame_rate: FrameRate,
    start_time: u64,
    duration: u64,
    seed: u64,
    codec: CodecName,
    openh264: Option<PathBuf>,
    resolution_changes: Vec<ResolutionChange>,
}

/// 録画の途中での解像度変更指定 (録画開始からの経過秒と変更後の解像度)
#[derive(Debug, Clone)]
struct ResolutionChange {
    time: u64,
    resolution: Resolution,
}

impl FromStr for ResolutionChange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((time, resolution)) = s.split_once(':') else {
            return Err(format!("invalid resolution-change: {s}"));
        };
        let time = time
            .parse()
            .map_err(|_| format!("invalid resolution-change time: {s}"))?;
        let resolution = resolution
            .parse()
            .map_err(|_| format!("invalid resolution-change resolution: {s}"))?;
        Ok(Self { time, resolution })
    }
}

impl Args {
    fn parse(raw_args: &mut noargs::RawArgs) -> noargs::Result<Self> {
        // noargs の仕様上、位置引数 (arg) はオプション (opt / flag) より先に
        // take すると、オプションを位置引数の前に指定できなくなる
        // (arg が先頭の未消費トークンを無条件に消費してしまうため)。
        // オプションを先に take してから、最後に位置引数を take すること。
        Ok(Self {
            connection_id: noargs::opt("connection-id")
                .ty("ID")
                .doc("archive JSON の connection_id を指定します")
                .take(raw_args)
                .present_and_then(|a| Ok::<_, Box<dyn std::error::Error>>(a.value().parse()?))?
                .unwrap_or_else(generate_connection_id),
            resolution: noargs::opt("resolution")
                .ty("WxH")
                .default("1280x720")
                .doc("出力解像度を指定します")
                .take(raw_args)
                .then(|a| a.value().parse())?,
            frame_rate: noargs::opt("frame-rate")
                .ty("N")
                .default("30")
                .doc("フレームレートを指定します")
                .take(raw_args)
                .then(|a| a.value().parse())?,
            start_time: noargs::opt("start-time")
                .ty("SECONDS")
                .default("0")
                .doc("録画の開始時刻を指定します")
                .take(raw_args)
                .then(|a| a.value().parse())?,
            duration: noargs::opt("duration")
                .ty("SECONDS")
                .default("10")
                .doc("録画の長さを指定します")
                .take(raw_args)
                .then(|a| a.value().parse())?,
            seed: noargs::opt("seed")
                .ty("N")
                .doc("映像パターンの乱数シードを指定します (未指定の場合はランダムに決定されます)")
                .take(raw_args)
                .present_and_then(|a| a.value().parse())?
                .unwrap_or_else(generate_seed),
            codec: noargs::opt("codec")
                .ty("CODEC")
                .default("VP9")
                .doc("出力映像のコーデックを指定します (VP8 / VP9 / H264 / H265 / AV1)")
                .take(raw_args)
                .then(|a| CodecName::parse_video(a.value()))?,
            openh264: noargs::opt("openh264")
                .ty("PATH")
                .env("SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH")
                .doc("OpenH264 の共有ライブラリのパスを指定します (H264 エンコードに使用)")
                .take(raw_args)
                .present_and_then(|a| a.value().parse())?,
            resolution_changes: {
                let opt = noargs::opt("resolution-change").ty("TIME:WxH").doc(concat!(
                    "録画の途中で解像度を変更します (複数回指定可能)\n",
                    "TIME は録画開始からの経過秒で、--duration 未満の範囲内です"
                ));
                let mut changes = Vec::new();
                while let Some(change) = opt
                    .take(raw_args)
                    .present_and_then(|a| a.value().parse::<ResolutionChange>())?
                {
                    changes.push(change);
                }
                changes
            },
            output_dir: noargs::arg("OUTPUT_DIR")
                .example("/path/to/output/")
                .doc("ダミー録画データの出力先ディレクトリを指定します")
                .take(raw_args)
                .then(crate::arg_utils::validate_existing_directory_path)?,
        })
    }
}

/// `--resolution-change` の指定が有効かどうかを検証する
fn validate_resolution_changes(args: &Args) -> crate::Result<()> {
    let mut previous_time = None;
    let mut previous_resolution = args.resolution;
    for change in &args.resolution_changes {
        if change.time >= args.duration {
            return Err(crate::Error::new(format!(
                "invalid --resolution-change: time {} must be less than duration {}",
                change.time, args.duration
            )));
        }
        if previous_time.is_some_and(|t| change.time <= t) {
            return Err(crate::Error::new(
                "invalid --resolution-change: time must be monotonically increasing",
            ));
        }
        if change.resolution == previous_resolution {
            return Err(crate::Error::new(format!(
                "invalid --resolution-change: resolution {}x{} must differ from the previous resolution",
                change.resolution.width().get(),
                change.resolution.height().get()
            )));
        }
        previous_time = Some(change.time);
        previous_resolution = change.resolution;
    }
    Ok(())
}

/// connection_id の自動生成 (タイムスタンプ + 乱数ベース)
fn generate_connection_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX epoch")
        .as_nanos();
    // 26 文字の英数字 (Sora の connection_id 形式に似せたもの)
    let charset: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut x = nanos;
    let mut id = String::with_capacity(26);
    for _ in 0..26 {
        id.push(charset[(x % charset.len() as u128) as usize] as char);
        x /= charset.len() as u128;
    }
    id
}

/// seed の自動生成 (システム時刻ベース)
///
/// 再現性が欲しい場合は `--seed` で明示的に指定する。
/// 指定された seed は最終的なログに含まれるため、`--verbose` で確認できる。
fn generate_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX epoch")
        .as_nanos();
    // 下位 64 ビットのみを使う (上位ビットはほぼ固定値のため)
    nanos as u64
}

pub fn run(mut raw_args: noargs::RawArgs) -> noargs::Result<()> {
    let args = Args::parse(&mut raw_args)?;
    if let Some(help) = raw_args.finish()? {
        print!("{help}");
        return Ok(());
    }

    // 必要に応じて openh264 の共有ライブラリを読み込む
    let openh264_lib = args
        .openh264
        .as_ref()
        .map(Openh264Library::load)
        .transpose()?;

    run_impl(&args, openh264_lib).map_err(|e| noargs::Error::from(e.reason))
}

fn run_impl(args: &Args, openh264_lib: Option<Openh264Library>) -> crate::Result<()> {
    validate_resolution_changes(args)?;

    let frame_count = args.frame_rate.frame_count_for_secs(args.duration);
    let fps = args.frame_rate.frames_per_second();

    // フレーム番号ベースの解像度セグメントを計算する
    let segments = build_resolution_segments(args.resolution, &args.resolution_changes, fps);

    // フレーム描画 (raden)
    let frames = draw_frames(&segments, frame_count, fps, args.seed, &args.connection_id)?;

    // エンコード + MP4 書き出し
    let mp4_path = args
        .output_dir
        .join(format!("archive-{}.mp4", args.connection_id));
    encode_and_write_mp4(&mp4_path, args, &segments, &frames, openh264_lib)?;

    // archive JSON を書き出す
    write_archive_json(args)?;

    tracing::info!(
        "generated dummy archive: {} ({}x{}@{}fps, {} seconds, codec: {}, seed: {}, resolution changes: {})",
        args.connection_id,
        args.resolution.width().get(),
        args.resolution.height().get(),
        args.frame_rate.numerator,
        args.duration,
        args.codec.as_str(),
        args.seed,
        args.resolution_changes.len(),
    );
    Ok(())
}

/// 録画の途中での解像度変更を、フレーム番号ベースのセグメント列に変換する
///
/// セグメントは録画の開始から終了までを重複なく覆い、`start_frame` は
/// そのセグメントの最初のフレーム番号を表す。
#[derive(Debug)]
struct ResolutionSegment {
    start_frame: u64,
    width: usize,
    height: usize,
}

/// 解像度スケジュールをフレーム番号ベースのセグメント列に変換する
///
/// 指定時刻 T 秒は、タイムスタンプが T 秒以上になる最初のフレーム (切り上げ) から
/// 新しい解像度を適用する。
fn build_resolution_segments(
    initial_resolution: Resolution,
    resolution_changes: &[ResolutionChange],
    fps: f64,
) -> Vec<ResolutionSegment> {
    let mut segments = Vec::with_capacity(resolution_changes.len() + 1);
    let mut start_frame = 0;
    let mut width = initial_resolution.width().get();
    let mut height = initial_resolution.height().get();
    for change in resolution_changes {
        segments.push(ResolutionSegment {
            start_frame,
            width,
            height,
        });
        start_frame = (change.time as f64 * fps).ceil() as u64;
        width = change.resolution.width().get();
        height = change.resolution.height().get();
    }
    segments.push(ResolutionSegment {
        start_frame,
        width,
        height,
    });
    segments
}

/// raden で描画した Prgb32 フレームを I420 に変換したもの
struct RenderedFrame {
    yuv_data: Vec<u8>,
    width: usize,
    height: usize,
}

/// raden で実ユースケース風の映像を描画する
///
/// raden の examples/animation.rs を参考にしたパターン:
/// - seed で決まる色相の暗い固定背景 (ウェブ会議用途を想定し、背景色は時間変化させない)
/// - 横方向に並ぶ円が sin 波で上下に動くウェーブパターン
/// - リサージュ曲線風に動く半透明のバウンドする円
/// - 中央周囲を周回する円群
/// - 左上に connection_id をピクセルアート (5x7 ドット) で表示する
///
/// 解像度はセグメント単位で切り替わり、各フレームはその時点のセグメントの
/// width / height で描画される。
fn draw_frames(
    segments: &[ResolutionSegment],
    frame_count: u64,
    fps: f64,
    seed: u64,
    connection_id: &str,
) -> crate::Result<Vec<RenderedFrame>> {
    let mut frames = Vec::with_capacity(frame_count as usize);
    let mut rng = SimpleRng::new(seed);
    let mut runtime = raden::PipelineRuntime::new();

    // フレーム描画の進捗をプログレスバーで表示する (非ターミナルでは自動的に無効化される)
    let mut progress =
        ProgressBar::new(frame_count, ProgressKind::Frame).with_label("drawing frames");

    // 背景色相はシードで決める (0-360)。時間変化はさせない
    let base_hue = (rng.next_u64() % 360) as f64;
    let (br, bg, bb) = hsv_to_rgb(base_hue, 0.4, 0.3);

    // セグメントは開始フレーム順に並んでいるため、進行方向にのみ進める
    let mut segment_index = 0;
    for frame_index in 0..frame_count {
        while segment_index + 1 < segments.len()
            && segments[segment_index + 1].start_frame <= frame_index
        {
            segment_index += 1;
        }
        let segment = &segments[segment_index];
        let width = segment.width;
        let height = segment.height;
        let w = width as f64;
        let h = height as f64;

        // ドットの大きさは画面の縦サイズの 2% を 7 ドット (5x7 の縦) で割った値にする
        let dot_size = ((h * 0.02) / 7.0).round().max(1.0);

        let mut image = raden::Image::new(width as u32, height as u32, raden::PixelFormat::Prgb32);
        let mut ctx = raden::Context::new(&mut image, &mut runtime);

        // 時間 (秒単位)
        let t = frame_index as f64 / fps;

        // --- 背景: 固定色の暗い背景 (SrcCopy) ---
        ctx.set_comp_op(raden::CompOp::SrcCopy);
        ctx.set_fill_style(raden::Rgba32::rgb(br, bg, bb));
        ctx.fill_all();
        ctx.set_comp_op(raden::CompOp::SrcOver);

        // connection_id を左上に白文字で表示する
        // (背景は暗い固定色のため、黒背景なしでも十分な可読性がある)
        draw_pixel_text(
            &mut ctx,
            connection_id,
            dot_size * 6.0,
            dot_size * 6.0,
            dot_size,
            raden::Rgba32::rgb(0xFF, 0xFF, 0xFF),
        );

        // --- ウェーブパターン: 横方向に並ぶ円が sin 波で上下に動く ---
        let num_waves = 5;
        for wave_idx in 0..num_waves {
            let wi = wave_idx as f64;
            let wave_offset = wi * 0.5;
            let wave_amplitude = 50.0 + wi * 20.0;
            let wave_freq = 0.008 + wi * 0.002;
            let wave_speed = 2.0 + wi * 0.3;
            let wave_y_base = h * 0.3 + wi * 80.0;

            let wave_hue = (base_hue + t * 60.0 + wi * 50.0) % 360.0;
            let (wr, wg, wb) = hsv_to_rgb(wave_hue, 0.8, 0.9);
            ctx.set_fill_style(raden::Rgba32::new(wr, wg, wb, 150));

            let mut x = 0.0;
            while x < w {
                let y = wave_y_base
                    + wave_amplitude * (wave_freq * x + t * wave_speed + wave_offset).sin();
                let radius = 8.0 + 4.0 * (t * 3.0 + x * 0.01).sin();
                ctx.fill_circle(&raden::Circle::new(x, y, radius));
                x += 20.0;
            }
        }

        // --- バウンドする円: リサージュ曲線風に動く複数の半透明円 ---
        let num_balls = 8;
        for ball_idx in 0..num_balls {
            let bi = ball_idx as f64;
            let freq_x = 0.5 + bi * 0.15;
            let freq_y = 0.7 + bi * 0.12;
            let phase_x = bi * std::f64::consts::PI / 4.0;
            let phase_y = bi * std::f64::consts::PI / 3.0;

            let bx = w * 0.5 + (w * 0.35) * (t * freq_x + phase_x).sin();
            let by = h * 0.5 + (h * 0.3) * (t * freq_y + phase_y).sin();
            let ball_radius = 30.0 + 15.0 * (t * 4.0 + bi).sin();

            let ball_hue = (base_hue + bi * 45.0 + t * 100.0) % 360.0;
            let (cr, cg, cb) = hsv_to_rgb(ball_hue, 1.0, 1.0);
            ctx.set_fill_style(raden::Rgba32::new(cr, cg, cb, 200));
            ctx.fill_circle(&raden::Circle::new(bx, by, ball_radius));

            // 光沢効果 (小さい白い円、左上寄りに配置)
            ctx.save();
            let hl_radius = ball_radius * 0.3;
            ctx.set_fill_style(raden::Rgba32::new(255, 255, 255, 100));
            ctx.fill_circle(&raden::Circle::new(
                bx - ball_radius * 0.3,
                by - ball_radius * 0.3,
                hl_radius,
            ));
            ctx.restore();
        }

        // --- 回転する円群: 中央周囲を周回する円 ---
        let num_shapes = 6;
        let center_x = w * 0.5;
        let center_y = h * 0.5;
        for shape_idx in 0..num_shapes {
            let si = shape_idx as f64;
            let angle = t * (1.0 + si * 0.2) + si * std::f64::consts::PI / 3.0;
            let dist = 150.0 + 50.0 * (t * 2.0 + si).sin();
            let sx = center_x + dist * angle.cos();
            let sy = center_y + dist * angle.sin();

            let shape_hue = (base_hue + si * 60.0 + t * 80.0) % 360.0;
            let (sr, sg, sb) = hsv_to_rgb(shape_hue, 0.9, 0.95);
            let shape_radius = 25.0 + 15.0 * (t * 3.0 + si).sin();
            ctx.set_fill_style(raden::Rgba32::new(sr, sg, sb, 180));
            ctx.fill_circle(&raden::Circle::new(sx, sy, shape_radius));
        }

        ctx.end();

        // Prgb32 → I420 変換
        let yuv_data = prgb32_to_i420(&image, width, height)?;
        frames.push(RenderedFrame {
            yuv_data,
            width,
            height,
        });

        progress.inc(1);
    }
    progress.finish();

    Ok(frames)
}

/// HSV を RGB に変換する (h: 0-360, s: 0-1, v: 0-1)
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

/// 5x7 ドットマトリクス (0-9, A-Z) のグリフ定義
///
/// 各行は下位 5 ビットが左から右のドットに対応する
/// (例: 'A' の 1 行目 0x0E = 01110 = ███ の中央 3 ドット)。
/// 小文字は大文字に正規化して描画する。
const GLYPHS: [&[u8; 7]; 36] = [
    // '0' - '9'
    &[0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
    &[0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
    &[0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
    &[0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E],
    &[0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
    &[0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
    &[0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
    &[0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
    &[0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
    &[0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
    // A から Z
    &[0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
    &[0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
    &[0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
    &[0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
    &[0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
    &[0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
    &[0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
    &[0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
    &[0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
    &[0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0E],
    &[0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
    &[0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
    &[0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
    &[0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
    &[0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
    &[0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
    &[0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
    &[0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
    &[0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
    &[0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
    &[0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
    &[0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
    &[0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
    &[0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
    &[0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
    &[0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
];

/// 5x7 ドットマトリクスでテキストを描画する
///
/// 1 文字は 5x7 ドット。文字間のギャップは 1 ドット。
/// ドットの大きさ (`dot_size`) はピクセル単位で、縦サイズに応じて拡大縮小される。
fn draw_pixel_text(
    ctx: &mut raden::Context<'_>,
    text: &str,
    origin_x: f64,
    origin_y: f64,
    dot_size: f64,
    color: raden::Rgba32,
) {
    ctx.set_fill_style(color);
    let mut x = origin_x;
    for ch in text.chars() {
        // 小文字は大文字に正規化し、英数字以外はスキップする
        let Some(glyph) = glyph_for(ch) else {
            // 未知の文字は 1 文字分の空白として扱う
            x += 6.0 * dot_size;
            continue;
        };
        for (row, row_bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if row_bits & (1 << (4 - col)) != 0 {
                    ctx.fill_rect(&raden::Rect::new(
                        x + col as f64 * dot_size,
                        origin_y + row as f64 * dot_size,
                        dot_size,
                        dot_size,
                    ));
                }
            }
        }
        x += 6.0 * dot_size;
    }
}

/// 文字を 5x7 グリフに変換する (英数字のみ。小文字は大文字に正規化)
fn glyph_for(ch: char) -> Option<&'static [u8; 7]> {
    let upper = ch.to_ascii_uppercase();
    match upper {
        '0'..='9' => Some(GLYPHS[(upper as u8 - b'0') as usize]),
        'A'..='Z' => Some(GLYPHS[(upper as u8 - b'A' + 10) as usize]),
        _ => None,
    }
}

/// raden の Prgb32 (premultiplied ARGB) を I420 に変換する
///
/// libyuv の `argb_to_i420` は直の ARGB を想定しているため、
/// 先に premultiplied をアンプレマルチプライしてから変換する。
fn prgb32_to_i420(image: &raden::Image, width: usize, height: usize) -> crate::Result<Vec<u8>> {
    let data = image.data();
    let stride = image.stride();

    // premultiplied ARGB (0xAARRGGBB) を直の ARGB に変換する
    //
    // raden の Prgb32 は u32 で 0xAARRGGBB を保持するため、
    // メモリ上 (リトルエンディアン) のバイト順は [B, G, R, A] になる。
    // libyuv の argb_to_i420 も同じバイト順を期待するため、
    // unpremultiply のみを行い、バイト順は入れ替えない。
    let mut argb_data = Vec::with_capacity(data.len());
    for chunk in data.as_chunks::<4>().0 {
        let b = chunk[0];
        let g = chunk[1];
        let r = chunk[2];
        let a = chunk[3];
        if a == 0 {
            argb_data.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let inv = 255.0 / a as f32;
            argb_data.push((b as f32 * inv).round() as u8);
            argb_data.push((g as f32 * inv).round() as u8);
            argb_data.push((r as f32 * inv).round() as u8);
            argb_data.push(a);
        }
    }

    let y_size = width * height;
    let uv_width = width.div_ceil(2);
    let uv_height = height.div_ceil(2);
    let uv_size = uv_width * uv_height;
    let mut yuv_data = vec![0u8; y_size + uv_size * 2];

    let src = shiguredo_libyuv::ArgbImage {
        data: &argb_data,
        stride,
    };
    let (y_plane, rest) = yuv_data.split_at_mut(y_size);
    let (u_plane, v_plane) = rest.split_at_mut(uv_size);
    let mut dst = shiguredo_libyuv::I420ImageMut {
        y: y_plane,
        y_stride: width,
        u: u_plane,
        u_stride: uv_width,
        v: v_plane,
        v_stride: uv_width,
    };
    let size = shiguredo_libyuv::ImageSize::new(width, height);
    shiguredo_libyuv::argb_to_i420(&src, &mut dst, size)
        .map_err(|e| crate::Error::new(format!("argb_to_i420 failed: {e}")))?;
    Ok(yuv_data)
}

/// 指定コーデックでエンコードして MP4 に書き出す
///
/// 解像度が変わるセグメントの境界では、エンコーダーを新しい解像度のものに
/// 乗り換える。新しく作ったエンコーダーの最初のフレームは、どのエンコーダー
/// でも必ずキーフレームになる (libvpx / OpenH264 / VideoToolbox / SVT-AV1 の
/// いずれも、エンコーダー初期化直後の最初のフレームはキーフレームになる) ため、
/// 明示的なキーフレーム強制は行わない。
fn encode_and_write_mp4(
    mp4_path: &Path,
    args: &Args,
    segments: &[ResolutionSegment],
    frames: &[RenderedFrame],
    openh264_lib: Option<Openh264Library>,
) -> crate::Result<()> {
    let fps = args.frame_rate;

    // エンコーダーとライターを構成する
    let input_stream_id = MediaStreamId::new(0);
    let encoder_output_stream_id = MediaStreamId::new(1);
    let writer_input_stream_id = encoder_output_stream_id;

    // エンコーダーの構築 (セグメントごとに width / height が異なる)
    fn create_encoder(
        args: &Args,
        width: usize,
        height: usize,
        input_stream_id: MediaStreamId,
        encoder_output_stream_id: MediaStreamId,
        openh264_lib: Option<Openh264Library>,
    ) -> crate::Result<VideoEncoder> {
        let options = VideoEncoderOptions {
            codec: args.codec,
            engines: None,
            bitrate: 1_000_000,
            width: crate::types::EvenUsize::truncating_new(width),
            height: crate::types::EvenUsize::truncating_new(height),
            frame_rate: args.frame_rate,
            encode_params: LayoutEncodeParams::default(),
        };
        VideoEncoder::new(
            &options,
            input_stream_id,
            encoder_output_stream_id,
            openh264_lib,
        )
    }

    let mut encoder = create_encoder(
        args,
        segments[0].width,
        segments[0].height,
        input_stream_id,
        encoder_output_stream_id,
        openh264_lib.clone(),
    )?;

    let writer_options = Mp4WriterOptions {
        resolution: args.resolution,
        duration: Duration::from_secs(args.duration),
        frame_rate: fps,
    };
    let mut writer = Mp4Writer::new(
        mp4_path,
        &writer_options,
        None,
        Some(writer_input_stream_id),
    )?;

    // 各フレームをエンコードして書き出す
    // (進捗は描画と区別できるようラベルを付けてプログレスバーで表示する)
    let mut progress = ProgressBar::new(frames.len() as u64, ProgressKind::Frame)
        .with_label("encoding and writing MP4");
    let frame_duration = fps.frame_duration();
    let mut segment_index = 0;
    for (i, frame) in frames.iter().enumerate() {
        // セグメントの境界 (最初のフレームを除く) でエンコーダーを乗り換える
        if i > 0
            && segment_index + 1 < segments.len()
            && segments[segment_index + 1].start_frame as usize == i
        {
            segment_index += 1;

            // 旧エンコーダーに EOS を送って残りのフレームをフラッシュする
            encoder.process_input(MediaProcessorInput::eos(input_stream_id))?;
            drain_encoder(&mut encoder, &mut writer)?;

            // 新しい解像度のエンコーダーを構築する
            encoder = create_encoder(
                args,
                segments[segment_index].width,
                segments[segment_index].height,
                input_stream_id,
                encoder_output_stream_id,
                openh264_lib.clone(),
            )?;
        }

        let timestamp = fps.frame_timestamp(i as u64);
        let video_frame = VideoFrame {
            source_id: None,
            data: frame.yuv_data.clone(),
            format: VideoFormat::I420,
            keyframe: i == 0,
            width: frame.width,
            height: frame.height,
            timestamp,
            duration: frame_duration,
            sample_entry: None,
        };

        encoder.process_input(MediaProcessorInput::video_frame(
            input_stream_id,
            video_frame,
        ))?;
        drain_encoder(&mut encoder, &mut writer)?;
        progress.inc(1);
    }
    progress.finish();

    // EOS を送る
    encoder.process_input(MediaProcessorInput::eos(input_stream_id))?;
    drain_encoder(&mut encoder, &mut writer)?;

    // ライターにも EOS を送って finalize させる
    writer.process_input(MediaProcessorInput::eos(writer_input_stream_id))?;
    drain_writer(&mut writer)?;

    Ok(())
}

fn drain_encoder(encoder: &mut VideoEncoder, writer: &mut Mp4Writer) -> crate::Result<()> {
    loop {
        match encoder.process_output()? {
            MediaProcessorOutput::Processed { stream_id, sample } => {
                writer.process_input(MediaProcessorInput::sample(stream_id, sample))?;
            }
            MediaProcessorOutput::Pending { .. } => return Ok(()),
            MediaProcessorOutput::Finished => return Ok(()),
        }
    }
}

fn drain_writer(writer: &mut Mp4Writer) -> crate::Result<()> {
    loop {
        match writer.process_output()? {
            MediaProcessorOutput::Processed { .. } => {}
            MediaProcessorOutput::Pending { .. } => return Ok(()),
            MediaProcessorOutput::Finished => return Ok(()),
        }
    }
}

/// archive JSON を書き出す
fn write_archive_json(args: &Args) -> crate::Result<()> {
    let stop_time_offset = args.start_time + args.duration;
    let json = format!(
        r#"{{
  "connection_id": "{}",
  "format": "mp4",
  "audio": false,
  "video": true,
  "start_time_offset": {},
  "stop_time_offset": {}
}}
"#,
        args.connection_id, args.start_time, stop_time_offset
    );
    let path = args
        .output_dir
        .join(format!("archive-{}.json", args.connection_id));
    fs::write(path, json).map_err(|e| crate::Error::new(format!("failed to write JSON: {e}")))
}

/// シンプルな疑似乱数生成器 (Xorshift)
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x9E3779B97F4A7C15 } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}
