use std::collections::VecDeque;

use shiguredo_openh264::Openh264Library;

use crate::video::{VideoFormat, VideoFrame};
use crate::video_h264;

#[derive(Debug)]
pub struct Openh264Decoder {
    inner: shiguredo_openh264::Decoder,
    input_queue: VecDeque<VideoFrame>,
    output_queue: VecDeque<VideoFrame>,
}

impl Openh264Decoder {
    pub fn new(lib: Openh264Library) -> crate::Result<Self> {
        Ok(Self {
            inner: shiguredo_openh264::Decoder::new(lib)?,
            input_queue: VecDeque::new(),
            output_queue: VecDeque::new(),
        })
    }

    pub fn decode(&mut self, frame: &VideoFrame) -> crate::Result<()> {
        if !matches!(frame.format, VideoFormat::H264 | VideoFormat::H264AnnexB) {
            return Err(crate::Error::new(format!(
                "expected H264 video frame, got {:?}",
                frame.format
            )));
        }

        if frame.keyframe {
            // SPS / PPS などが変わると、デコーダーのバッファ内のフレームが失われることがあるようなので、
            // 変更の可能性があるキーフレームを処理する前に、常に finish() を呼ぶようにしている。
            // （よりちゃんとやるなら、frame.data をパースして SPS / PPS の変更をチェックするようにするといい）
            self.finish()?;
        }

        let decoded = if matches!(frame.format, VideoFormat::H264) {
            self.inner
                .decode(&video_h264::h264_length_prefixed_to_annexb(&frame.data)?)?
        } else {
            self.inner.decode(&frame.data)?
        };
        self.input_queue.push_back(frame.to_stripped());

        let Some(decoded) = decoded else {
            // まだデコーダーのバッファ内にある
            return Ok(());
        };

        let input_frame = self
            .input_queue
            .pop_front()
            .ok_or_else(|| crate::Error::new("input queue is empty when handling decoded frame"))?;
        let output_frame = Self::to_rgb_frame(input_frame, decoded)?;
        self.output_queue.push_back(output_frame);
        Ok(())
    }

    pub fn finish(&mut self) -> crate::Result<()> {
        let Some(decoded) = self.inner.finish()? else {
            return Ok(());
        };
        let input_frame = self
            .input_queue
            .pop_front()
            .ok_or_else(|| crate::Error::new("input queue is empty when handling decoded frame"))?;
        let output_frame = Self::to_rgb_frame(input_frame, decoded)?;
        self.output_queue.push_back(output_frame);
        Ok(())
    }

    fn to_rgb_frame(
        input_frame: VideoFrame,
        frame: shiguredo_openh264::DecodedFrame,
    ) -> crate::Result<VideoFrame> {
        VideoFrame::new_i420(
            input_frame,
            frame.width(),
            frame.height(),
            frame.y_plane(),
            frame.u_plane(),
            frame.v_plane(),
            frame.y_stride(),
            frame.u_stride(),
            frame.v_stride(),
        )
    }

    pub fn next_decoded_frame(&mut self) -> Option<VideoFrame> {
        self.output_queue.pop_front()
    }
}
