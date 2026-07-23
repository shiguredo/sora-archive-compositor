use std::{
    collections::VecDeque,
    fs::File,
    io::{BufWriter, Seek, SeekFrom, Write},
    num::NonZeroU32,
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime},
};

use shiguredo_mp4::{
    BoxSize, BoxType, Either, Encode, FixedPointNumber, LanguageCode, Mp4FileTime, Utf8String,
    boxes::{
        Brand, Co64Box, DinfBox, FreeBox, FtypBox, HdlrBox, MdhdBox, MdiaBox, MediaHeader, MinfBox,
        MoovBox, MvhdBox, SampleEntry, SmhdBox, StblBox, StcoBox, StscBox, StscEntry, StsdBox,
        StssBox, StszBox, SttsBox, TkhdBox, TrakBox, UnknownBox, VmhdBox,
    },
};

use crate::{
    audio::AudioData,
    layout::{Layout, Resolution},
    media::{MediaSample, MediaStreamId},
    mixer_audio::MIXED_AUDIO_DATA_DURATION,
    processor::{
        MediaProcessor, MediaProcessorInput, MediaProcessorOutput, MediaProcessorSpec,
        MediaProcessorWorkloadHint,
    },
    stats::{Mp4WriterStats, ProcessorStats},
    video::{FrameRate, VideoFrame, video_frame_duration_fits},
};

// Hisui では出力 MP4 のタイムスケールはマイクロ秒固定にする
const TIMESCALE: NonZeroU32 = NonZeroU32::MIN.saturating_add(1_000_000 - 1);

// 映像・音声混在時のチャンクの尺の最大値（映像か音声の片方だけの場合はチャンクは一つだけ）
const MAX_CHUNK_DURATION: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct Mp4WriterOptions {
    pub resolution: Resolution,
    pub duration: Duration,
    pub frame_rate: FrameRate,
}

impl Mp4WriterOptions {
    pub fn from_layout(layout: &Layout) -> Self {
        Self {
            resolution: layout.resolution,
            duration: layout.duration(),
            frame_rate: layout.frame_rate,
        }
    }
}

/// 合成結果を含んだ MP4 ファイルを書き出すための構造体
#[derive(Debug)]
pub struct Mp4Writer {
    file: BufWriter<File>,
    file_size: u64,
    resolution: Resolution,
    moov_box_offset: u64,
    mdat_box_offset: u64,
    audio_chunks: Vec<Chunk>,
    video_chunks: Vec<Chunk>,
    audio_sample_entry: Option<SampleEntry>,
    video_sample_entries: Vec<SampleEntry>,
    input_audio_stream_id: Option<MediaStreamId>,
    input_video_stream_id: Option<MediaStreamId>,
    input_audio_queue: VecDeque<Arc<AudioData>>,
    input_video_queue: VecDeque<Arc<VideoFrame>>,
    finalize_time: Mp4FileTime,
    appending_video_chunk: bool,
    stats: Mp4WriterStats,
}

impl Mp4Writer {
    /// [`Mp4Writer`] インスタンスを生成する
    pub fn new<P: AsRef<Path>>(
        path: P,
        options: &Mp4WriterOptions,
        input_audio_stream_id: Option<MediaStreamId>,
        input_video_stream_id: Option<MediaStreamId>,
    ) -> crate::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        let mut this = Self {
            file: BufWriter::new(file),
            file_size: 0,
            resolution: options.resolution,
            moov_box_offset: 0,
            mdat_box_offset: 0,
            audio_chunks: Vec::new(),
            video_chunks: Vec::new(),
            audio_sample_entry: None,
            video_sample_entries: Vec::new(),
            finalize_time: Mp4FileTime::from_unix_time(Duration::ZERO),
            input_audio_stream_id,
            input_video_stream_id,
            input_audio_queue: VecDeque::new(),
            input_video_queue: VecDeque::new(),
            appending_video_chunk: true,
            stats: Mp4WriterStats::default(),
        };
        this.init(options)?;

        Ok(this)
    }

    /// 統計情報を返す
    pub fn stats(&self) -> &Mp4WriterStats {
        &self.stats
    }

    fn handle_next_audio_and_video(
        &mut self,
        audio_timestamp: Option<Duration>,
        video_timestamp: Option<Duration>,
    ) -> crate::Result<bool> {
        match (audio_timestamp, video_timestamp){
              (None, None) => {
                // 全部の入力の処理が完了した
                self.finalize()?;
                return Ok(false);
            }
            (None, Some(_)) => {
                // 残りは映像のみ
                let new_chunk = self.video_chunks.len() == self.audio_chunks.len();
                self.append_video_frame(new_chunk)?;
            }
            (Some(_), None) => {
                // 残りは音声のみ
                let new_chunk = self.audio_chunks.is_empty()
                    || self.video_chunks.len() > self.audio_chunks.len();
                self.append_audio_data(new_chunk)?;
            }
            (Some(audio_timestamp), Some(video_timestamp))
                if
                // 音声が一定以上遅れている場合は映像に追従する
                (self.appending_video_chunk && video_timestamp.saturating_sub(audio_timestamp) > MAX_CHUNK_DURATION)
                ||
                // 一度音声追記モードに入った場合には、映像に追いつくまでは音声を追記し続ける
                (!self.appending_video_chunk && video_timestamp > audio_timestamp) =>
            {
                let new_chunk = self.video_chunks.len() > self.audio_chunks.len();
                self.append_audio_data(new_chunk)?;
            }
            (Some(_), Some(_)) => {
                // 音声との差が一定以内の場合は、映像の処理を進める
                let new_chunk = self.video_chunks.len() == self.audio_chunks.len();
                self.append_video_frame(new_chunk)?;
            }
        }

        Ok(true)
    }

    pub fn current_duration(&self) -> Duration {
        self.stats
            .total_audio_track_duration
            .get()
            .max(self.stats.total_video_track_duration.get())
    }

    fn append_video_frame(&mut self, new_chunk: bool) -> crate::Result<()> {
        // 次の入力を取り出す（これは常に成功する）
        let frame = self
            .input_video_queue
            .pop_front()
            .ok_or_else(|| crate::Error::new("queue is empty"))?;

        if self.stats.video_codec.get().is_none()
            && let Some(name) = frame.format.codec_name()
        {
            self.stats.video_codec.set(name);
        }

        // サンプルエントリーはエンコーダーが生成する。通常は最初のフレームに 1 回だけ
        // 載るが、途中で解像度が変わる場合には新しいエントリーが載ったフレームが来る。
        // 新しいエントリーが来たときは、そのフレームから新しいチャンクを始めることで、
        // stsc の sample_description_index をチャンク単位で切り替えられるようにする。
        let mut new_chunk = new_chunk;
        if let Some(entry) = &frame.sample_entry {
            if self.video_sample_entries.is_empty() {
                // 最初のフレームでサンプルエントリーが無い場合はエラー
                self.video_sample_entries.push(entry.clone());
            } else if self.video_sample_entries.last() != Some(entry) {
                // 前のサンプルエントリーと異なる場合は、新しいエントリーとして登録して
                // チャンクを強制的に分割する
                self.video_sample_entries.push(entry.clone());
                new_chunk = true;
            }
        } else if self.video_sample_entries.is_empty() {
            return Err(crate::Error::new(
                "first video frame is missing sample entry",
            ));
        }

        // 必要に応じて新しいチャンクを始める
        if new_chunk {
            self.video_chunks.push(Chunk {
                offset: self.file_size,
                samples: Vec::new(),
                // sample_description_index は 1 始まりの stsd エントリー番号
                sample_description_index: NonZeroU32::MIN
                    .saturating_add(self.video_sample_entries.len() as u32 - 1),
            });
            self.stats.total_video_chunk_count.add(1);
        }

        // 一番最後に moov ボックスを構築するためのメタデータを覚えておく
        let sample = Sample {
            keyframe: frame.keyframe,
            size: frame.data.len() as u32,
            duration: duration_to_stts_sample_delta(frame.duration)?,
        };
        self.video_chunks
            .last_mut()
            .ok_or_else(|| crate::Error::new("no video chunk to append sample"))?
            .samples
            .push(sample);
        self.stats.total_video_sample_count.add(1);

        // mdat ボックスにデータを追記する
        self.file.write_all(&frame.data)?;
        self.file_size += frame.data.len() as u64;
        self.stats
            .total_video_sample_data_byte_size
            .add(frame.data.len() as u64);

        self.stats.total_video_track_duration.add(frame.duration);
        self.appending_video_chunk = true;
        Ok(())
    }

    fn append_audio_data(&mut self, new_chunk: bool) -> crate::Result<()> {
        // 次の入力を取り出す（これは常に成功する）
        let data = self
            .input_audio_queue
            .pop_front()
            .ok_or_else(|| crate::Error::new("queue is empty"))?;

        if self.stats.audio_codec.get().is_none()
            && let Some(name) = data.format.codec_name()
        {
            self.stats.audio_codec.set(name);
        }

        // Hisui では途中でエンコード情報が変わることがないので、
        // サンプルエントリーは最初に一回だけ存在する
        if self.audio_sample_entry.is_none() {
            if data.sample_entry.is_none() {
                return Err(crate::Error::new(
                    "first audio data is missing sample entry",
                ));
            }
            self.audio_sample_entry = data.sample_entry.clone();
        } else if data.sample_entry.is_some() {
            return Err(crate::Error::new(
                "unexpected sample entry after the first audio data",
            ));
        }

        // 必要に応じて新しいチャンクを始める
        if new_chunk {
            self.audio_chunks.push(Chunk {
                offset: self.file_size,
                samples: Vec::new(),
                // 音声はサンプルエントリーが 1 つだけのため、番号は常に 1
                sample_description_index: NonZeroU32::MIN,
            });
            self.stats.total_audio_chunk_count.add(1);
        }

        // 一番最後に moov ボックスを構築するためのメタデータを覚えておく
        let sample = Sample {
            keyframe: true,
            size: data.data.len() as u32,
            duration: duration_to_stts_sample_delta(data.duration)?,
        };
        self.audio_chunks
            .last_mut()
            .ok_or_else(|| crate::Error::new("no audio chunk to append sample"))?
            .samples
            .push(sample);
        self.stats.total_audio_sample_count.add(1);

        // mdat ボックスにデータを追記する
        self.file.write_all(&data.data)?;
        self.file_size += data.data.len() as u64;
        self.stats
            .total_audio_sample_data_byte_size
            .add(data.data.len() as u64);

        self.stats.total_audio_track_duration.add(data.duration);
        self.appending_video_chunk = false;
        Ok(())
    }

    fn finalize(&mut self) -> crate::Result<()> {
        self.finalize_time = Mp4FileTime::from_unix_time(SystemTime::UNIX_EPOCH.elapsed()?);

        // 確定した moov ボックスの内容で事前に確保しておいた free ボックスの
        // 領域を上書きする
        let moov_box = self.build_moov_box()?;

        // 2026.3.0 以降の低レベル API では box_size() 相当のメソッドは撤廃されているので、
        // エンコード結果のバイト長からサイズを求める
        let moov_box_bytes = moov_box.encode_to_vec()?;
        let moov_box_size = moov_box_bytes.len() as u64;
        let free_box_min_size = 8;
        let reserved_size = self.mdat_box_offset - self.moov_box_offset;
        self.stats
            .actual_moov_box_size
            .set(moov_box_size + free_box_min_size);
        if moov_box_size + free_box_min_size >= reserved_size {
            return Err(crate::Error::new(
                "assertion failed: moov_box_size + free_box_min_size < reserved_size",
            ));
        }

        self.file.seek(SeekFrom::Start(self.moov_box_offset))?;
        self.file.write_all(&moov_box_bytes)?;

        let free_box_payload_size =
            self.mdat_box_offset - (self.moov_box_offset + moov_box_size) - 8;
        let free_box = FreeBox {
            payload: vec![0; free_box_payload_size as usize],
        };
        let free_box_bytes = free_box.encode_to_vec()?;
        self.file.write_all(&free_box_bytes)?;

        // [NOTE]
        // 特に支障はないはずなので mdat ボックスは可変長サイズ扱いのままにしておく
        // (もし問題があるようなら、ここで確定したサイズに上書きする)

        self.file.flush()?;
        Ok(())
    }

    fn build_moov_box(&self) -> crate::Result<MoovBox> {
        let mut trak_boxes = Vec::new();
        if !self.audio_chunks.is_empty() {
            let track_id = trak_boxes.len() as u32 + 1;
            trak_boxes.push(self.build_audio_trak_box(track_id)?);
        }
        if !self.video_chunks.is_empty() {
            let track_id = trak_boxes.len() as u32 + 1;
            trak_boxes.push(self.build_video_trak_box(track_id)?);
        }

        let mvhd_box = MvhdBox {
            creation_time: self.finalize_time,
            modification_time: self.finalize_time,
            timescale: TIMESCALE,
            duration: self.current_duration().as_micros() as u64,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: trak_boxes.len() as u32 + 1,
        };

        Ok(MoovBox {
            mvhd_box,
            trak_boxes,
            // sora-archive-compositor は通常 MP4 のみを扱うため mvex ボックスは常に無し
            mvex_box: None,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_audio_trak_box(&self, track_id: u32) -> crate::Result<TrakBox> {
        let tkhd_box = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time: self.finalize_time,
            modification_time: self.finalize_time,
            track_id,
            duration: self.stats.total_audio_track_duration.get().as_micros() as u64,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: TkhdBox::DEFAULT_AUDIO_VOLUME,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: FixedPointNumber::default(),
            height: FixedPointNumber::default(),
        };
        Ok(TrakBox {
            tkhd_box,
            edts_box: None,
            mdia_box: self.build_audio_mdia_box()?,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_video_trak_box(&self, track_id: u32) -> crate::Result<TrakBox> {
        let tkhd_box = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time: self.finalize_time,
            modification_time: self.finalize_time,
            track_id,
            duration: self.stats.total_video_track_duration.get().as_micros() as u64,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: TkhdBox::DEFAULT_VIDEO_VOLUME,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: FixedPointNumber::new(self.resolution.width().get() as i16, 0),
            height: FixedPointNumber::new(self.resolution.height().get() as i16, 0),
        };
        Ok(TrakBox {
            tkhd_box,
            edts_box: None,
            mdia_box: self.build_video_mdia_box()?,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_audio_mdia_box(&self) -> crate::Result<MdiaBox> {
        let sample_entry = self
            .audio_sample_entry
            .as_ref()
            .ok_or_else(|| crate::Error::new("audio sample entry is not set"))?;
        let mdhd_box = MdhdBox {
            creation_time: self.finalize_time,
            modification_time: self.finalize_time,
            timescale: TIMESCALE,
            duration: self.stats.total_audio_track_duration.get().as_micros() as u64,
            language: LanguageCode::UNDEFINED,
        };
        let hdlr_box = HdlrBox {
            handler_type: HdlrBox::HANDLER_TYPE_SOUN,
            name: Utf8String::EMPTY.into_null_terminated_bytes(),
        };
        let minf_box = MinfBox {
            media_header: Some(MediaHeader::Smhd(SmhdBox::default())),
            dinf_box: DinfBox::LOCAL_FILE,
            stbl_box: self
                .build_stbl_box(std::slice::from_ref(sample_entry), &self.audio_chunks)?,
            unknown_boxes: Vec::new(),
        };
        Ok(MdiaBox {
            mdhd_box,
            hdlr_box,
            minf_box,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_video_mdia_box(&self) -> crate::Result<MdiaBox> {
        if self.video_sample_entries.is_empty() {
            return Err(crate::Error::new("video sample entry is not set"));
        }
        let mdhd_box = MdhdBox {
            creation_time: self.finalize_time,
            modification_time: self.finalize_time,
            timescale: TIMESCALE,
            duration: self.stats.total_video_track_duration.get().as_micros() as u64,
            language: LanguageCode::UNDEFINED,
        };
        let hdlr_box = HdlrBox {
            handler_type: HdlrBox::HANDLER_TYPE_VIDE,
            name: Utf8String::EMPTY.into_null_terminated_bytes(),
        };
        let minf_box = MinfBox {
            media_header: Some(MediaHeader::Vmhd(VmhdBox::default())),
            dinf_box: DinfBox::LOCAL_FILE,
            stbl_box: self.build_stbl_box(&self.video_sample_entries, &self.video_chunks)?,
            unknown_boxes: Vec::new(),
        };
        Ok(MdiaBox {
            mdhd_box,
            hdlr_box,
            minf_box,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_stbl_box(
        &self,
        sample_entries: &[SampleEntry],
        chunks: &[Chunk],
    ) -> crate::Result<StblBox> {
        let stsd_box = StsdBox {
            entries: sample_entries.to_vec(),
        };

        let stts_box = SttsBox::from_sample_deltas(
            chunks
                .iter()
                .flat_map(|c| c.samples.iter().map(|s| s.duration)),
        )?;

        let stsc_box = StscBox {
            entries: chunks
                .iter()
                .enumerate()
                .map(|(i, c)| StscEntry {
                    first_chunk: NonZeroU32::MIN.saturating_add(i as u32),
                    sample_per_chunk: c.samples.len() as u32,
                    sample_description_index: c.sample_description_index,
                })
                .collect(),
        };

        let stsz_box = StszBox::Variable {
            entry_sizes: chunks
                .iter()
                .flat_map(|s| s.samples.iter().map(|s| s.size))
                .collect(),
        };

        let stco_or_co64_box = if self.file_size > u32::MAX as u64 {
            Either::B(Co64Box {
                chunk_offsets: chunks.iter().map(|c| c.offset).collect(),
            })
        } else {
            Either::A(StcoBox {
                chunk_offsets: chunks.iter().map(|c| c.offset as u32).collect(),
            })
        };

        let is_all_keyframe = chunks.iter().all(|c| c.samples.iter().all(|s| s.keyframe));
        let stss_box = if is_all_keyframe {
            None
        } else {
            Some(StssBox {
                sample_numbers: chunks
                    .iter()
                    .flat_map(|c| c.samples.iter())
                    .enumerate()
                    .filter_map(|(i, s)| {
                        s.keyframe
                            .then_some(NonZeroU32::MIN.saturating_add(i as u32))
                    })
                    .collect(),
            })
        };

        Ok(StblBox {
            stsd_box,
            stts_box,
            // Sora 録画合成では映像・音声とも一定尺の PTS を持たせているため ctts / cslg / sdtp は不要
            ctts_box: None,
            cslg_box: None,
            stsc_box,
            stsz_box,
            stco_or_co64_box,
            stss_box,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        })
    }

    // 実際にメディアデータを書き込む前の MP4 ファイルの初期化処理
    fn init(&mut self, options: &Mp4WriterOptions) -> crate::Result<()> {
        // ftyp ボックスを書きこむ
        self.write_ftyp_box()?;

        // 最終的な moov ボックスを保持可能なサイズの free ボックスを書きこむ
        // (先頭付近に moov ボックスを配置することで、動画プレイヤーの再生開始までに掛かる時間を短縮できる)
        self.write_free_box(options)?;

        // 可変長の mdat ボックスのヘッダーを書きこむ
        self.mdat_box_offset = self.file_size;

        // 2026.3.0 系の shiguredo_mp4 では MdatBox は固定長 (payload を持つ) 構造体となり、
        // `is_variable_size` フラグが撤廃されている。低レベル API では BoxHeader::new も
        // クレート外に公開されていないため、ここでは "size = 0 (末尾まで可変長)" を意味する
        // 8 バイトのヘッダーを直接書き出す。0 サイズは MP4 の仕様で「ファイル末尾まで」を表す
        let mdat_variable_header = [0u8, 0, 0, 0, b'm', b'd', b'a', b't'];
        self.file.write_all(&mdat_variable_header)?;

        // [NOTE] 可変サイズの mdat ヘッダーは 8 バイト固定
        self.file_size += mdat_variable_header.len() as u64;

        Ok(())
    }

    fn write_ftyp_box(&mut self) -> crate::Result<()> {
        // Hisui で扱う可能性があるコーデックを全て含んだ互換性ブランドを指定しておく。
        // （もし必要最小限に絞りたくなったら、実際にファイルに含まれるコーデックから動的に生成するようにする）
        let compatible_brands = vec![
            Brand::ISOM,
            Brand::ISO2,
            Brand::MP41,
            Brand::AVC1,
            Brand::AV01,
        ];

        let ftyp_box = FtypBox {
            major_brand: Brand::ISOM,
            minor_version: 0,
            compatible_brands,
        };
        let ftyp_box_bytes = ftyp_box.encode_to_vec()?;
        self.file.write_all(&ftyp_box_bytes)?;
        self.file_size += ftyp_box_bytes.len() as u64;

        Ok(())
    }

    fn write_free_box(&mut self, options: &Mp4WriterOptions) -> crate::Result<()> {
        self.moov_box_offset = self.file_size;

        // faststart 用にダミーの moov を事前に構築する (必要なサイズの計測用)
        // かなり余裕をみた計算方法になっているので、これで足りないことはまずないはず
        let moov_box = self.build_dummy_moov_box(options)?;
        let max_moov_box_size = moov_box.encode_to_vec()?.len() as u64;
        self.stats.reserved_moov_box_size.set(max_moov_box_size);
        tracing::debug!("reserved moov box size: {max_moov_box_size}");

        // 初期化時点では free ボックスで領域だけ確保しておく
        let free_box = FreeBox {
            payload: vec![0; max_moov_box_size as usize],
        };
        let free_box_bytes = free_box.encode_to_vec()?;
        self.file.write_all(&free_box_bytes)?;
        self.file_size += free_box_bytes.len() as u64;
        Ok(())
    }

    fn build_dummy_moov_box(&self, options: &Mp4WriterOptions) -> crate::Result<MoovBox> {
        let mvhd_box = MvhdBox {
            // フィールドの値はなんでもいいのでテキトウに設定しておく
            creation_time: Mp4FileTime::default(),
            modification_time: Mp4FileTime::default(),
            timescale: NonZeroU32::MIN,
            duration: u64::MAX, // ここが 32 bit に収まるかどうかでサイズが変わるので大きい値を指定する
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: 1,
        };

        let duration = options.duration;
        let mut trak_boxes = Vec::new();
        if self.input_audio_stream_id.is_some() {
            let audio_sample_count =
                (duration.as_micros() / MIXED_AUDIO_DATA_DURATION.as_micros()) as usize;
            trak_boxes.push(self.build_dummy_trak_box(audio_sample_count)?);
        }
        if self.input_video_stream_id.is_some() {
            let video_sample_count = duration.as_secs() as usize
                * options.frame_rate.numerator.get()
                / options.frame_rate.denominator.get();
            trak_boxes.push(self.build_dummy_trak_box(video_sample_count)?);
        }

        Ok(MoovBox {
            mvhd_box,
            trak_boxes,
            mvex_box: None,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_dummy_trak_box(&self, sample_count: usize) -> crate::Result<TrakBox> {
        let tkhd_box = TkhdBox {
            // フィールドの値はなんでもいいのでテキトウに設定しておく
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time: Mp4FileTime::default(),
            modification_time: Mp4FileTime::default(),
            track_id: 1,
            duration: u64::MAX, // ここは 32 bit に収まるかどうかでサイズが変わる
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: TkhdBox::DEFAULT_AUDIO_VOLUME,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: FixedPointNumber::default(),
            height: FixedPointNumber::default(),
        };
        Ok(TrakBox {
            tkhd_box,
            edts_box: None,
            mdia_box: self.build_dummy_mdia_box(sample_count)?,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_dummy_mdia_box(&self, sample_count: usize) -> crate::Result<MdiaBox> {
        let mdhd_box = MdhdBox {
            // フィールドの値はなんでもいいのでテキトウに設定しておく
            creation_time: Mp4FileTime::default(),
            modification_time: Mp4FileTime::default(),
            timescale: NonZeroU32::MIN,
            duration: u64::MAX, // ここは 32 bit に収まるかどうかでサイズが変わる
            language: LanguageCode::UNDEFINED,
        };
        let hdlr_box = HdlrBox {
            // 同上（テキトウな固定値でいい）
            handler_type: HdlrBox::HANDLER_TYPE_VIDE,
            name: Utf8String::EMPTY.into_null_terminated_bytes(),
        };
        let minf_box = MinfBox {
            // 同上（テキトウな固定値でいい）
            media_header: Some(MediaHeader::Vmhd(VmhdBox::default())),
            dinf_box: DinfBox::LOCAL_FILE,
            stbl_box: self.build_dummy_stbl_box(sample_count)?,
            unknown_boxes: Vec::new(),
        };
        Ok(MdiaBox {
            mdhd_box,
            hdlr_box,
            minf_box,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_dummy_stbl_box(&self, sample_count: usize) -> crate::Result<StblBox> {
        // Hisui では途中でエンコード情報が変わることはないので
        // サンプルエントリーは常に 1 つとなる
        let sample_entries = vec![SampleEntry::Unknown(UnknownBox {
            box_type: BoxType::Normal(*b"dumy"),
            box_size: BoxSize::U64(u64::MAX),

            // 多めを確保しておく (サンプルエントリーの中身が 4KB を超えることはまずない）
            payload: vec![0; 4096],
        })];
        let stsd_box = StsdBox {
            entries: sample_entries.clone(),
        };

        // 最悪ケースを想定して、全部のサンプルの尺が異なる、という扱いにしておく
        let stts_box = SttsBox::from_sample_deltas(0..sample_count as u32)?;

        // 最悪ケースを想定して、1 つのチャンクに 1 つのサンプルしかない、という扱いにしておく
        let stsc_box = StscBox {
            entries: (0..sample_count as u32)
                .map(|i| StscEntry {
                    first_chunk: NonZeroU32::MIN.saturating_add(i),
                    sample_per_chunk: 1, // チャンク内のサンプル数は 1 固定
                    sample_description_index: NonZeroU32::MIN,
                })
                .collect(),
        };

        // 最悪ケースを想定して、全部のサンプルのサイズが異なる、という扱いにしておく
        let stsz_box = StszBox::Variable {
            entry_sizes: (0..sample_count as u32).collect(),
        };

        // 最悪ケースを想定して、MP4 ファイルのサイズが 4GB を越える、という扱いにしておく
        let co64_box = Co64Box {
            chunk_offsets: (0..sample_count as u64).collect(),
        };

        // 最悪ケースを想定して、全てが同期サンプル(キーフレーム)、という扱いにしておく
        //
        // なお、本来なら、このケースはボックスそのものが不要だが、ここでは、
        // 最大サイズ推定用にあえてボックスを残している
        let stss_box = StssBox {
            sample_numbers: (0..sample_count as u32)
                .map(|i| NonZeroU32::MIN.saturating_add(i))
                .collect(),
        };

        Ok(StblBox {
            stsd_box,
            stts_box,
            // ダミー moov の最大サイズ推定用としては ctts / cslg / sdtp を含めない
            ctts_box: None,
            cslg_box: None,
            stsc_box,
            stsz_box,
            stco_or_co64_box: Either::B(co64_box),
            stss_box: Some(stss_box),
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        })
    }
}

impl MediaProcessor for Mp4Writer {
    fn spec(&self) -> MediaProcessorSpec {
        MediaProcessorSpec {
            input_stream_ids: self
                .input_audio_stream_id
                .into_iter()
                .chain(self.input_video_stream_id)
                .collect(),
            output_stream_ids: Vec::new(),
            stats: ProcessorStats::Mp4Writer(self.stats.clone()),
            workload_hint: MediaProcessorWorkloadHint::WRITER,
        }
    }

    fn process_input(&mut self, input: MediaProcessorInput) -> crate::Result<()> {
        match input.sample {
            Some(MediaSample::Audio(sample))
                if Some(input.stream_id) == self.input_audio_stream_id =>
            {
                self.input_audio_queue.push_back(sample);
            }
            None if Some(input.stream_id) == self.input_audio_stream_id => {
                self.input_audio_stream_id = None;
            }
            Some(MediaSample::Video(sample))
                if Some(input.stream_id) == self.input_video_stream_id =>
            {
                self.input_video_queue.push_back(sample);
            }
            None if Some(input.stream_id) == self.input_video_stream_id => {
                self.input_video_stream_id = None;
            }
            _ => return Err(crate::Error::new("BUG: unexpected input stream")),
        }
        Ok(())
    }

    fn process_output(&mut self) -> crate::Result<MediaProcessorOutput> {
        loop {
            if let Some(id) = self.input_video_stream_id
                && self.input_video_queue.is_empty()
            {
                return Ok(MediaProcessorOutput::pending(id));
            } else if let Some(id) = self.input_audio_stream_id
                && self.input_audio_queue.is_empty()
            {
                return Ok(MediaProcessorOutput::pending(id));
            }

            let audio_timestamp = self.input_audio_queue.front().map(|x| x.timestamp);
            let video_timestamp = self.input_video_queue.front().map(|x| x.timestamp);

            let in_progress = self.handle_next_audio_and_video(audio_timestamp, video_timestamp)?;

            if !in_progress {
                return Ok(MediaProcessorOutput::Finished);
            }
        }
    }
}

#[derive(Debug)]
struct Chunk {
    offset: u64,
    samples: Vec<Sample>,
    sample_description_index: NonZeroU32,
}

#[derive(Debug)]
struct Sample {
    keyframe: bool,
    size: u32,
    duration: u32,
}

/// stts の sample delta (u32 マイクロ秒) に変換する。上限超過時はエラーにする。
pub(crate) fn duration_to_stts_sample_delta(duration: Duration) -> crate::Result<u32> {
    let micros = duration.as_micros();
    if !video_frame_duration_fits(duration) {
        return Err(crate::Error::new(format!(
            "sample duration exceeds stts u32 limit: {micros} microseconds"
        )));
    }
    // `video_frame_duration_fits` 通過後は u32 に収まる
    Ok(micros as u32)
}
