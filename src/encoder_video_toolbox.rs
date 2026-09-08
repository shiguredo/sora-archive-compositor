use std::sync::{Arc, Mutex};

use shiguredo_mp4::boxes::SampleEntry;

use crate::{
    encoder::VideoEncoderOptions,
    output_queue::OutputQueue,
    types::{CodecName, EvenUsize},
    video::{FrameRate, VideoFormat, VideoFrame},
    video_h264, video_h265,
};

/// callback スレッドから呼ばれる Handler をラップした型
type VideoToolboxEncodeHandler =
    shiguredo_video_toolbox::FnEncodeHandler<VideoFrame, shiguredo_video_toolbox::Error>;

#[derive(Debug)]
struct EncodedFrameWithMeta {
    data: Vec<u8>,
    keyframe: bool,
    input_frame: VideoFrame,
}

/// sample_entry の設定状態を共有するための構造体
///
/// callback スレッドと next_encoded_frame 側の両方から参照する。
/// entry は最初の出力フレームにだけ載せる。taken は「最初のフレームを出力済みか」を表し、
/// 出力後に take() で entry が空になっても、コールバックが再設定しないようにする
#[derive(Debug, Default)]
struct SampleEntrySlot {
    entry: Option<SampleEntry>,
    taken: bool,
}

// 2026.2.0-canary.2 の Encoder は Debug を実装していないため、手動で Debug を実装する
pub struct VideoToolboxEncoder {
    inner: shiguredo_video_toolbox::Encoder<VideoToolboxEncodeHandler>,
    output_queue: Arc<Mutex<OutputQueue<EncodedFrameWithMeta>>>,
    sample_entry: Arc<Mutex<SampleEntrySlot>>,
    width: EvenUsize,
    height: EvenUsize,
    format: VideoFormat,
    fps: FrameRate,
}

impl std::fmt::Debug for VideoToolboxEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoToolboxEncoder")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .field("fps", &self.fps)
            .finish()
    }
}

impl VideoToolboxEncoder {
    pub fn new_h264(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let width = options.width;
        let height = options.height;
        let mut config = options.encode_params.video_toolbox_h264.clone();
        // 2026.1.1 の EncoderConfig は width / height が u32 になった
        config.width = u32::try_from(width.get())
            .map_err(|_| crate::Error::new("video width is too large for VideoToolbox"))?;
        config.height = u32::try_from(height.get())
            .map_err(|_| crate::Error::new("video height is too large for VideoToolbox"))?;
        // 2026.1.1 で target_bitrate -> average_bitrate (Option<u64>) にリネーム
        config.average_bitrate = Some(options.bitrate as u64);
        config.fps_numerator = options.frame_rate.numerator.get() as u32;
        config.fps_denominator = options.frame_rate.denominator.get() as u32;
        Self::build_encoder(config, width, height, VideoFormat::H264, options.frame_rate)
    }

    pub fn new_h265(options: &VideoEncoderOptions) -> crate::Result<Self> {
        let width = options.width;
        let height = options.height;
        let mut config = options.encode_params.video_toolbox_h265.clone();
        config.width = u32::try_from(width.get())
            .map_err(|_| crate::Error::new("video width is too large for VideoToolbox"))?;
        config.height = u32::try_from(height.get())
            .map_err(|_| crate::Error::new("video height is too large for VideoToolbox"))?;
        config.average_bitrate = Some(options.bitrate as u64);
        config.fps_numerator = options.frame_rate.numerator.get() as u32;
        config.fps_denominator = options.frame_rate.denominator.get() as u32;
        Self::build_encoder(config, width, height, VideoFormat::H265, options.frame_rate)
    }

    /// handler を構築して Encoder を生成する共通処理
    fn build_encoder(
        config: shiguredo_video_toolbox::EncoderConfig,
        width: EvenUsize,
        height: EvenUsize,
        format: VideoFormat,
        fps: FrameRate,
    ) -> crate::Result<Self> {
        let output_queue: Arc<Mutex<OutputQueue<EncodedFrameWithMeta>>> =
            Arc::new(Mutex::new(OutputQueue::new()));
        let sample_entry: Arc<Mutex<SampleEntrySlot>> = Arc::new(Mutex::new(Default::default()));
        let handler_queue = output_queue.clone();
        let handler_sample_entry = sample_entry.clone();
        // 2026.2.0-canary.2 で Encoder::new は handler 引数が必要になり、
        // encode はコールバック通知ベースに変わった
        let handler = shiguredo_video_toolbox::FnEncodeHandler::new(move |result| {
            handle_encode_callback(
                &handler_queue,
                &handler_sample_entry,
                width,
                height,
                format,
                fps,
                result,
            );
        });
        let inner = shiguredo_video_toolbox::Encoder::new(config, handler)?;
        Ok(Self {
            inner,
            output_queue,
            sample_entry,
            width,
            height,
            format,
            fps,
        })
    }

    pub fn codec(&self) -> CodecName {
        if self.format == VideoFormat::H264 {
            CodecName::H264
        } else {
            CodecName::H265
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
        // 2026.1.1 で encode の入力が FrameData + EncodeOptions ベースへ再設計された
        let frame_data = shiguredo_video_toolbox::FrameData::I420 {
            y: y_plane,
            u: u_plane,
            v: v_plane,
        };
        let encode_options = shiguredo_video_toolbox::EncodeOptions {
            force_key_frame: false,
        };
        // 2026.2.0-canary.2 で encode は user_data (第 3 引数) を受け取るようになった。
        // callback スレッドで入力フレームのメタデータを復元するために to_stripped() を渡す
        self.inner
            .encode(&frame_data, &encode_options, frame.to_stripped())?;
        Ok(())
    }

    pub fn finish(&mut self) -> crate::Result<()> {
        // 2026.2.0-canary.2 で finish() は残りのエンコード結果をコールバックで通知して戻る
        self.inner.finish()?;
        Ok(())
    }

    pub fn next_encoded_frame(&mut self) -> crate::Result<Option<VideoFrame>> {
        let encoded = {
            let mut queue = self.output_queue.lock().expect("output queue is poisoned");
            match queue.pop()? {
                Some(encoded) => encoded,
                None => return Ok(None),
            }
        };
        // 最初の出力フレームにだけ sample_entry を載せる。take() で entry が空になっても、
        // コールバック側は taken フラグで再設定しない (SampleEntrySlot 参照)
        let sample_entry = {
            let mut slot = self.sample_entry.lock().expect("sample entry is poisoned");
            let entry = slot.entry.take();
            slot.taken = true;
            entry
        };
        Ok(Some(VideoFrame {
            source_id: encoded.input_frame.source_id.clone(),
            data: encoded.data,
            format: self.format,
            keyframe: encoded.keyframe,
            width: encoded.input_frame.width,
            height: encoded.input_frame.height,
            timestamp: encoded.input_frame.timestamp,
            duration: encoded.input_frame.duration,
            sample_entry,
        }))
    }
}

/// callback スレッドから呼ばれるコールバック本体
fn handle_encode_callback(
    output_queue: &Mutex<OutputQueue<EncodedFrameWithMeta>>,
    sample_entry_slot: &Mutex<SampleEntrySlot>,
    width: EvenUsize,
    height: EvenUsize,
    format: VideoFormat,
    fps: FrameRate,
    result: std::result::Result<
        shiguredo_video_toolbox::EncodedFrame<VideoFrame>,
        shiguredo_video_toolbox::Error,
    >,
) {
    match result {
        Ok(encoded_frame) => {
            // 最初の出力フレームのときだけ sample_entry を構築する。
            // 出力済み (taken) の場合は next_encoded_frame 側で take() 済みなので再設定しない
            let mut sample_entry_guard =
                sample_entry_slot.lock().expect("sample entry is poisoned");
            if !sample_entry_guard.taken && sample_entry_guard.entry.is_none() {
                let entry = if format == VideoFormat::H264 {
                    video_h264::h264_sample_entry_from_parameter_sets(
                        &encoded_frame.sps_list,
                        &encoded_frame.pps_list,
                    )
                } else {
                    video_h265::h265_sample_entry(
                        width.get(),
                        height.get(),
                        fps,
                        encoded_frame.vps_list.clone(),
                        encoded_frame.sps_list.clone(),
                        encoded_frame.pps_list.clone(),
                    )
                };
                match entry {
                    Ok(entry) => sample_entry_guard.entry = Some(entry),
                    Err(e) => {
                        drop(sample_entry_guard);
                        output_queue
                            .lock()
                            .expect("output queue is poisoned")
                            .push_err(e);
                        return;
                    }
                }
            }
            drop(sample_entry_guard);
            output_queue
                .lock()
                .expect("output queue is poisoned")
                .push_ok(EncodedFrameWithMeta {
                    data: encoded_frame.data,
                    keyframe: encoded_frame.keyframe,
                    input_frame: encoded_frame.user_data,
                });
        }
        Err(err) => {
            output_queue
                .lock()
                .expect("output queue is poisoned")
                .push_err(crate::Error::new(format!(
                    "video toolbox encode error: {err}"
                )));
        }
    }
}
