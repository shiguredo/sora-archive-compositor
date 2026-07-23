use std::{collections::VecDeque, path::PathBuf, time::Duration};

use crate::{
    decoder::{AudioDecoder, VideoDecoder, VideoDecoderOptions},
    media::MediaStreamId,
    metadata::{ContainerFormat, SourceId},
    processor::{
        MediaProcessor, MediaProcessorInput, MediaProcessorOutput, MediaProcessorSpec,
        MediaProcessorWorkloadHint,
    },
    reader::{AudioReader, VideoReader},
    scheduler::Scheduler,
    stats::ProcessorStats,
    types::CodecName,
    video::{VideoFormat, VideoFrame},
    video_h264::H264AnnexBNalUnits,
};

use shiguredo_openh264::Openh264Library;

const AUDIO_ENCODED_STREAM_ID: MediaStreamId = MediaStreamId::new(0);
const VIDEO_ENCODED_STREAM_ID: MediaStreamId = MediaStreamId::new(1);
const AUDIO_DECODED_STREAM_ID: MediaStreamId = MediaStreamId::new(2);
const VIDEO_DECODED_STREAM_ID: MediaStreamId = MediaStreamId::new(3);

pub fn run(mut args: noargs::RawArgs) -> noargs::Result<()> {
    let decode: bool = noargs::flag("decode")
        .doc("指定された場合にはデコードまで行います")
        .take(&mut args)
        .is_present();
    let openh264: Option<PathBuf> = noargs::opt("openh264")
        .ty("PATH")
        .env("SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH")
        .doc("OpenH264 の共有ライブラリのパス")
        .take(&mut args)
        .present_and_then(|a| a.value().parse())?;
    let input_file_path: PathBuf = noargs::arg("INPUT_FILE")
        .example("/path/to/archive.mp4")
        .doc("情報取得対象の録画ファイル(.mp4|.webm)")
        .take(&mut args)
        .then(|a| a.value().parse())?;
    if let Some(help) = args.finish()? {
        print!("{help}");
        return Ok(());
    }

    let format = match input_file_path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .as_ref()
    {
        "mp4" => ContainerFormat::Mp4,
        "webm" => ContainerFormat::Webm,
        ext => {
            return Err(crate::Error::new(format!("unsupported container format: {ext}")).into());
        }
    };

    let mut scheduler = Scheduler::new();
    let dummy_source_id = SourceId::new("inspect"); // 使われないのでなんでもいい

    let reader = AudioReader::new(
        AUDIO_ENCODED_STREAM_ID,
        dummy_source_id.clone(),
        format,
        Duration::ZERO,
        vec![input_file_path.clone()],
    )?;
    scheduler.register(reader)?;

    let reader = VideoReader::new(
        VIDEO_ENCODED_STREAM_ID,
        dummy_source_id.clone(),
        format,
        Duration::ZERO,
        vec![input_file_path.clone()],
    )?;
    scheduler.register(reader)?;

    if decode {
        let decoder = AudioDecoder::new_opus(AUDIO_ENCODED_STREAM_ID, AUDIO_DECODED_STREAM_ID)?;
        scheduler.register(decoder)?;
    }

    if decode {
        let options = VideoDecoderOptions {
            openh264_lib: openh264.clone().map(Openh264Library::load).transpose()?,
            decode_params: Default::default(),
            engines: None,
        };
        let decoder = VideoDecoder::new(VIDEO_ENCODED_STREAM_ID, VIDEO_DECODED_STREAM_ID, options);
        scheduler.register(decoder)?;
    }

    scheduler.register(OutputPrinter::new(input_file_path.clone(), format, decode))?;
    scheduler.run()?;

    Ok(())
}

#[derive(Debug)]
struct AudioSampleInfo {
    timestamp: Duration,
    duration: Duration,
    data_size: usize,
    decoded_data_size: Option<usize>,
}

impl nojson::DisplayJson for AudioSampleInfo {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.set_indent_size(0);
        f.object(|f| {
            f.member("timestamp_us", self.timestamp.as_micros())?;
            f.member("duration_us", self.duration.as_micros())?;
            f.member("data_size", self.data_size)?;
            if let Some(v) = self.decoded_data_size {
                f.member("decoded_data_size", v)?;
            }
            Ok(())
        })?;
        f.set_indent_size(2);
        Ok(())
    }
}

#[derive(Debug)]
struct VideoSampleInfo {
    timestamp: Duration,
    duration: Duration,
    data_size: usize,
    keyframe: bool,
    codec_specific_info: Option<VideoCodecSpecificInfo>,
    decoded_data_size: Option<usize>,
    width: Option<usize>,
    height: Option<usize>,
}

impl VideoSampleInfo {
    fn apply_decoded(&mut self, decoded: DecodedVideoInfo) {
        self.decoded_data_size = Some(decoded.decoded_data_size);
        self.width = Some(decoded.width);
        self.height = Some(decoded.height);
    }
}

/// 符号化サンプルより先に届いたデコード結果を一時保持するための情報
#[derive(Debug)]
struct DecodedVideoInfo {
    decoded_data_size: usize,
    width: usize,
    height: usize,
}

impl nojson::DisplayJson for VideoSampleInfo {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.set_indent_size(0);
        f.object(|f| {
            f.member("timestamp_us", self.timestamp.as_micros())?;
            f.member("duration_us", self.duration.as_micros())?;
            f.member("data_size", self.data_size)?;
            f.member("keyframe", self.keyframe)?;
            match &self.codec_specific_info {
                None => {}
                Some(VideoCodecSpecificInfo::H264 { nalus }) => {
                    f.member("nalus", nalus)?;
                }
            }
            if let Some(v) = self.decoded_data_size {
                f.member("decoded_data_size", v)?;
            }
            if let Some(v) = self.width {
                f.member("width", v)?;
            }
            if let Some(v) = self.height {
                f.member("height", v)?;
            }
            Ok(())
        })?;
        f.set_indent_size(2);
        Ok(())
    }
}

#[derive(Debug)]
struct H264NalUnitInfo {
    ty: u8,
    nri: u8,
}

impl nojson::DisplayJson for H264NalUnitInfo {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            f.member("type", self.ty)?;
            f.member("nri", self.nri)
        })
    }
}

#[derive(Debug)]
enum VideoCodecSpecificInfo {
    H264 { nalus: Vec<H264NalUnitInfo> },
}

impl VideoCodecSpecificInfo {
    fn new(sample: &VideoFrame) -> Option<Self> {
        match sample.format {
            VideoFormat::H264AnnexB => {
                let mut nalus = Vec::new();
                for nalu in H264AnnexBNalUnits::new(&sample.data) {
                    match nalu {
                        Ok(nalu) => {
                            let header_byte = nalu.data.first()?;
                            let nri = (header_byte >> 5) & 0b11;
                            nalus.push(H264NalUnitInfo { ty: nalu.ty, nri });
                        }
                        Err(_) => return None, // パースエラー
                    }
                }

                Some(VideoCodecSpecificInfo::H264 { nalus })
            }
            VideoFormat::H264 => {
                let mut nalus = Vec::new();
                let mut data = &sample.data[..];

                // NOTE: sora の場合は区切りバイトサイズは 4 に固定
                while data.len() > 4 {
                    let length = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
                    data = &data[4..];

                    if data.len() < length || length == 0 {
                        return None; // パースエラー
                    }

                    let header_byte = data[0];
                    let nalu_type = header_byte & 0b0001_1111;
                    let nri = (header_byte >> 5) & 0b11;

                    nalus.push(H264NalUnitInfo { ty: nalu_type, nri });

                    data = &data[length..];
                }

                Some(VideoCodecSpecificInfo::H264 { nalus })
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct OutputPrinter {
    path: PathBuf,
    format: ContainerFormat,
    /// `--decode` 指定時のみ終了時に付与漏れを検査する
    decode: bool,
    audio_codec: Option<CodecName>,
    video_codec: Option<CodecName>,
    audio_samples: Vec<AudioSampleInfo>,
    video_samples: Vec<VideoSampleInfo>,
    /// 符号化サンプルより先に届いた音声デコード結果のサイズ
    pending_audio_decoded_data_sizes: VecDeque<usize>,
    /// 符号化サンプルより先に届いた映像デコード結果
    pending_video_decoded_infos: VecDeque<DecodedVideoInfo>,
    input_stream_ids: Vec<MediaStreamId>,
    next_input_stream_index: usize,
}

impl OutputPrinter {
    fn new(path: PathBuf, format: ContainerFormat, decode: bool) -> Self {
        Self {
            path,
            format,
            decode,
            audio_codec: None,
            video_codec: None,
            audio_samples: Vec::new(),
            video_samples: Vec::new(),
            pending_audio_decoded_data_sizes: VecDeque::new(),
            pending_video_decoded_infos: VecDeque::new(),
            input_stream_ids: if decode {
                vec![
                    AUDIO_ENCODED_STREAM_ID,
                    VIDEO_ENCODED_STREAM_ID,
                    AUDIO_DECODED_STREAM_ID,
                    VIDEO_DECODED_STREAM_ID,
                ]
            } else {
                vec![AUDIO_ENCODED_STREAM_ID, VIDEO_ENCODED_STREAM_ID]
            },
            next_input_stream_index: 0,
        }
    }

    /// 先頭の未デコード音声へデコードサイズを付ける。付け先が無ければキューへ積む。
    fn attach_or_queue_audio_decoded(&mut self, decoded_data_size: usize) {
        if let Some(info) = self
            .audio_samples
            .iter_mut()
            .find(|s| s.decoded_data_size.is_none())
        {
            info.decoded_data_size = Some(decoded_data_size);
        } else {
            self.pending_audio_decoded_data_sizes
                .push_back(decoded_data_size);
        }
    }

    /// キューに残っている音声デコード結果を、先頭の未デコードへ FIFO で適用する。
    fn try_apply_pending_audio_decoded(&mut self) {
        while let Some(decoded_data_size) = self.pending_audio_decoded_data_sizes.pop_front() {
            let Some(info) = self
                .audio_samples
                .iter_mut()
                .find(|s| s.decoded_data_size.is_none())
            else {
                self.pending_audio_decoded_data_sizes
                    .push_front(decoded_data_size);
                break;
            };
            info.decoded_data_size = Some(decoded_data_size);
        }
    }

    /// 先頭の未デコード映像へデコード結果を付ける。付け先が無ければキューへ積む。
    fn attach_or_queue_video_decoded(&mut self, decoded: DecodedVideoInfo) {
        if let Some(info) = self
            .video_samples
            .iter_mut()
            .find(|s| s.decoded_data_size.is_none())
        {
            info.apply_decoded(decoded);
        } else {
            self.pending_video_decoded_infos.push_back(decoded);
        }
    }

    /// キューに残っている映像デコード結果を、先頭の未デコードへ FIFO で適用する。
    fn try_apply_pending_video_decoded(&mut self) {
        while let Some(decoded) = self.pending_video_decoded_infos.pop_front() {
            let Some(info) = self
                .video_samples
                .iter_mut()
                .find(|s| s.decoded_data_size.is_none())
            else {
                self.pending_video_decoded_infos.push_front(decoded);
                break;
            };
            info.apply_decoded(decoded);
        }
    }

    /// `--decode` 時、符号化とデコードの対応付けが完了していることを確認する。
    fn ensure_decode_attribution_complete(&self) -> crate::Result<()> {
        if !self.decode {
            return Ok(());
        }
        if self
            .audio_samples
            .iter()
            .any(|s| s.decoded_data_size.is_none())
        {
            return Err(crate::Error::new(
                "undecoded audio sample remains after inspect --decode",
            ));
        }
        if self
            .video_samples
            .iter()
            .any(|s| s.decoded_data_size.is_none())
        {
            return Err(crate::Error::new(
                "undecoded video sample remains after inspect --decode",
            ));
        }
        if !self.pending_audio_decoded_data_sizes.is_empty() {
            return Err(crate::Error::new(
                "unapplied decoded audio result remains after inspect --decode",
            ));
        }
        if !self.pending_video_decoded_infos.is_empty() {
            return Err(crate::Error::new(
                "unapplied decoded video result remains after inspect --decode",
            ));
        }
        Ok(())
    }
}

impl MediaProcessor for OutputPrinter {
    fn spec(&self) -> MediaProcessorSpec {
        MediaProcessorSpec {
            input_stream_ids: self.input_stream_ids.clone(),
            output_stream_ids: Vec::new(),
            stats: ProcessorStats::other("output_printer"),
            workload_hint: MediaProcessorWorkloadHint::WRITER,
        }
    }

    fn process_input(&mut self, input: MediaProcessorInput) -> crate::Result<()> {
        let Some(sample) = input.sample else {
            self.input_stream_ids.retain(|id| *id != input.stream_id);
            self.next_input_stream_index = 0;
            return Ok(());
        };
        match input.stream_id {
            AUDIO_ENCODED_STREAM_ID => {
                let sample = sample.expect_audio_data()?;
                if self.audio_codec.is_none() {
                    self.audio_codec = sample.format.codec_name();
                }
                self.audio_samples.push(AudioSampleInfo {
                    timestamp: sample.timestamp,
                    duration: sample.duration,
                    data_size: sample.data.len(),
                    decoded_data_size: None,
                });
                self.try_apply_pending_audio_decoded();
            }
            AUDIO_DECODED_STREAM_ID => {
                let sample = sample.expect_audio_data()?;
                self.attach_or_queue_audio_decoded(sample.data.len());
            }
            VIDEO_ENCODED_STREAM_ID => {
                let sample = sample.expect_video_frame()?;
                if self.video_codec.is_none() {
                    self.video_codec = sample.format.codec_name();
                }
                self.video_samples.push(VideoSampleInfo {
                    timestamp: sample.timestamp,
                    duration: sample.duration,
                    data_size: sample.data.len(),
                    keyframe: sample.keyframe,
                    codec_specific_info: VideoCodecSpecificInfo::new(&sample),
                    decoded_data_size: None,
                    width: None,
                    height: None,
                });
                self.try_apply_pending_video_decoded();
            }
            VIDEO_DECODED_STREAM_ID => {
                let sample = sample.expect_video_frame()?;
                self.attach_or_queue_video_decoded(DecodedVideoInfo {
                    decoded_data_size: sample.data.len(),
                    width: sample.width,
                    height: sample.height,
                });
            }
            _ => return Err(crate::Error::new("BUG: unexpected stream ID")),
        }
        Ok(())
    }

    fn process_output(&mut self) -> crate::Result<MediaProcessorOutput> {
        if self.input_stream_ids.is_empty() {
            self.ensure_decode_attribution_complete()?;
            crate::json::pretty_print(self)?;
            Ok(MediaProcessorOutput::Finished)
        } else {
            let awaiting_stream_id = self.input_stream_ids[self.next_input_stream_index];
            self.next_input_stream_index =
                (self.next_input_stream_index + 1) % self.input_stream_ids.len();
            Ok(MediaProcessorOutput::pending(awaiting_stream_id))
        }
    }
}

impl nojson::DisplayJson for OutputPrinter {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            f.member("path", &self.path)?;
            f.member("format", self.format)?;
            if let Some(c) = self.audio_codec {
                f.member("audio_codec", c)?;
                f.member(
                    "audio_duration_us",
                    self.audio_samples
                        .iter()
                        .map(|s| s.duration)
                        .sum::<Duration>()
                        .as_micros(),
                )?;
                f.member("audio_sample_count", self.audio_samples.len())?;
                f.member("audio_samples", &self.audio_samples)?;
            }
            if let Some(c) = self.video_codec {
                f.member("video_codec", c)?;
                f.member(
                    "video_duration_us",
                    self.video_samples
                        .iter()
                        .map(|s| s.duration)
                        .sum::<Duration>()
                        .as_micros(),
                )?;
                f.member("video_sample_count", self.video_samples.len())?;
                f.member(
                    "video_keyframe_sample_count",
                    self.video_samples.iter().filter(|s| s.keyframe).count(),
                )?;
                f.member("video_samples", &self.video_samples)?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioData, AudioFormat};

    fn printer_for_decode() -> OutputPrinter {
        OutputPrinter::new(PathBuf::from("test.mp4"), ContainerFormat::Mp4, true)
    }

    fn encoded_audio(data_size: usize, timestamp_ms: u64) -> MediaProcessorInput {
        MediaProcessorInput::audio_data(
            AUDIO_ENCODED_STREAM_ID,
            AudioData {
                source_id: None,
                data: vec![0; data_size],
                format: AudioFormat::Opus,
                stereo: true,
                sample_rate: 48000,
                timestamp: Duration::from_millis(timestamp_ms),
                duration: Duration::from_millis(20),
                sample_entry: None,
            },
        )
    }

    fn decoded_audio(data_size: usize, timestamp_ms: u64) -> MediaProcessorInput {
        MediaProcessorInput::audio_data(
            AUDIO_DECODED_STREAM_ID,
            AudioData {
                source_id: None,
                data: vec![0; data_size],
                format: AudioFormat::I16Be,
                stereo: true,
                sample_rate: 48000,
                timestamp: Duration::from_millis(timestamp_ms),
                duration: Duration::from_millis(20),
                sample_entry: None,
            },
        )
    }

    fn encoded_video(data_size: usize, timestamp_ms: u64) -> MediaProcessorInput {
        MediaProcessorInput::video_frame(
            VIDEO_ENCODED_STREAM_ID,
            VideoFrame {
                source_id: None,
                data: vec![0; data_size],
                format: VideoFormat::Vp9,
                keyframe: true,
                width: 320,
                height: 240,
                timestamp: Duration::from_millis(timestamp_ms),
                duration: Duration::from_millis(33),
                sample_entry: None,
            },
        )
    }

    fn decoded_video(
        data_size: usize,
        width: usize,
        height: usize,
        timestamp_ms: u64,
    ) -> MediaProcessorInput {
        MediaProcessorInput::video_frame(
            VIDEO_DECODED_STREAM_ID,
            VideoFrame {
                source_id: None,
                data: vec![0; data_size],
                format: VideoFormat::I420,
                keyframe: true,
                width,
                height,
                timestamp: Duration::from_millis(timestamp_ms),
                duration: Duration::from_millis(33),
                sample_entry: None,
            },
        )
    }

    fn finish_all_streams(printer: &mut OutputPrinter) -> crate::Result<()> {
        for stream_id in [
            AUDIO_ENCODED_STREAM_ID,
            VIDEO_ENCODED_STREAM_ID,
            AUDIO_DECODED_STREAM_ID,
            VIDEO_DECODED_STREAM_ID,
        ] {
            printer.process_input(MediaProcessorInput::eos(stream_id))?;
        }
        // pretty_print を避けるため、終了検査だけ直接呼ぶ
        printer.ensure_decode_attribution_complete()
    }

    /// 符号化が先に複数積まれたあとデコード結果が来るとき、FIFO で先頭から付与されること。
    /// 旧実装の rfind (LIFO) だと末尾に付くため、このケースで誤対応になる。
    #[test]
    fn attaches_delayed_decoded_results_in_fifo_order() {
        let mut printer = printer_for_decode();

        printer
            .process_input(encoded_video(10, 0))
            .expect("符号化 1 件目の入力に失敗した");
        printer
            .process_input(encoded_video(20, 33))
            .expect("符号化 2 件目の入力に失敗した");
        printer
            .process_input(decoded_video(100, 320, 240, 0))
            .expect("デコード 1 件目の入力に失敗した");
        printer
            .process_input(decoded_video(200, 160, 120, 33))
            .expect("デコード 2 件目の入力に失敗した");

        assert_eq!(printer.video_samples[0].decoded_data_size, Some(100));
        assert_eq!(printer.video_samples[0].width, Some(320));
        assert_eq!(printer.video_samples[0].height, Some(240));
        assert_eq!(printer.video_samples[1].decoded_data_size, Some(200));
        assert_eq!(printer.video_samples[1].width, Some(160));
        assert_eq!(printer.video_samples[1].height, Some(120));

        assert!(
            finish_all_streams(&mut printer).is_ok(),
            "付与完了後の終了検査に失敗した"
        );
    }

    /// デコード結果が符号化サンプルより先に届いても、キュー経由で後から FIFO 付与されること。
    #[test]
    fn queues_decoded_results_that_arrive_before_encoded_samples() {
        let mut printer = printer_for_decode();

        printer
            .process_input(decoded_audio(1000, 0))
            .expect("先着デコードの入力に失敗した");
        assert!(printer.audio_samples.is_empty());
        assert_eq!(printer.pending_audio_decoded_data_sizes.len(), 1);

        printer
            .process_input(encoded_audio(50, 0))
            .expect("後続の符号化入力に失敗した");
        assert_eq!(printer.audio_samples[0].decoded_data_size, Some(1000));
        assert!(printer.pending_audio_decoded_data_sizes.is_empty());

        assert!(
            finish_all_streams(&mut printer).is_ok(),
            "付与完了後の終了検査に失敗した"
        );
    }

    /// デコード結果が足りないまま終了するとエラーになること。
    #[test]
    fn errors_when_encoded_sample_remains_undecoded_at_finish() {
        let mut printer = printer_for_decode();
        printer
            .process_input(encoded_video(10, 0))
            .expect("符号化入力に失敗した");

        let err = finish_all_streams(&mut printer).expect_err("未付与のまま終了できてしまった");
        assert!(
            err.reason.contains("undecoded video sample remains"),
            "予期しないエラー: {}",
            err.reason
        );
    }

    /// 符号化が来ないままデコード結果だけ残るとエラーになること。
    #[test]
    fn errors_when_unapplied_decoded_result_remains_at_finish() {
        let mut printer = printer_for_decode();
        printer
            .process_input(decoded_audio(1000, 0))
            .expect("先着デコードの入力に失敗した");

        let err = finish_all_streams(&mut printer).expect_err("未適用のまま終了できてしまった");
        assert!(
            err.reason
                .contains("unapplied decoded audio result remains"),
            "予期しないエラー: {}",
            err.reason
        );
    }
}
