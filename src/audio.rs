use std::time::Duration;

use shiguredo_mp4::{
    FixedPointNumber,
    boxes::{AudioSampleEntryFields, SampleEntry},
};

use crate::{metadata::SourceId, types::CodecName};

// 現時点では 48000 固定
pub const SAMPLE_RATE: u16 = 48000;

// 現時点ではステレオ固定
pub const CHANNELS: u16 = 2;

// エンコードパラメーターのデフォルト値
pub const DEFAULT_BITRATE: usize = 65536;

#[derive(Debug, Clone)]
pub struct AudioData {
    pub source_id: Option<SourceId>,
    pub data: Vec<u8>,
    pub format: AudioFormat,
    pub stereo: bool,
    pub sample_rate: u16,
    pub timestamp: Duration,
    pub duration: Duration,
    pub sample_entry: Option<SampleEntry>,
}

impl AudioData {
    pub fn stereo_samples(&self) -> crate::Result<impl '_ + Iterator<Item = (i16, i16)>> {
        if self.format != AudioFormat::I16Be {
            return Err(crate::Error::new(format!(
                "expected I16Be audio format, got {:?}",
                self.format
            )));
        }
        if !self.stereo {
            return Err(crate::Error::new("expected stereo audio data"));
        }

        let samples = self.data.as_chunks::<4>().0.iter().map(|c| {
            (
                i16::from_be_bytes([c[0], c[1]]),
                i16::from_be_bytes([c[2], c[3]]),
            )
        });
        Ok(samples)
    }

    pub fn interleaved_stereo_samples(&self) -> crate::Result<impl '_ + Iterator<Item = i16>> {
        if self.format != AudioFormat::I16Be {
            return Err(crate::Error::new(format!(
                "expected I16Be audio format, got {:?}",
                self.format
            )));
        }
        if !self.stereo {
            return Err(crate::Error::new("expected stereo audio data"));
        }

        let samples = self.data.as_chunks::<4>().0.iter().flat_map(|c| {
            [
                i16::from_be_bytes([c[0], c[1]]),
                i16::from_be_bytes([c[2], c[3]]),
            ]
            .into_iter()
        });
        Ok(samples)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    I16Be,
    Opus,
    Aac,
}

impl AudioFormat {
    pub fn codec_name(self) -> Option<CodecName> {
        match self {
            AudioFormat::I16Be => None,
            AudioFormat::Opus => Some(CodecName::Opus),
            AudioFormat::Aac => Some(CodecName::Aac),
        }
    }
}

impl std::fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = self.codec_name().map(|n| n.as_str()).unwrap_or("PCM");
        write!(f, "{name}")
    }
}

pub fn sample_entry_audio_fields() -> AudioSampleEntryFields {
    AudioSampleEntryFields {
        data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
        channelcount: CHANNELS,
        samplesize: 16,
        samplerate: FixedPointNumber::new(SAMPLE_RATE, 0),
    }
}
