use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use crate::{
    media::MediaStreamId,
    processor::{
        MediaProcessor, MediaProcessorInput, MediaProcessorOutput, MediaProcessorSpec,
        MediaProcessorWorkloadHint,
    },
    stats::ProcessorStats,
    video::VideoFormat,
};

/// I420 映像フレームを連続した YUV ファイルへ書き出す Writer
///
/// Scheduler 上の `MediaProcessor` として動作する。API は hisui の `ProcessorHandle`
/// ベースではなく、本リポジトリの `MediaStreamId` ベースを維持する。
#[derive(Debug)]
pub struct YuvWriter {
    input_stream_id: MediaStreamId,
    eos: bool,
    file: File,
}

impl YuvWriter {
    pub fn new<P: AsRef<Path>>(input_stream_id: MediaStreamId, path: P) -> crate::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .map_err(|e| crate::Error::new(format!("{e}: {}", path.as_ref().display())))?;
        Ok(Self {
            input_stream_id,
            eos: false,
            file,
        })
    }
}

impl MediaProcessor for YuvWriter {
    fn spec(&self) -> MediaProcessorSpec {
        MediaProcessorSpec {
            input_stream_ids: vec![self.input_stream_id],
            output_stream_ids: Vec::new(),
            stats: ProcessorStats::other("yuv_writer"),
            workload_hint: MediaProcessorWorkloadHint::WRITER,
        }
    }

    fn process_input(&mut self, input: MediaProcessorInput) -> crate::Result<()> {
        if let Some(sample) = input.sample {
            let frame = sample.expect_video_frame()?;
            if !matches!(frame.format, VideoFormat::I420) {
                return Err(crate::Error::new(format!(
                    "expected I420 video frame, got {:?}",
                    frame.format
                )));
            }
            self.file.write_all(&frame.data)?;
        } else {
            self.eos = true;
        }
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

// I420 (YUV 4:2:0 8-bit) の生データファイルを 1 フレームずつ読み込むリーダー
//
// `YuvWriter` が書き出した連続バッファを、指定された解像度に基づいてフレーム単位に
// 区切りながら読み込む。VMAF 評価でフレームごとに参照・劣化画像を取り出すために使う。
#[derive(Debug)]
pub struct YuvReader {
    file: File,
    y_size: usize,
    chroma_size: usize,
}

// I420 の 1 フレーム分のデータと、その Y / U / V プレーンへの分割情報
#[derive(Debug)]
pub struct YuvFrame {
    data: Vec<u8>,
    y_size: usize,
    chroma_size: usize,
}

impl YuvFrame {
    pub fn y(&self) -> &[u8] {
        &self.data[..self.y_size]
    }

    pub fn u(&self) -> &[u8] {
        &self.data[self.y_size..self.y_size + self.chroma_size]
    }

    pub fn v(&self) -> &[u8] {
        &self.data[self.y_size + self.chroma_size..]
    }
}

impl YuvReader {
    pub fn new<P: AsRef<Path>>(path: P, width: usize, height: usize) -> crate::Result<Self> {
        // I420 の各プレーンサイズ。輝度は width * height、色差は水平・垂直ともに半分。
        // 本パイプラインでは解像度は常に偶数だが、念のため切り上げで計算する
        let y_size = width * height;
        let chroma_size = width.div_ceil(2) * height.div_ceil(2);
        let file = File::open(&path)
            .map_err(|e| crate::Error::new(format!("{e}: {}", path.as_ref().display())))?;
        Ok(Self {
            file,
            y_size,
            chroma_size,
        })
    }

    fn frame_size(&self) -> usize {
        self.y_size + self.chroma_size * 2
    }

    // 次の 1 フレームを読み込む。ファイル終端に達していれば `None` を返す
    //
    // フレーム境界の途中でファイルが終わっている場合はエラーとする。
    pub fn read_frame(&mut self) -> crate::Result<Option<YuvFrame>> {
        let frame_size = self.frame_size();
        // フレームサイズは解像度から決まる固定値であり、入力データ由来のサイズ値ではないため
        // 事前確保しても破損データによるメモリ暴走のリスクはない
        let mut data = vec![0u8; frame_size];
        let mut filled = 0;
        while filled < frame_size {
            let read_size = self.file.read(&mut data[filled..])?;
            if read_size == 0 {
                break;
            }
            filled += read_size;
        }
        if filled == 0 {
            return Ok(None);
        }
        if filled != frame_size {
            return Err(crate::Error::new(format!(
                "YUV file size is not a multiple of the frame size {frame_size} (trailing {filled} bytes)"
            )));
        }
        Ok(Some(YuvFrame {
            data,
            y_size: self.y_size,
            chroma_size: self.chroma_size,
        }))
    }
}
