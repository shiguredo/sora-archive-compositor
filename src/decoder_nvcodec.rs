use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::layout_decode_params::LayoutDecodeParams;
use crate::video::{VideoFormat, VideoFrame};
use crate::video_h264::{self, H264_NALU_TYPE_PPS, H264_NALU_TYPE_SPS};
use crate::video_h265::{
    self, H265_NALU_TYPE_PPS, H265_NALU_TYPE_SPS, H265_NALU_TYPE_VPS, NALU_HEADER_LENGTH,
};

/// callback スレッドからデコード結果を受け取るキュー
#[derive(Debug, Default)]
struct DecodeOutputQueue {
    ok_frames: VecDeque<DecodedFrameWithMeta>,
    errors: VecDeque<crate::Error>,
}

/// callback で受け取ったフレームと、user_data として渡した入力側メタデータ
#[derive(Debug)]
struct DecodedFrameWithMeta {
    width: usize,
    height: usize,
    /// 元データは NV12。y_stride == uv_stride == pitch
    nv12_data: Vec<u8>,
    y_stride: usize,
    uv_stride: usize,
    input_frame: VideoFrame,
}

type NvcodecDecodeHandler =
    shiguredo_nvcodec::FnDecodeHandler<VideoFrame, shiguredo_nvcodec::Error>;

#[derive(Debug)]
pub struct NvcodecDecoder {
    inner: shiguredo_nvcodec::Decoder<NvcodecDecodeHandler>,
    output_queue: Arc<Mutex<DecodeOutputQueue>>,
    parameter_sets: Option<Vec<u8>>, // VPS/SPS/PPS をキャッシュ
}

impl NvcodecDecoder {
    pub fn new_h264(params: &LayoutDecodeParams) -> crate::Result<Self> {
        tracing::debug!("create nvcodec(H264) decoder");
        Self::new_common(params.nvcodec_h264.clone())
    }

    pub fn new_h265(params: &LayoutDecodeParams) -> crate::Result<Self> {
        tracing::debug!("create nvcodec(H265) decoder");
        Self::new_common(params.nvcodec_h265.clone())
    }

    pub fn new_av1(params: &LayoutDecodeParams) -> crate::Result<Self> {
        tracing::debug!("create nvcodec(AV1) decoder");
        Self::new_common(params.nvcodec_av1.clone())
    }

    pub fn new_vp8(params: &LayoutDecodeParams) -> crate::Result<Self> {
        tracing::debug!("create nvcodec(VP8) decoder");
        Self::new_common(params.nvcodec_vp8.clone())
    }

    pub fn new_vp9(params: &LayoutDecodeParams) -> crate::Result<Self> {
        tracing::debug!("create nvcodec(VP9) decoder");
        Self::new_common(params.nvcodec_vp9.clone())
    }

    fn new_common(config: shiguredo_nvcodec::DecoderConfig) -> crate::Result<Self> {
        let output_queue: Arc<Mutex<DecodeOutputQueue>> = Arc::new(Mutex::new(Default::default()));
        let handler_queue = output_queue.clone();
        let handler = shiguredo_nvcodec::FnDecodeHandler::new(move |result| {
            handle_decode_callback(&handler_queue, result);
        });
        let inner = shiguredo_nvcodec::Decoder::new(config, handler)?;
        Ok(Self {
            inner,
            output_queue,
            parameter_sets: None,
        })
    }

    pub fn decode(&mut self, frame: &VideoFrame) -> crate::Result<()> {
        if !matches!(
            frame.format,
            VideoFormat::H264
                | VideoFormat::H264AnnexB
                | VideoFormat::H265
                | VideoFormat::Vp8
                | VideoFormat::Vp9
                | VideoFormat::Av1
        ) {
            return Err(crate::Error::new(format!(
                "unsupported video format for nvcodec decoder: {:?}",
                frame.format
            )));
        }

        // サンプルエントリからパラメータセットを抽出してキャッシュを更新する。
        //
        // MP4 reader は sample entry が変化したフレームでのみ `sample_entry` を `Some` にする。
        // 解像度変化などで sample entry が更新されたとき、新しいパラメータセット
        // (H.264 は SPS / PPS、H.265 は VPS / SPS / PPS) を prepend できるよう取り直す。
        if let Some(sample_entry) = &frame.sample_entry {
            self.parameter_sets = Some(extract_parameter_sets_annexb(sample_entry, frame.format)?);
        }

        let data = if matches!(
            frame.format,
            VideoFormat::Vp8 | VideoFormat::Vp9 | VideoFormat::Av1
        ) {
            // VP8 / VP9 / AV1 の場合は Annex B 形式は存在しないので、データの変換は不要
            Cow::Borrowed(&frame.data)
        } else if frame.format == VideoFormat::H264AnnexB {
            // すでに Annex B 形式の場合はそのまま使用
            Cow::Borrowed(&frame.data)
        } else {
            // length-prefixed 形式を Annex B 形式へ変換する (H264 / H265)
            let mut data_annexb = if frame.format == VideoFormat::H265 {
                video_h265::h265_length_prefixed_to_annexb(&frame.data)?
            } else {
                video_h264::h264_length_prefixed_to_annexb(&frame.data)?
            };

            // キーフレームで、かつパラメータセットがデータに含まれていない場合は先頭に追加
            if frame.keyframe
                && let Some(parameter_sets) = &self.parameter_sets
                && !contains_parameter_sets(&frame.data, frame.format)
            {
                let mut prefixed = parameter_sets.clone();
                prefixed.extend_from_slice(&data_annexb);
                data_annexb = prefixed;
            }

            Cow::Owned(data_annexb)
        };

        // 2026.2.0 で decode は user_data (第 2 引数) を受け取るようになった
        self.inner.decode(&data, frame.to_stripped())?;
        Ok(())
    }

    pub fn finish(&mut self) -> crate::Result<()> {
        // 2026.2.0 で finish() は flush() にリネームされた。
        // Decoder::flush() は callback を同期的に呼び切って戻る。
        self.inner.flush()?;
        Ok(())
    }

    pub fn next_decoded_frame(&mut self) -> Option<VideoFrame> {
        let decoded = {
            let mut queue = self.output_queue.lock().expect("output queue is poisoned");
            while let Some(err) = queue.errors.pop_front() {
                tracing::error!("nvcodec decode error: {}", err.display());
            }
            queue.ok_frames.pop_front()?
        };

        // NV12 から I420 への変換
        let width = decoded.width;
        let height = decoded.height;
        let uv_width = width.div_ceil(2);
        let uv_height = height.div_ceil(2);

        // I420 用のバッファを確保
        let y_size = width * height;
        let uv_size = uv_width * uv_height;
        let total_size = y_size + uv_size * 2;

        let mut i420_data = vec![0u8; total_size];
        let (y_plane, rest) = i420_data.split_at_mut(y_size);
        let (u_plane, v_plane) = rest.split_at_mut(uv_size);

        // libyuv を使って NV12 から I420 に変換
        let (nv12_y, nv12_uv) = decoded.nv12_data.split_at(decoded.y_stride * height);
        let src = shiguredo_libyuv::Nv12Image {
            y: nv12_y,
            y_stride: decoded.y_stride,
            uv: nv12_uv,
            uv_stride: decoded.uv_stride,
        };

        let mut dst = shiguredo_libyuv::I420ImageMut {
            y: y_plane,
            y_stride: width,
            u: u_plane,
            u_stride: uv_width,
            v: v_plane,
            v_stride: uv_width,
        };

        let size = shiguredo_libyuv::ImageSize::new(width, height);
        if let Err(e) = shiguredo_libyuv::nv12_to_i420(&src, &mut dst, size) {
            tracing::error!("libyuv nv12_to_i420 failed: {e}");
            return None;
        }

        match VideoFrame::new_i420(
            decoded.input_frame,
            width,
            height,
            y_plane,
            u_plane,
            v_plane,
            width,
            uv_width,
            uv_width,
        ) {
            Ok(frame) => Some(frame),
            Err(e) => {
                tracing::error!("failed to assemble I420 frame: {}", e.display());
                None
            }
        }
    }
}

/// callback スレッドから呼ばれるコールバック本体
fn handle_decode_callback(
    output_queue: &Mutex<DecodeOutputQueue>,
    result: std::result::Result<
        shiguredo_nvcodec::DecodedFrame<VideoFrame>,
        shiguredo_nvcodec::Error,
    >,
) {
    match result {
        Ok(decoded) => {
            let width = decoded.width();
            let height = decoded.height();
            let y_stride = decoded.y_stride();
            let uv_stride = decoded.uv_stride();
            let (nv12_data, input_frame) = decoded.into_parts();
            output_queue
                .lock()
                .expect("output queue is poisoned")
                .ok_frames
                .push_back(DecodedFrameWithMeta {
                    width,
                    height,
                    nv12_data,
                    y_stride,
                    uv_stride,
                    input_frame,
                });
        }
        Err(err) => {
            output_queue
                .lock()
                .expect("output queue is poisoned")
                .errors
                .push_back(crate::Error::new(format!("nvcodec decode error: {err}")));
        }
    }
}

/// サンプルエントリからパラメータセットを Annex.B 形式で抽出
fn extract_parameter_sets_annexb(
    sample_entry: &shiguredo_mp4::boxes::SampleEntry,
    format: VideoFormat,
) -> crate::Result<Vec<u8>> {
    use shiguredo_mp4::boxes::SampleEntry;

    match (sample_entry, format) {
        (SampleEntry::Hev1(entry), VideoFormat::H265) => {
            let mut annexb_data = Vec::new();
            for array in &entry.hvcc_box.nalu_arrays {
                for nalu in &array.nalus {
                    annexb_data.extend_from_slice(&[0, 0, 0, 1]);
                    annexb_data.extend_from_slice(nalu);
                }
            }
            Ok(annexb_data)
        }
        (SampleEntry::Hvc1(entry), VideoFormat::H265) => {
            let mut annexb_data = Vec::new();
            for array in &entry.hvcc_box.nalu_arrays {
                for nalu in &array.nalus {
                    annexb_data.extend_from_slice(&[0, 0, 0, 1]);
                    annexb_data.extend_from_slice(nalu);
                }
            }
            Ok(annexb_data)
        }
        (SampleEntry::Avc1(entry), VideoFormat::H264) => {
            let mut annexb_data = Vec::new();
            // SPS
            for sps in &entry.avcc_box.sps_list {
                annexb_data.extend_from_slice(&[0, 0, 0, 1]);
                annexb_data.extend_from_slice(sps);
            }
            // PPS
            for pps in &entry.avcc_box.pps_list {
                annexb_data.extend_from_slice(&[0, 0, 0, 1]);
                annexb_data.extend_from_slice(pps);
            }
            Ok(annexb_data)
        }
        _ => {
            // VP8 / VP9 / AV1 はパラメータセットを個別に送る必要がないため空の Vec を返す
            Ok(Vec::new())
        }
    }
}

/// データの先頭にパラメータセットが含まれているかチェック
fn contains_parameter_sets(data: &[u8], format: VideoFormat) -> bool {
    if data.len() < NALU_HEADER_LENGTH + 1 {
        return false;
    }

    match format {
        VideoFormat::H265 => {
            // H.265 の NAL unit type は 2 バイト目の上位 6 ビット
            let nal_unit_type = (data[NALU_HEADER_LENGTH] >> 1) & 0x3F;
            matches!(
                nal_unit_type,
                H265_NALU_TYPE_PPS | H265_NALU_TYPE_SPS | H265_NALU_TYPE_VPS
            )
        }
        VideoFormat::H264 => {
            // H.264 の NAL unit type は下位 5 ビット
            let nal_unit_type = data[NALU_HEADER_LENGTH] & 0x1F;
            matches!(nal_unit_type, H264_NALU_TYPE_SPS | H264_NALU_TYPE_PPS)
        }
        VideoFormat::Av1 => {
            // AV1 はパラメータセットの概念が異なるため常に false
            false
        }
        _ => false,
    }
}
