use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::Duration;

use crate::audio::AudioData;
use crate::video::VideoFrame;

// 内部用の識別子
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MediaStreamId(u64);

impl MediaStreamId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub fn fetch_add(&mut self, n: u64) -> Self {
        let id = *self;
        self.0 += n;
        id
    }
}

impl nojson::DisplayJson for MediaStreamId {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.value(self.0)
    }
}

impl<'text, 'raw> TryFrom<nojson::RawJsonValue<'text, 'raw>> for MediaStreamId {
    type Error = nojson::JsonParseError;

    fn try_from(value: nojson::RawJsonValue<'text, 'raw>) -> Result<Self, Self::Error> {
        value.try_into().map(Self)
    }
}

#[derive(Debug, Clone)]
pub enum MediaSample {
    Audio(Arc<AudioData>),
    Video(Arc<VideoFrame>),
}

impl MediaSample {
    pub fn timestamp(&self) -> Duration {
        match self {
            Self::Audio(x) => x.timestamp,
            Self::Video(x) => x.timestamp,
        }
    }

    pub fn expect_audio_data(self) -> crate::Result<Arc<AudioData>> {
        if let Self::Audio(sample) = self {
            Ok(sample)
        } else {
            Err(crate::Error::new(
                "expected an audio sample, but got a video sample",
            ))
        }
    }

    pub fn expect_video_frame(self) -> crate::Result<Arc<VideoFrame>> {
        if let Self::Video(frame) = self {
            Ok(frame)
        } else {
            Err(crate::Error::new(
                "expected a video sample, but got an audio sample",
            ))
        }
    }

    pub fn audio_data(data: AudioData) -> Self {
        Self::Audio(Arc::new(data))
    }

    pub fn video_frame(frame: VideoFrame) -> Self {
        Self::Video(Arc::new(frame))
    }
}

pub type MediaStreamReceiver = Receiver<MediaSample>;
pub type MediaStreamSyncSender = SyncSender<MediaSample>;
