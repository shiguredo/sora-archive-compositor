use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use shiguredo_mp4::boxes::{Avc1Box, AvccBox, SampleEntry};

use crate::{
    video::{VideoFormat, VideoFrame},
    video_h264::{
        self, H264_NALU_TYPE_PPS, H264_NALU_TYPE_SPS, H264AnnexBNalUnits, NALU_HEADER_LENGTH,
    },
    video_h265::{H265_NALU_TYPE_PPS, H265_NALU_TYPE_SPS, H265_NALU_TYPE_VPS},
};

/// callback スレッドから呼ばれる Handler をラップした型
type VideoToolboxDecodeHandler =
    shiguredo_video_toolbox::FnDecodeHandler<VideoFrame, shiguredo_video_toolbox::Error>;

/// デコード結果を積むキュー
#[derive(Debug, Default)]
struct DecodeOutputQueue {
    ok_frames: VecDeque<VideoFrame>,
    errors: VecDeque<crate::Error>,
}

// 2026.2.0-canary.2 の Decoder は Debug を実装していないため、手動で Debug を実装する
pub struct VideoToolboxDecoder {
    inner: shiguredo_video_toolbox::Decoder<VideoToolboxDecodeHandler>,
    output_queue: Arc<Mutex<DecodeOutputQueue>>,

    // デコーダーの再初期化が必要かどうかの判定に使うフィールド
    vps: Vec<u8>,
    sps: Vec<u8>,
    pps: Vec<u8>,
}

impl std::fmt::Debug for VideoToolboxDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoToolboxDecoder")
            .field("vps", &self.vps)
            .field("sps", &self.sps)
            .field("pps", &self.pps)
            .finish()
    }
}

impl VideoToolboxDecoder {
    pub fn new_h264(frame: &VideoFrame) -> crate::Result<Self> {
        let (sps, pps) = get_h264_sps_pps(frame)?;
        tracing::debug!("Initialize H.264 decoder: sps={sps:?}, pps={pps:?}");

        // 2026.2.0-canary.2 で Decoder::new は handler 引数が必要になり、
        // decode はコールバック通知ベースに変わった
        let config = shiguredo_video_toolbox::DecoderConfig {
            codec: shiguredo_video_toolbox::DecoderCodec::H264 {
                sps: &sps,
                pps: &pps,
                nalu_len_bytes: NALU_HEADER_LENGTH as u32,
            },
            // Sora 録画で扱う YUV は I420 想定なので、デコード出力も I420 に揃える
            pixel_format: shiguredo_video_toolbox::PixelFormat::I420,
        };
        let (inner, output_queue) = Self::build_decoder(config)?;
        Ok(Self {
            inner,
            output_queue,
            vps: Vec::new(),
            sps,
            pps,
        })
    }

    pub fn new_h265(frame: &VideoFrame) -> crate::Result<Self> {
        let (vps, sps, pps) = get_h265_vps_sps_pps(frame)?;
        tracing::debug!("Initialize H.265 decoder: vps={vps:?}, sps={sps:?}, pps={pps:?}");

        let config = shiguredo_video_toolbox::DecoderConfig {
            codec: shiguredo_video_toolbox::DecoderCodec::Hevc {
                vps,
                sps,
                pps,
                nalu_len_bytes: NALU_HEADER_LENGTH as u32,
            },
            pixel_format: shiguredo_video_toolbox::PixelFormat::I420,
        };
        let (inner, output_queue) = Self::build_decoder(config)?;
        Ok(Self {
            inner,
            output_queue,
            vps: vps.to_vec(),
            sps: sps.to_vec(),
            pps: pps.to_vec(),
        })
    }

    /// handler を構築して Decoder を生成する共通処理
    fn build_decoder(
        config: shiguredo_video_toolbox::DecoderConfig<'_>,
    ) -> crate::Result<(
        shiguredo_video_toolbox::Decoder<VideoToolboxDecodeHandler>,
        Arc<Mutex<DecodeOutputQueue>>,
    )> {
        let output_queue: Arc<Mutex<DecodeOutputQueue>> = Arc::new(Mutex::new(Default::default()));
        let handler_queue = output_queue.clone();
        let handler = shiguredo_video_toolbox::FnDecodeHandler::new(move |result| {
            handle_decode_callback(&handler_queue, result);
        });
        let inner = shiguredo_video_toolbox::Decoder::new(config, handler)?;
        Ok((inner, output_queue))
    }

    // VPS / SPS / PPS の情報が変わっていたらデコーダーを再初期化する
    //
    // [NOTE] WebM 対応がなくなったら VideoDecoder 側でサンプルエントリーの変更を見てハンドリングできる
    fn reinitialize_if_need(&mut self, frame: &VideoFrame) -> crate::Result<()> {
        if !frame.keyframe {
            // 切り替わりが発生するのは必ずキーフレーム
            return Ok(());
        }

        let changed = if frame.format == VideoFormat::H265 {
            // [NOTE] VPS / SPS / PPS が存在しない場合には、デコード情報が変わっていないと判断して何もしない
            get_h265_vps_sps_pps(frame)
                .map(|(vps, sps, pps)| vps != self.vps || sps != self.sps || pps != self.pps)
                .unwrap_or(false)
        } else {
            // [NOTE] VPS / SPS / PPS が存在しない場合には、デコード情報が変わっていないと判断して何もしない
            get_h264_sps_pps(frame)
                .map(|(sps, pps)| sps != self.sps || pps != self.pps)
                .unwrap_or(false)
        };

        if changed {
            self.reinitialize(frame)?;
        }
        Ok(())
    }

    /// デコーダーを作り直す
    ///
    /// 2026.2.0-canary.2 のデコードは非同期コールバックベースのため、
    /// 先に finish() で古いデコーダーのコールバックを全て完了させてから、
    /// 古いキューの内容を新しいキューへ引き継ぐ (デコード済みフレームの消失を防ぐ)
    fn reinitialize(&mut self, frame: &VideoFrame) -> crate::Result<()> {
        // 古いデコーダーの遅延フレームを排出し、非同期コールバックの完了を待つ
        self.inner.finish()?;
        let old_queue = std::mem::take(&mut self.output_queue);
        let new_decoder = if frame.format == VideoFormat::H265 {
            Self::new_h265(frame)?
        } else {
            Self::new_h264(frame)?
        };
        // 古いキューに残っているフレームを新しいキューへ移す
        {
            let mut new_queue = new_decoder
                .output_queue
                .lock()
                .expect("output queue is poisoned");
            let mut old_queue = old_queue.lock().expect("output queue is poisoned");
            new_queue.ok_frames.append(&mut old_queue.ok_frames);
            new_queue.errors.append(&mut old_queue.errors);
        }
        *self = new_decoder;
        Ok(())
    }

    pub fn decode(&mut self, frame: &VideoFrame) -> crate::Result<()> {
        if !matches!(
            frame.format,
            VideoFormat::H264 | VideoFormat::H264AnnexB | VideoFormat::H265
        ) {
            return Err(crate::Error::new(format!(
                "expected H264 or H265 video frame, got {:?}",
                frame.format
            )));
        }

        self.reinitialize_if_need(frame)?;

        let data = if matches!(frame.format, VideoFormat::H264AnnexB) {
            video_h264::h264_annexb_to_length_prefixed(&frame.data)?
        } else {
            frame.data.clone()
        };

        // 2026.2.0-canary.2 で decode は user_data (第 2 引数) を受け取るようになった。
        // コールバックスレッドで入力フレームのメタデータを復元するために to_stripped() を渡す
        self.inner.decode(&data, frame.to_stripped())?;
        Ok(())
    }

    pub fn finish(&mut self) -> crate::Result<()> {
        // 2026.2.0-canary.2 で finish() は遅延フレームの排出と非同期コールバックの完了待ちを行う
        self.inner.finish()?;
        Ok(())
    }

    pub fn next_decoded_frame(&mut self) -> Option<VideoFrame> {
        let mut queue = self.output_queue.lock().expect("output queue is poisoned");
        // エラーは呼び出し側では取り出せないので、ここではとりあえずログに出しつつ捨てる
        while let Some(err) = queue.errors.pop_front() {
            tracing::error!("video toolbox decode error: {}", err.display());
        }
        queue.ok_frames.pop_front()
    }
}

/// callback スレッドから呼ばれるコールバック本体
fn handle_decode_callback(
    output_queue: &Mutex<DecodeOutputQueue>,
    result: std::result::Result<
        shiguredo_video_toolbox::DecodedFrame<VideoFrame>,
        shiguredo_video_toolbox::Error,
    >,
) {
    match result {
        Ok(decoded) => {
            // 2026.2.0-canary.2 で DecodedFrame は構造体 variant に変わった
            let shiguredo_video_toolbox::DecodedFrame::I420 { frame, user_data } = decoded else {
                // I420 のみを設定しているので、Nv12 が来ることはない
                output_queue
                    .lock()
                    .expect("output queue is poisoned")
                    .errors
                    .push_back(crate::Error::new(
                        "unexpected Nv12 decoded frame: decoder is configured for I420 output",
                    ));
                return;
            };
            let width = frame.width();
            let height = frame.height();
            match VideoFrame::new_i420(
                user_data,
                width,
                height,
                frame.y_plane(),
                frame.u_plane(),
                frame.v_plane(),
                frame.y_stride(),
                frame.u_stride(),
                frame.v_stride(),
            ) {
                Ok(i420) => {
                    output_queue
                        .lock()
                        .expect("output queue is poisoned")
                        .ok_frames
                        .push_back(i420);
                }
                Err(e) => {
                    output_queue
                        .lock()
                        .expect("output queue is poisoned")
                        .errors
                        .push_back(e);
                }
            }
        }
        Err(err) => {
            output_queue
                .lock()
                .expect("output queue is poisoned")
                .errors
                .push_back(crate::Error::new(format!(
                    "video toolbox decode error: {err}"
                )));
        }
    }
}

fn get_h264_sps_pps(frame: &VideoFrame) -> crate::Result<(Vec<u8>, Vec<u8>)> {
    if !matches!(frame.format, VideoFormat::H264 | VideoFormat::H264AnnexB) {
        return Err(crate::Error::new(format!(
            "expected H264 video frame, got {:?}",
            frame.format
        )));
    }

    let mut sps = Vec::new();
    let mut pps = Vec::new();
    match frame.format {
        VideoFormat::H264AnnexB => {
            for nal in H264AnnexBNalUnits::new(&frame.data) {
                let nal = nal?;
                match nal.ty {
                    H264_NALU_TYPE_SPS => sps = nal.data.to_vec(),
                    H264_NALU_TYPE_PPS => pps = nal.data.to_vec(),
                    _ => {}
                }
            }
        }
        VideoFormat::H264 => {
            let Some(SampleEntry::Avc1(Avc1Box {
                avcc_box: AvccBox {
                    sps_list, pps_list, ..
                },
                ..
            })) = &frame.sample_entry
            else {
                return Err(crate::Error::new(
                    "missing sample entry for H.264 first frame",
                ));
            };
            sps = sps_list
                .first()
                .ok_or_else(|| crate::Error::new("H.264 avcC box has no SPS"))?
                .to_vec();
            pps = pps_list
                .first()
                .ok_or_else(|| crate::Error::new("H.264 avcC box has no PPS"))?
                .to_vec();
        }
        _ => unreachable!(),
    }
    if sps.is_empty() {
        return Err(crate::Error::new("assertion failed: !sps.is_empty()"));
    }
    if pps.is_empty() {
        return Err(crate::Error::new("assertion failed: !pps.is_empty()"));
    }

    Ok((sps, pps))
}

fn get_h265_vps_sps_pps(frame: &VideoFrame) -> crate::Result<(&[u8], &[u8], &[u8])> {
    if !matches!(frame.format, VideoFormat::H265) {
        return Err(crate::Error::new(format!(
            "expected H265 video frame, got {:?}",
            frame.format
        )));
    }

    // Hev1Box と Hvc1Box は別型のため or-pattern では同一識別子に束縛できない。
    // 共通の HvccBox 参照にまとめてから NALU を取り出す。
    let hvcc = match &frame.sample_entry {
        Some(SampleEntry::Hev1(b)) => &b.hvcc_box,
        Some(SampleEntry::Hvc1(b)) => &b.hvcc_box,
        _ => return Err(crate::Error::new("no H.265 sample entry")),
    };

    let mut vps = &[][..];
    let mut sps = &[][..];
    let mut pps = &[][..];
    for arrays in &hvcc.nalu_arrays {
        if arrays.nalus.is_empty() {
            continue;
        }

        match arrays.nal_unit_type.get() {
            H265_NALU_TYPE_VPS => vps = arrays.nalus[0].as_slice(),
            H265_NALU_TYPE_SPS => sps = arrays.nalus[0].as_slice(),
            H265_NALU_TYPE_PPS => pps = arrays.nalus[0].as_slice(),
            _ => {}
        }
    }
    if vps.is_empty() {
        return Err(crate::Error::new("assertion failed: !vps.is_empty()"));
    }
    if sps.is_empty() {
        return Err(crate::Error::new("assertion failed: !sps.is_empty()"));
    }
    if pps.is_empty() {
        return Err(crate::Error::new("assertion failed: !pps.is_empty()"));
    }

    Ok((vps, sps, pps))
}
