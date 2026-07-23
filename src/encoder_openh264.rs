use std::sync::Arc;

use crate::{
    encoder::VideoEncoderOptions,
    video::{VideoFormat, VideoFrame},
    video_h264::{self},
};

#[derive(Debug)]
pub struct Openh264Encoder {
    inner: shiguredo_openh264::Encoder,
    encoded: Option<VideoFrame>,
    is_first: bool,
}

impl Openh264Encoder {
    pub fn new(
        lib: shiguredo_openh264::Openh264Library,
        options: &VideoEncoderOptions,
    ) -> crate::Result<Self> {
        let width = options.width.get();
        let height = options.height.get();
        let config = shiguredo_openh264::EncoderConfig {
            fps_numerator: options.frame_rate.numerator.get(),
            fps_denominator: options.frame_rate.denominator.get(),
            width,
            height,
            target_bitrate: options.bitrate,
            ..options.encode_params.openh264.clone()
        };
        // 2026.1.0 で Encoder::new は EncoderConfig を所有権で受け取るようになった
        let inner = shiguredo_openh264::Encoder::new(lib, config)?;
        Ok(Self {
            inner,
            encoded: None,
            is_first: true,
        })
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
        // 2026.1.0 で EncodeOptions が追加された。ここでは force_idr を指定しない
        let encode_options = shiguredo_openh264::EncodeOptions { force_idr: false };
        let encoded = self
            .inner
            .encode(y_plane, u_plane, v_plane, &encode_options)?;
        let Some(encoded) = encoded else {
            return Ok(());
        };
        // 2026.1.0 で EncodedFrame.keyframe が廃止されて frame_type が導入された。
        // IDR のみをキーフレーム扱いにする
        let is_keyframe = encoded.frame_type == shiguredo_openh264::FrameType::Idr;

        // 2026.1.0 では SPS/PPS が data から分離されて別フィールドで返ってくるので、
        // サンプルエントリー生成用と AVCC 変換用の Annex B バイト列を再構築する
        let mut annexb_for_sample_entry = Vec::new();
        for nalu in encoded
            .sps_list
            .iter()
            .chain(encoded.pps_list.iter())
            .chain(std::iter::once(&encoded.data))
        {
            annexb_for_sample_entry.extend_from_slice(&[0, 0, 0, 1]);
            annexb_for_sample_entry.extend_from_slice(nalu);
        }

        let sample_entry = if self.is_first {
            self.is_first = false;
            Some(video_h264::h264_sample_entry_from_annexb(
                frame.width,
                frame.height,
                &annexb_for_sample_entry,
            )?)
        } else {
            None
        };

        // AnnexB から MP4 向けの形式に変換する。SPS/PPS は sample_entry 側で処理済みなので data のみを走査する
        let data = video_h264::h264_annexb_to_length_prefixed_skip_sei(&encoded.data)?;

        self.encoded = Some(VideoFrame {
            source_id: None,
            data,
            format: VideoFormat::H264,
            keyframe: is_keyframe,
            width: frame.width,
            height: frame.height,
            timestamp: frame.timestamp,
            duration: frame.duration,
            sample_entry,
        });

        Ok(())
    }

    // 他のエンコーダーに合わせてメソッドだけ用意しておく
    pub fn finish(&mut self) -> crate::Result<()> {
        Ok(())
    }

    pub fn next_encoded_frame(&mut self) -> Option<VideoFrame> {
        self.encoded.take()
    }
}
