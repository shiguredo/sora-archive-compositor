use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::Arc;

use shiguredo_openh264::Openh264Library;

#[cfg(target_os = "macos")]
use crate::encoder_audio_toolbox::AudioToolboxEncoder;
#[cfg(feature = "fdk-aac")]
use crate::encoder_fdk_aac::FdkAacEncoder;
use crate::encoder_libvpx::LibvpxEncoder;
#[cfg(feature = "nvcodec")]
use crate::encoder_nvcodec::NvcodecEncoder;
#[cfg(target_os = "macos")]
use crate::encoder_video_toolbox::VideoToolboxEncoder;
use crate::{
    audio::AudioData,
    encoder_openh264::Openh264Encoder,
    encoder_opus::OpusEncoder,
    encoder_svt_av1::SvtAv1Encoder,
    layout::Layout,
    layout_encode_params::LayoutEncodeParams,
    media::{MediaSample, MediaStreamId},
    processor::{
        MediaProcessor, MediaProcessorInput, MediaProcessorOutput, MediaProcessorSpec,
        MediaProcessorWorkloadHint,
    },
    stats::{AudioEncoderStats, ProcessorStats, VideoEncoderStats},
    types::{CodecName, EngineName, EvenUsize, VideoCodecDirection},
    video::{FrameRate, VideoFrame},
};

#[derive(Debug)]
pub struct AudioEncoder {
    input_stream_id: MediaStreamId,
    output_stream_id: MediaStreamId,
    stats: AudioEncoderStats,
    encoded: VecDeque<AudioData>,
    eos: bool,
    inner: AudioEncoderInner,
}

impl AudioEncoder {
    pub fn new(
        codec: CodecName,
        bitrate: NonZeroUsize,
        input_stream_id: MediaStreamId,
        output_stream_id: MediaStreamId,
        #[cfg(feature = "fdk-aac")] fdk_aac_lib: Option<shiguredo_fdk_aac::FdkAacLibrary>,
    ) -> crate::Result<Self> {
        match codec {
            CodecName::Aac => {
                // feature 有効時は実行時にロードしたライブラリがあれば FDK-AAC を使う。
                // パス未指定ならシステムデフォルトは試行せず、macOS は Audio Toolbox、
                // それ以外はエラーにする（hisui と同じ）。
                #[cfg(feature = "fdk-aac")]
                if let Some(lib) = fdk_aac_lib {
                    return AudioEncoder::new_fdk_aac(
                        input_stream_id,
                        output_stream_id,
                        bitrate,
                        lib,
                    );
                }

                #[cfg(target_os = "macos")]
                {
                    AudioEncoder::new_audio_toolbox_aac(input_stream_id, output_stream_id, bitrate)
                }

                #[cfg(not(target_os = "macos"))]
                {
                    Err(crate::Error::new(
                        "AAC encoding requires FDK-AAC library. \
                         Please specify the library path using --fdk-aac command line argument or \
                         SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH environment variable.",
                    ))
                }
            }
            CodecName::Opus => AudioEncoder::new_opus(input_stream_id, output_stream_id, bitrate),
            _ => unreachable!(),
        }
    }

    fn new_opus(
        input_stream_id: MediaStreamId,
        output_stream_id: MediaStreamId,
        bitrate: NonZeroUsize,
    ) -> crate::Result<Self> {
        let stats = AudioEncoderStats::new(EngineName::Opus, CodecName::Opus);
        Ok(Self {
            input_stream_id,
            output_stream_id,
            stats,
            encoded: VecDeque::new(),
            eos: false,
            inner: AudioEncoderInner::new_opus(bitrate)?,
        })
    }

    #[cfg(feature = "fdk-aac")]
    fn new_fdk_aac(
        input_stream_id: MediaStreamId,
        output_stream_id: MediaStreamId,
        bitrate: NonZeroUsize,
        lib: shiguredo_fdk_aac::FdkAacLibrary,
    ) -> crate::Result<Self> {
        let stats = AudioEncoderStats::new(EngineName::FdkAac, CodecName::Aac);
        Ok(Self {
            input_stream_id,
            output_stream_id,
            stats,
            encoded: VecDeque::new(),
            eos: false,
            inner: AudioEncoderInner::new_fdk_aac(lib, bitrate)?,
        })
    }

    #[cfg(target_os = "macos")]
    fn new_audio_toolbox_aac(
        input_stream_id: MediaStreamId,
        output_stream_id: MediaStreamId,
        bitrate: NonZeroUsize,
    ) -> crate::Result<Self> {
        let stats = AudioEncoderStats::new(EngineName::AudioToolbox, CodecName::Aac);
        Ok(Self {
            input_stream_id,
            output_stream_id,
            stats,
            encoded: VecDeque::new(),
            eos: false,
            inner: AudioEncoderInner::new_audio_toolbox_aac(bitrate)?,
        })
    }

    pub fn name(&self) -> EngineName {
        match &self.inner {
            #[cfg(feature = "fdk-aac")]
            AudioEncoderInner::FdkAac(_) => EngineName::FdkAac,
            #[cfg(target_os = "macos")]
            AudioEncoderInner::AudioToolbox(_) => EngineName::AudioToolbox,
            AudioEncoderInner::Opus(_) => EngineName::Opus,
        }
    }

    pub fn codec(&self) -> CodecName {
        match &self.inner {
            #[cfg(feature = "fdk-aac")]
            AudioEncoderInner::FdkAac(_) => CodecName::Aac,
            #[cfg(target_os = "macos")]
            AudioEncoderInner::AudioToolbox(_) => CodecName::Aac,
            AudioEncoderInner::Opus(_) => CodecName::Opus,
        }
    }

    pub fn get_engines(codec: CodecName, is_fdk_aac_available: bool) -> Vec<EngineName> {
        let mut engines = Vec::new();
        match codec {
            CodecName::Aac => {
                // FDK-AAC は feature 有効かつ共有ライブラリをロードできたときだけ候補に入れる
                if is_fdk_aac_available {
                    engines.push(EngineName::FdkAac);
                }
                #[cfg(target_os = "macos")]
                {
                    engines.push(EngineName::AudioToolbox);
                }
            }
            CodecName::Opus => engines.push(EngineName::Opus),
            _ => unreachable!(),
        }
        engines
    }
}

impl MediaProcessor for AudioEncoder {
    fn spec(&self) -> MediaProcessorSpec {
        MediaProcessorSpec {
            input_stream_ids: vec![self.input_stream_id],
            output_stream_ids: vec![self.output_stream_id],
            stats: ProcessorStats::AudioEncoder(self.stats.clone()),
            workload_hint: MediaProcessorWorkloadHint::AUDIO_ENCODER,
        }
    }

    fn process_input(&mut self, input: MediaProcessorInput) -> crate::Result<()> {
        let encoded = if let Some(sample) = input.sample {
            let data = sample.expect_audio_data()?;
            self.inner.encode(&data)?
        } else {
            self.eos = true;
            self.inner.finish()?
        };

        if let Some(encoded) = encoded {
            self.stats.total_audio_data_count.add(1);
            self.encoded.push_back(encoded);
        }
        Ok(())
    }

    fn process_output(&mut self) -> crate::Result<MediaProcessorOutput> {
        if let Some(data) = self.encoded.pop_front() {
            Ok(MediaProcessorOutput::Processed {
                stream_id: self.output_stream_id,
                sample: MediaSample::audio_data(data),
            })
        } else if self.eos {
            Ok(MediaProcessorOutput::Finished)
        } else {
            Ok(MediaProcessorOutput::Pending {
                awaiting_stream_id: Some(self.input_stream_id),
            })
        }
    }
}

#[derive(Debug)]
enum AudioEncoderInner {
    #[cfg(feature = "fdk-aac")]
    FdkAac(FdkAacEncoder),
    #[cfg(target_os = "macos")]
    AudioToolbox(AudioToolboxEncoder),
    Opus(OpusEncoder),
}

impl AudioEncoderInner {
    fn new_opus(bitrate: NonZeroUsize) -> crate::Result<Self> {
        OpusEncoder::new(bitrate).map(Self::Opus)
    }

    #[cfg(feature = "fdk-aac")]
    fn new_fdk_aac(
        lib: shiguredo_fdk_aac::FdkAacLibrary,
        bitrate: NonZeroUsize,
    ) -> crate::Result<Self> {
        FdkAacEncoder::new(lib, bitrate).map(Self::FdkAac)
    }

    #[cfg(target_os = "macos")]
    fn new_audio_toolbox_aac(bitrate: NonZeroUsize) -> crate::Result<Self> {
        AudioToolboxEncoder::new(bitrate).map(Self::AudioToolbox)
    }

    fn encode(&mut self, data: &AudioData) -> crate::Result<Option<AudioData>> {
        match self {
            #[cfg(feature = "fdk-aac")]
            Self::FdkAac(encoder) => encoder.encode(data),
            #[cfg(target_os = "macos")]
            Self::AudioToolbox(encoder) => encoder.encode(data),
            Self::Opus(encoder) => encoder.encode(data).map(Some),
        }
    }

    fn finish(&mut self) -> crate::Result<Option<AudioData>> {
        match self {
            #[cfg(feature = "fdk-aac")]
            Self::FdkAac(encoder) => encoder.finish(),
            #[cfg(target_os = "macos")]
            Self::AudioToolbox(encoder) => encoder.finish(),
            Self::Opus(_encoder) => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VideoEncoderOptions {
    pub codec: CodecName,
    pub engines: Option<Vec<EngineName>>,
    pub bitrate: usize,
    pub width: EvenUsize,
    pub height: EvenUsize,
    pub frame_rate: FrameRate,
    pub encode_params: LayoutEncodeParams,
}

impl VideoEncoderOptions {
    pub fn from_layout(layout: &Layout) -> Self {
        Self {
            codec: layout.video_codec,
            engines: layout.video_encode_engines.clone(),
            bitrate: layout.video_bitrate_bps(),
            width: layout.resolution.width(),
            height: layout.resolution.height(),
            frame_rate: layout.frame_rate,
            encode_params: layout.encode_params.clone(),
        }
    }
}

#[derive(Debug)]
pub struct VideoEncoder {
    input_stream_id: MediaStreamId,
    output_stream_id: MediaStreamId,
    stats: VideoEncoderStats,
    encoded: VecDeque<VideoFrame>,
    eos: bool,
    inner: VideoEncoderInner,
}

impl VideoEncoder {
    pub fn new(
        options: &VideoEncoderOptions,
        input_stream_id: MediaStreamId,
        output_stream_id: MediaStreamId,
        openh264_lib: Option<Openh264Library>,
    ) -> crate::Result<Self> {
        let candidate_engines = options
            .engines
            .clone()
            .unwrap_or_else(|| VideoEncoder::get_engines(options.codec, openh264_lib.is_some()));
        let engine = candidate_engines
            .iter()
            .find(|engine| engine.is_available_video_encode_codec(options.codec))
            .copied();
        let inner = match (engine, options.codec) {
            (Some(EngineName::Libvpx), CodecName::Vp8) => VideoEncoderInner::new_vp8(options)?,
            (Some(EngineName::Libvpx), CodecName::Vp9) => VideoEncoderInner::new_vp9(options)?,
            #[cfg(feature = "nvcodec")]
            (Some(EngineName::Nvcodec), CodecName::H264) => {
                VideoEncoderInner::new_nvcodec_h264(options)?
            }
            #[cfg(feature = "nvcodec")]
            (Some(EngineName::Nvcodec), CodecName::H265) => {
                VideoEncoderInner::new_nvcodec_h265(options)?
            }
            #[cfg(feature = "nvcodec")]
            (Some(EngineName::Nvcodec), CodecName::Av1) => {
                VideoEncoderInner::new_nvcodec_av1(options)?
            }
            #[cfg(target_os = "macos")]
            (Some(EngineName::VideoToolbox), CodecName::H264) => {
                VideoEncoderInner::new_video_toolbox_h264(options)?
            }
            #[cfg(target_os = "macos")]
            (Some(EngineName::VideoToolbox), CodecName::H265) => {
                VideoEncoderInner::new_video_toolbox_h265(options)?
            }
            (Some(EngineName::Openh264), CodecName::H264) => {
                let lib = openh264_lib.ok_or_else(|| crate::Error::new(concat!(
                        "OpenH264 library is required for H.264 encoding. ",
                        "Please specify the library path using --openh264 command line argument or ",
                        "SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH environment variable.").to_owned()))?;
                VideoEncoderInner::new_openh264(lib, options)?
            }
            (Some(EngineName::SvtAv1), CodecName::Av1) => VideoEncoderInner::new_svt_av1(options)?,
            _ => {
                return Err(crate::Error::new(format!(
                    "no available encoder for {} codec (candidate encoders: {})",
                    options.codec.as_str(),
                    candidate_engines
                        .iter()
                        .map(|engine| engine.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        };

        let stats = VideoEncoderStats::new(inner.name(), inner.codec());

        Ok(Self {
            input_stream_id,
            output_stream_id,
            stats,
            encoded: VecDeque::new(),
            eos: false,
            inner,
        })
    }

    pub fn name(&self) -> EngineName {
        self.inner.name()
    }

    pub fn codec(&self) -> CodecName {
        self.inner.codec()
    }

    pub fn get_engines(codec: CodecName, is_openh264_available: bool) -> Vec<EngineName> {
        EngineName::video_candidate_engines(
            codec,
            VideoCodecDirection::Encode,
            is_openh264_available,
        )
    }

    pub fn encoder_stats(&self) -> &VideoEncoderStats {
        &self.stats
    }
}

impl MediaProcessor for VideoEncoder {
    fn spec(&self) -> MediaProcessorSpec {
        MediaProcessorSpec {
            input_stream_ids: vec![self.input_stream_id],
            output_stream_ids: vec![self.output_stream_id],
            stats: ProcessorStats::VideoEncoder(self.stats.clone()),
            workload_hint: MediaProcessorWorkloadHint::VIDEO_ENCODER,
        }
    }

    fn process_input(&mut self, input: MediaProcessorInput) -> crate::Result<()> {
        if let Some(sample) = input.sample {
            let frame = sample.expect_video_frame()?;
            self.stats.total_input_video_frame_count.add(1);
            self.inner.encode(frame)?;
        } else {
            self.eos = true;
            self.inner.finish()?;
        }

        while let Some(encoded) = self.inner.next_encoded_frame() {
            self.stats.total_output_video_frame_count.add(1);
            self.encoded.push_back(encoded);
        }
        Ok(())
    }

    fn process_output(&mut self) -> crate::Result<MediaProcessorOutput> {
        if let Some(frame) = self.encoded.pop_front() {
            Ok(MediaProcessorOutput::Processed {
                stream_id: self.output_stream_id,
                sample: MediaSample::video_frame(frame),
            })
        } else if self.eos {
            Ok(MediaProcessorOutput::Finished)
        } else {
            Ok(MediaProcessorOutput::Pending {
                awaiting_stream_id: Some(self.input_stream_id),
            })
        }
    }
}

#[derive(Debug)]
enum VideoEncoderInner {
    // LibvpxEncoder は shiguredo_libvpx 2026 系で内部保持データが大きくなり、
    // 他のバリアントとのサイズ差が clippy::large_enum_variant の閾値を超えたので Box で包む
    Libvpx(Box<LibvpxEncoder>),
    Openh264(Openh264Encoder),
    SvtAv1(SvtAv1Encoder),
    #[cfg(target_os = "macos")]
    VideoToolbox(VideoToolboxEncoder),
    #[cfg(feature = "nvcodec")]
    Nvcodec(Box<NvcodecEncoder>), // Box は clippy::large_enum_variant 対策
}

impl VideoEncoderInner {
    fn new_vp8(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let encoder = LibvpxEncoder::new_vp8(options)?;
        Ok(Self::Libvpx(Box::new(encoder)))
    }

    fn new_vp9(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let encoder = LibvpxEncoder::new_vp9(options)?;
        Ok(Self::Libvpx(Box::new(encoder)))
    }

    fn new_openh264(lib: Openh264Library, options: &VideoEncoderOptions) -> crate::Result<Self> {
        let encoder = Openh264Encoder::new(lib, options)?;
        Ok(Self::Openh264(encoder))
    }

    fn new_svt_av1(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let encoder = SvtAv1Encoder::new(options)?;
        Ok(Self::SvtAv1(encoder))
    }

    #[cfg(target_os = "macos")]
    fn new_video_toolbox_h264(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let encoder = VideoToolboxEncoder::new_h264(options)?;
        Ok(Self::VideoToolbox(encoder))
    }

    #[cfg(target_os = "macos")]
    fn new_video_toolbox_h265(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let encoder = VideoToolboxEncoder::new_h265(options)?;
        Ok(Self::VideoToolbox(encoder))
    }

    #[cfg(feature = "nvcodec")]
    fn new_nvcodec_h265(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let encoder = NvcodecEncoder::new_h265(options)?;
        Ok(Self::Nvcodec(Box::new(encoder)))
    }

    #[cfg(feature = "nvcodec")]
    fn new_nvcodec_h264(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let encoder = NvcodecEncoder::new_h264(options)?;
        Ok(Self::Nvcodec(Box::new(encoder)))
    }

    #[cfg(feature = "nvcodec")]
    fn new_nvcodec_av1(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let encoder = NvcodecEncoder::new_av1(options)?;
        Ok(Self::Nvcodec(Box::new(encoder)))
    }

    fn encode(&mut self, frame: Arc<VideoFrame>) -> crate::Result<()> {
        match self {
            Self::Libvpx(encoder) => encoder.encode(frame),
            Self::Openh264(encoder) => encoder.encode(frame),
            Self::SvtAv1(encoder) => encoder.encode(frame),
            #[cfg(target_os = "macos")]
            Self::VideoToolbox(encoder) => encoder.encode(frame),
            #[cfg(feature = "nvcodec")]
            Self::Nvcodec(encoder) => encoder.encode(&frame),
        }
    }

    fn finish(&mut self) -> crate::Result<()> {
        match self {
            Self::Libvpx(encoder) => encoder.finish(),
            Self::Openh264(encoder) => encoder.finish(),
            Self::SvtAv1(encoder) => encoder.finish(),
            #[cfg(target_os = "macos")]
            Self::VideoToolbox(encoder) => encoder.finish(),
            #[cfg(feature = "nvcodec")]
            Self::Nvcodec(encoder) => encoder.finish(),
        }
    }

    fn next_encoded_frame(&mut self) -> Option<VideoFrame> {
        match self {
            Self::Libvpx(encoder) => encoder.next_encoded_frame(),
            Self::Openh264(encoder) => encoder.next_encoded_frame(),
            Self::SvtAv1(encoder) => encoder.next_encoded_frame(),
            #[cfg(target_os = "macos")]
            Self::VideoToolbox(encoder) => encoder.next_encoded_frame(),
            #[cfg(feature = "nvcodec")]
            Self::Nvcodec(encoder) => encoder.next_encoded_frame(),
        }
    }

    fn name(&self) -> EngineName {
        match self {
            Self::Libvpx(_) => EngineName::Libvpx,
            Self::Openh264(_) => EngineName::Openh264,
            Self::SvtAv1(_) => EngineName::SvtAv1,
            #[cfg(target_os = "macos")]
            Self::VideoToolbox(_) => EngineName::VideoToolbox,
            #[cfg(feature = "nvcodec")]
            Self::Nvcodec(_) => EngineName::Nvcodec,
        }
    }

    fn codec(&self) -> CodecName {
        match self {
            Self::Libvpx(encoder) => encoder.codec(),
            Self::Openh264(_) => CodecName::H264,
            Self::SvtAv1(_) => CodecName::Av1,
            #[cfg(target_os = "macos")]
            Self::VideoToolbox(encoder) => encoder.codec(),
            #[cfg(feature = "nvcodec")]
            Self::Nvcodec(encoder) => encoder.codec(),
        }
    }
}
