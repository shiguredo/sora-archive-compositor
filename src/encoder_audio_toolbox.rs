use std::{num::NonZeroUsize, time::Duration};

use shiguredo_mp4::{
    Uint,
    boxes::{EsdsBox, Mp4aBox, SampleEntry},
    descriptors::{DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor},
};

use crate::audio::{self, AudioData, AudioFormat, SAMPLE_RATE};

#[derive(Debug)]
pub struct AudioToolboxEncoder {
    inner: shiguredo_audio_toolbox::Encoder,
    sample_entry: Option<SampleEntry>,
    total_encoded_samples: u64,
}

// SAFETY:
// 2026.2.0-canary.0 の `shiguredo_audio_toolbox::Encoder` は
// 「Apple 公式に AudioConverter のスレッド間移動可否が明記されていないため」という理由で
// `Send` を実装していない (crate ソースコメント参照)。
// 本ラッパー `AudioToolboxEncoder` はスケジューラー経由でワーカースレッドへ move された後、
// そのスレッド上でのみアクセスされる (Scheduler が Task を単一スレッドに固定する運用)。
// 複数スレッドから同時に触ることは無く、また move の前後で参照は残らないため、
// `unsafe impl Send` を付けて move だけ許可する。
unsafe impl Send for AudioToolboxEncoder {}

impl AudioToolboxEncoder {
    pub fn new(bitrate: NonZeroUsize) -> crate::Result<Self> {
        // 2026.2.0-canary.0 で Encoder::new は EncoderConfig を引数に取るようになった
        let config = shiguredo_audio_toolbox::EncoderConfig {
            codec: shiguredo_audio_toolbox::EncoderCodec::AacLc,
            sample_rate: SAMPLE_RATE as u32,
            channels: crate::audio::CHANNELS as u8,
            bitrate: Some(bitrate.get() as u32),
            bitrate_control_mode: None,
            codec_quality: None,
            vbr_quality: None,
        };
        let inner = shiguredo_audio_toolbox::Encoder::new(config)?;
        let sample_entry = Some(sample_entry(bitrate));
        Ok(Self {
            inner,
            sample_entry,
            total_encoded_samples: 0,
        })
    }

    pub fn finish(&mut self) -> crate::Result<Option<AudioData>> {
        // 2026.2.0-canary.0 で finish は () を返すようになり、
        // エンコード済みフレームは next_frame() で取り出す方式へ変わった
        self.inner.finish()?;
        if let Some(encoded) = self.inner.next_frame() {
            Ok(Some(self.handle_encoded_frame(encoded)))
        } else {
            Ok(None)
        }
    }

    pub fn encode(&mut self, data: &AudioData) -> crate::Result<Option<AudioData>> {
        if data.format != AudioFormat::I16Be {
            return Err(crate::Error::new(
                "assertion failed: data.format == AudioFormat::I16Be",
            ));
        }
        if !data.stereo {
            return Err(crate::Error::new("expected stereo audio data"));
        }

        let input = data.interleaved_stereo_samples()?.collect::<Vec<_>>();
        // 2026.2.0-canary.0 で encode は () を返し、エンコード済みフレームは next_frame() で取り出す
        self.inner.encode(&input)?;
        let Some(encoded) = self.inner.next_frame() else {
            return Ok(None);
        };
        Ok(Some(self.handle_encoded_frame(encoded)))
    }

    fn handle_encoded_frame(
        &mut self,
        encoded: shiguredo_audio_toolbox::EncodedFrame,
    ) -> AudioData {
        let duration = Duration::from_secs(encoded.samples as u64) / SAMPLE_RATE as u32;
        let timestamp = Duration::from_secs(self.total_encoded_samples) / SAMPLE_RATE as u32;
        self.total_encoded_samples += encoded.samples as u64;

        AudioData {
            // 固定値
            format: AudioFormat::Aac,
            stereo: true,
            sample_rate: SAMPLE_RATE,
            source_id: None,

            // サンプルエントリーは途中で変わらないので、最初に一回だけ載せる
            sample_entry: self.sample_entry.take(),

            // エンコード結果を反映する
            data: encoded.data,
            timestamp,
            duration,
        }
    }
}

fn sample_entry(bitrate: NonZeroUsize) -> SampleEntry {
    SampleEntry::Mp4a(Mp4aBox {
        audio: audio::sample_entry_audio_fields(),
        esds_box: EsdsBox {
            es: EsDescriptor {
                es_id: EsDescriptor::MIN_ES_ID,
                stream_priority: EsDescriptor::LOWEST_STREAM_PRIORITY,
                depends_on_es_id: None,
                url_string: None,
                ocr_es_id: None,
                dec_config_descr: DecoderConfigDescriptor {
                    object_type_indication:
                        DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3,
                    stream_type: DecoderConfigDescriptor::STREAM_TYPE_AUDIO,
                    up_stream: DecoderConfigDescriptor::UP_STREAM_FALSE,
                    // 2026.3.0 で dec_specific_info は Option<DecoderSpecificInfo> になった
                    dec_specific_info: Some(DecoderSpecificInfo {
                        // AAC LC, 48kHz, stereo 用の配列 (ISO_IEC_14496-3)
                        // - 最初の 5 bit: 0b00010 (AAC LC)
                        // - 次の 4 bit: 0b0011 (48kHz を意味する値)
                        // - 次の 4 bit: 0b0010 (ステレオを意味する値)
                        // - 最後の 3 bit: 未使用
                        payload: vec![0x11, 0x90],
                    }),

                    // 以下は適当にそれっぽい値を指定している
                    buffer_size_db: Uint::new(bitrate.get() as u32 / 8), // 1 秒分のバッファサイズ
                    max_bitrate: bitrate.get() as u32 * 2,               // 平均の 2 倍にしておく
                    avg_bitrate: bitrate.get() as u32,
                },
                sl_config_descr: shiguredo_mp4::descriptors::SlConfigDescriptor,
            },
        },
        unknown_boxes: Vec::new(),
    })
}
