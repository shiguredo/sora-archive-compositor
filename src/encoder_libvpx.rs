use std::collections::VecDeque;
use std::sync::Arc;

use shiguredo_mp4::{
    bitstream::vp8::{Vp8SampleEntryConfig, build_vp08_box},
    bitstream::vp9::{Vp9SampleEntryConfig, build_vp09_box, parse_frame_header},
    boxes::SampleEntry,
};

use crate::{
    encoder::VideoEncoderOptions,
    types::CodecName,
    video::{VideoFormat, VideoFrame},
};

// エンコードパラメーターのデフォルト値
pub const DEFAULT_CQ_LEVEL: &str = "30";
pub const DEFAULT_MIN_Q: &str = "10";
pub const DEFAULT_MAX_Q: &str = "50";

/// sample_entry の設定状態を共有する構造体
///
/// VP9 は初回キーフレーム取得後に sample entry を構築するため、
/// Video Toolbox エンコーダーと同型の slot パターンを使う
#[derive(Debug, Default)]
struct SampleEntrySlot {
    entry: Option<SampleEntry>,
    taken: bool,
}

#[derive(Debug)]
pub struct LibvpxEncoder {
    inner: shiguredo_libvpx::Encoder,
    format: VideoFormat,
    width: usize,
    height: usize,
    sample_entry_slot: SampleEntrySlot,
    input_queue: VecDeque<Arc<VideoFrame>>,
    output_queue: VecDeque<VideoFrame>,
}

impl LibvpxEncoder {
    pub fn new_vp8(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let width = options.width.get();
        let height = options.height.get();
        let config = shiguredo_libvpx::EncoderConfig {
            width,
            height,
            image_format: shiguredo_libvpx::ImageFormat::I420,
            fps_numerator: options.frame_rate.numerator.get(),
            fps_denominator: options.frame_rate.denominator.get(),
            target_bitrate: options.bitrate,
            ..options.encode_params.libvpx_vp8.clone()
        };
        tracing::debug!("libvpx vp8 encoder config: {config:?}");
        let inner = shiguredo_libvpx::Encoder::new(config)?;

        Ok(Self {
            inner,
            format: VideoFormat::Vp8,
            width,
            height,
            sample_entry_slot: SampleEntrySlot {
                entry: Some(vp8_sample_entry(width, height)),
                taken: false,
            },
            input_queue: VecDeque::new(),
            output_queue: VecDeque::new(),
        })
    }

    pub fn new_vp9(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let width = options.width.get();
        let height = options.height.get();
        let config = shiguredo_libvpx::EncoderConfig {
            width,
            height,
            image_format: shiguredo_libvpx::ImageFormat::I420,
            fps_numerator: options.frame_rate.numerator.get(),
            fps_denominator: options.frame_rate.denominator.get(),
            target_bitrate: options.bitrate,
            ..options.encode_params.libvpx_vp9.clone()
        };
        tracing::debug!("libvpx vp9 encoder config: {config:?}");
        let inner = shiguredo_libvpx::Encoder::new(config)?;

        Ok(Self {
            inner,
            format: VideoFormat::Vp9,
            width,
            height,
            sample_entry_slot: SampleEntrySlot::default(),
            input_queue: VecDeque::new(),
            output_queue: VecDeque::new(),
        })
    }

    pub fn codec(&self) -> CodecName {
        if self.format == VideoFormat::Vp8 {
            CodecName::Vp8
        } else {
            CodecName::Vp9
        }
    }

    pub fn encode(&mut self, frame: Arc<VideoFrame>) -> crate::Result<()> {
        if frame.format != VideoFormat::I420 {
            return Err(crate::Error::new(
                "assertion failed: frame.format == VideoFormat::I420",
            ));
        }

        let (y_plane, u_plane, v_plane) = frame
            .as_yuv_planes()
            .ok_or_else(|| crate::Error::new("failed to obtain YUV planes"))?;
        // 2026.2.0-canary.0 で encode の引数が ImageData + EncodeOptions ベースへ再設計された
        let image = shiguredo_libvpx::ImageData::I420 {
            y: y_plane,
            u: u_plane,
            v: v_plane,
        };
        let encode_options = shiguredo_libvpx::EncodeOptions {
            force_keyframe: false,
        };
        self.inner.encode(&image, &encode_options)?;
        self.input_queue.push_back(frame);
        self.handle_encoded_frames()?;

        Ok(())
    }

    pub fn finish(&mut self) -> crate::Result<()> {
        self.inner.finish()?;
        self.handle_encoded_frames()?;
        Ok(())
    }

    fn handle_encoded_frames(&mut self) -> crate::Result<()> {
        let mut pending = Vec::new();
        while let Some(frame) = self.inner.next_frame() {
            pending.push((
                frame.is_keyframe(),
                frame.data().to_vec(),
                frame.width() as usize,
                frame.height() as usize,
            ));
        }
        for (keyframe, data, width, height) in pending {
            let input_frame = self.input_queue.pop_front().ok_or_else(|| {
                crate::Error::new("input queue is empty when handling encoded frame")
            })?;
            let sample_entry = self.take_sample_entry(keyframe, &data)?;
            self.output_queue.push_back(VideoFrame {
                source_id: None,
                sample_entry,
                data,
                format: self.format,
                keyframe,
                width,
                height,
                timestamp: input_frame.timestamp,
                duration: input_frame.duration,
            });
        }

        Ok(())
    }

    fn take_sample_entry(
        &mut self,
        keyframe: bool,
        encoded_data: &[u8],
    ) -> crate::Result<Option<SampleEntry>> {
        if self.sample_entry_slot.taken {
            return Ok(None);
        }
        if self.sample_entry_slot.entry.is_none() && self.format == VideoFormat::Vp9 {
            if !keyframe {
                return Ok(None);
            }
            self.sample_entry_slot.entry = Some(vp9_sample_entry_from_frame(
                encoded_data,
                self.width,
                self.height,
            )?);
        }
        let entry = self.sample_entry_slot.entry.take();
        if entry.is_some() {
            self.sample_entry_slot.taken = true;
        }
        Ok(entry)
    }

    pub fn next_encoded_frame(&mut self) -> Option<VideoFrame> {
        self.output_queue.pop_front()
    }
}

fn vp8_sample_entry(width: usize, height: usize) -> SampleEntry {
    let config = Vp8SampleEntryConfig {
        video_full_range_flag: false,
        colour_primaries: Vp8SampleEntryConfig::COLOUR_PRIMARIES_BT709,
        transfer_characteristics: Vp8SampleEntryConfig::TRANSFER_CHARACTERISTICS_BT709,
        matrix_coefficients: Vp8SampleEntryConfig::MATRIX_COEFFICIENTS_BT709,
        width: width as u16,
        height: height as u16,
    };
    SampleEntry::Vp08(build_vp08_box(&config))
}

fn vp9_sample_entry_from_frame(
    data: &[u8],
    width: usize,
    height: usize,
) -> crate::Result<SampleEntry> {
    let header = parse_frame_header(data)?;
    let config = Vp9SampleEntryConfig {
        level: Vp9SampleEntryConfig::LEVEL_UNDEFINED,
        colour_primaries: Vp9SampleEntryConfig::COLOUR_PRIMARIES_BT709,
        transfer_characteristics: Vp9SampleEntryConfig::TRANSFER_CHARACTERISTICS_BT709,
        matrix_coefficients: Vp9SampleEntryConfig::MATRIX_COEFFICIENTS_BT709,
        width: width as u16,
        height: height as u16,
    };
    let vp09 = build_vp09_box(&header, &config)?;
    Ok(SampleEntry::Vp09(vp09))
}
