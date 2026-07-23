use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    num::NonZeroU32,
    path::Path,
    time::Duration,
};

use shiguredo_mp4::{
    BoxHeader, Decode,
    aux::SampleTableAccessor,
    boxes::{HdlrBox, MoovBox, SampleEntry, StblBox, TrakBox},
};

use crate::{
    audio::{AudioData, AudioFormat},
    metadata::SourceId,
    stats::{Mp4AudioReaderStats, Mp4VideoReaderStats, VideoResolution},
    video::{VideoFormat, VideoFrame},
};

/// 2026.3.0 系 shiguredo_mp4 の低レベル API は `&[u8]` ベースになり、旧 API の
/// `IgnoredBox::decode_or_ignore` に相当するボックス単位のストリーム読み込みが撤廃された。
/// ここではファイル内のトップレベルを走査して、必要な `moov` ボックスだけをメモリに読み込む。
/// mdat 等はスキップして、Sora 録画 MP4 全体をメモリに載せないようにする。
fn find_moov_box<R: Read + Seek>(reader: &mut R) -> crate::Result<MoovBox> {
    // 必ず先頭に ftyp があるので、そこから開始する
    reader.seek(SeekFrom::Start(0))?;

    loop {
        // ボックスヘッダーは通常 8 バイト、拡張サイズ (largesize) 付きの場合は 16 バイト。
        // まず 8 バイト読んで判定し、largesize (size フィールドが 1) なら追加で 8 バイト読む
        let header_start = reader.stream_position()?;
        let mut header_buf = [0u8; 16];
        reader.read_exact(&mut header_buf[..8])?;
        let uses_largesize = header_buf[..4] == [0, 0, 0, 1];
        let header_len = if uses_largesize {
            reader.read_exact(&mut header_buf[8..16])?;
            16
        } else {
            8
        };
        let (header, _) = BoxHeader::decode(&header_buf[..header_len])?;

        if header.box_type == MoovBox::TYPE {
            // ヘッダーを含めたボックス全体を読み込んでデコードする
            let box_total_size = header.box_size.get() as usize;
            if box_total_size < header_len {
                return Err(crate::Error::new("invalid moov box size"));
            }
            let mut buf = vec![0u8; box_total_size];
            buf[..header_len].copy_from_slice(&header_buf[..header_len]);
            reader.read_exact(&mut buf[header_len..])?;
            let (moov, _) = MoovBox::decode(&buf)?;
            return Ok(moov);
        }

        // moov 以外のボックスはヘッダーだけ消費して残りをスキップする。
        // box_size == 0 はファイル末尾まで続くことを意味するので、そこで終了する
        let box_total_size = header.box_size.get();
        if box_total_size == 0 {
            return Err(crate::Error::new("moov box not found before EOF"));
        }
        let next_pos = header_start + box_total_size;
        reader.seek(SeekFrom::Start(next_pos))?;
    }
}

#[derive(Debug)]
pub struct Mp4VideoReader {
    // ビデオトラックが存在しない場合は None になる
    inner: Option<Mp4VideoReaderInner>,
    stats: Mp4VideoReaderStats,
}

impl Mp4VideoReader {
    pub fn new<P: AsRef<Path>>(
        source_id: SourceId,
        path: P,
        stats: Mp4VideoReaderStats,
    ) -> crate::Result<Self> {
        let inner = Mp4VideoReaderInner::new(source_id, path, stats.clone())?;
        Ok(Self { inner, stats })
    }

    pub fn stats(&self) -> &Mp4VideoReaderStats {
        &self.stats
    }
}

impl Iterator for Mp4VideoReader {
    type Item = crate::Result<VideoFrame>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.as_mut()?.next()
    }
}

#[derive(Debug)]
pub struct Mp4VideoReaderInner {
    file: BufReader<File>,
    source_id: SourceId,
    table: SampleTableAccessor<StblBox>,
    timescale: NonZeroU32,
    next_sample_index: NonZeroU32,
    prev_sample_entry: Option<SampleEntry>,
    stats: Mp4VideoReaderStats,
}

impl Mp4VideoReaderInner {
    fn new<P: AsRef<Path>>(
        source_id: SourceId,
        path: P,
        stats: Mp4VideoReaderStats,
    ) -> crate::Result<Option<Self>> {
        let file = File::open(&path).map_err(|e| {
            crate::Error::new(format!("Cannot open file {}: {e}", path.as_ref().display()))
        })?;
        let mut file = BufReader::new(file);
        let Some(trak) = Self::find_trak_box(&mut file)? else {
            return Ok(None);
        };
        let table = SampleTableAccessor::new(trak.mdia_box.minf_box.stbl_box.clone())?;

        file.seek(SeekFrom::Start(0))?;

        Ok(Some(Self {
            file,
            source_id,
            table,
            timescale: trak.mdia_box.mdhd_box.timescale,
            next_sample_index: NonZeroU32::MIN,
            prev_sample_entry: None,
            stats,
        }))
    }

    fn find_trak_box<R: Read + Seek>(reader: &mut R) -> crate::Result<Option<TrakBox>> {
        let moov = find_moov_box(reader)?;
        Ok(moov
            .trak_boxes
            .into_iter()
            .find(|t| t.mdia_box.hdlr_box.handler_type == HdlrBox::HANDLER_TYPE_VIDE))
    }

    fn next_video_frame(&mut self) -> Option<crate::Result<VideoFrame>> {
        let sample = self.table.get_sample(self.next_sample_index)?;
        self.next_sample_index = self.next_sample_index.checked_add(1)?;

        let sample_entry = sample.chunk().sample_entry();
        let (metadata, format) = match sample_entry {
            SampleEntry::Avc1(b) => (&b.visual, VideoFormat::H264),
            SampleEntry::Hev1(b) => (&b.visual, VideoFormat::H265),
            SampleEntry::Hvc1(b) => (&b.visual, VideoFormat::H265),
            SampleEntry::Vp08(b) => (&b.visual, VideoFormat::Vp8),
            SampleEntry::Vp09(b) => (&b.visual, VideoFormat::Vp9),
            SampleEntry::Av01(b) => (&b.visual, VideoFormat::Av1),
            entry => {
                return Some(Err(crate::Error::new(format!(
                    "unsupported sample entry: {entry:?}"
                ))));
            }
        };

        if let Err(e) = self.file.seek(SeekFrom::Start(sample.data_offset())) {
            return Some(Err(e.into()));
        }

        let mut data = vec![0; sample.data_size() as usize];
        if let Err(e) = self.file.read_exact(&mut data) {
            return Some(Err(e.into()));
        }

        let timestamp = Duration::from_secs(sample.timestamp()) / self.timescale.get();
        let duration = Duration::from_secs(sample.duration() as u64) / self.timescale.get();
        let resolution = (metadata.width, metadata.height);

        self.stats.total_sample_count.add(1);
        self.stats.total_track_duration.set(timestamp + duration);
        if self.stats.codec.get().is_none()
            && let Some(name) = format.codec_name()
        {
            self.stats.codec.set(name);
        }
        self.stats.resolutions.insert(VideoResolution {
            width: resolution.0 as usize,
            height: resolution.1 as usize,
        });

        Some(Ok(VideoFrame {
            source_id: Some(self.source_id.clone()),
            sample_entry: if self
                .prev_sample_entry
                .as_ref()
                .is_none_or(|entry| entry != sample_entry)
            {
                self.prev_sample_entry = Some(sample_entry.clone());
                Some(sample_entry.clone())
            } else {
                None
            },
            data,
            format,
            keyframe: sample.is_sync_sample(),
            width: metadata.width as usize,
            height: metadata.height as usize,
            timestamp,
            duration,
        }))
    }
}

impl Iterator for Mp4VideoReaderInner {
    type Item = crate::Result<VideoFrame>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_video_frame()
    }
}

#[derive(Debug)]
pub struct Mp4AudioReader {
    // 音声トラックが存在しない場合は None になる
    inner: Option<Mp4AudioReaderInner>,
    stats: Mp4AudioReaderStats,
}

impl Mp4AudioReader {
    pub fn new<P: AsRef<Path>>(
        source_id: SourceId,
        path: P,
        stats: Mp4AudioReaderStats,
    ) -> crate::Result<Self> {
        let inner = Mp4AudioReaderInner::new(source_id, path, stats.clone())?;
        Ok(Self { inner, stats })
    }

    pub fn stats(&self) -> &Mp4AudioReaderStats {
        &self.stats
    }
}

impl Iterator for Mp4AudioReader {
    type Item = crate::Result<AudioData>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.as_mut()?.next()
    }
}

#[derive(Debug)]
pub struct Mp4AudioReaderInner {
    file: BufReader<File>,
    source_id: SourceId,
    table: SampleTableAccessor<StblBox>,
    timescale: NonZeroU32,
    next_sample_index: NonZeroU32,
    stats: Mp4AudioReaderStats,
}

impl Mp4AudioReaderInner {
    fn new<P: AsRef<Path>>(
        source_id: SourceId,
        path: P,
        stats: Mp4AudioReaderStats,
    ) -> crate::Result<Option<Self>> {
        let file = File::open(&path).map_err(|e| {
            crate::Error::new(format!("Cannot open file {}: {e}", path.as_ref().display()))
        })?;
        let mut file = BufReader::new(file);
        let Some(trak) = Self::find_trak_box(&mut file)? else {
            return Ok(None);
        };
        let table = SampleTableAccessor::new(trak.mdia_box.minf_box.stbl_box.clone())?;

        file.seek(SeekFrom::Start(0))?;

        Ok(Some(Self {
            source_id,
            file,
            table,
            timescale: trak.mdia_box.mdhd_box.timescale,
            next_sample_index: NonZeroU32::MIN,
            stats,
        }))
    }

    fn find_trak_box<R: Read + Seek>(reader: &mut R) -> crate::Result<Option<TrakBox>> {
        let moov = find_moov_box(reader)?;
        Ok(moov
            .trak_boxes
            .into_iter()
            .find(|t| t.mdia_box.hdlr_box.handler_type == HdlrBox::HANDLER_TYPE_SOUN))
    }

    fn next_audio_data(&mut self) -> Option<crate::Result<AudioData>> {
        let sample = self.table.get_sample(self.next_sample_index)?;
        self.next_sample_index = self.next_sample_index.checked_add(1)?;

        let sample_entry = sample.chunk().sample_entry();
        let (metadata, format) = match &sample_entry {
            SampleEntry::Opus(b) => (&b.audio, AudioFormat::Opus),
            entry => {
                return Some(Err(crate::Error::new(format!(
                    "unsupported sample entry: {entry:?}"
                ))));
            }
        };

        if let Err(e) = self.file.seek(SeekFrom::Start(sample.data_offset())) {
            return Some(Err(e.into()));
        }

        let mut data = vec![0; sample.data_size() as usize];
        if let Err(e) = self.file.read_exact(&mut data) {
            return Some(Err(e.into()));
        }

        let timestamp = Duration::from_secs(sample.timestamp()) / self.timescale.get();
        let duration = Duration::from_secs(sample.duration() as u64) / self.timescale.get();

        self.stats.total_sample_count.add(1);
        self.stats.total_track_duration.set(timestamp + duration);

        Some(Ok(AudioData {
            source_id: Some(self.source_id.clone()),
            data,
            format,
            sample_entry: Some(sample_entry.clone()),

            // [NOTE]
            // 一応、コンテナで指定された値を設定しているけど、
            // ここの値はあまり信用できないので、`AudioData` 処理側は、
            // 実際のペイロードの値を参照する想定
            stereo: metadata.channelcount != 1,

            sample_rate: metadata.samplerate.integer,
            timestamp,
            duration,
        }))
    }
}

impl Iterator for Mp4AudioReaderInner {
    type Item = crate::Result<AudioData>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_audio_data()
    }
}
