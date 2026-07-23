//! VP9 フレームから `SampleEntry` を構築する

use shiguredo_mp4::{
    bitstream::vp9::{Vp9FrameSize, Vp9SampleEntryConfig, build_vp09_box, parse_frame_header},
    boxes::SampleEntry,
};

/// VP9 のフレームバイト列から `SampleEntry::Vp09` を構築する
///
/// visual 寸法は `frame_size` が `Resolved` のときその値を使う。
/// `build_vp09_box` が要求する key / `intra_only` 以外はライブラリ側でエラーになる。
pub fn vp9_sample_entry_from_frame(data: &[u8]) -> crate::Result<SampleEntry> {
    let header = parse_frame_header(data)?;
    let (width, height) = match header.frame_size {
        Vp9FrameSize::Resolved { width, height } => (width, height),
        Vp9FrameSize::NotPresent | Vp9FrameSize::UsesRefFrames { .. } => {
            return Err(crate::Error::new(
                "VP9 sample entry requires Resolved frame_size in the frame header",
            ));
        }
    };
    let width = u16::try_from(width).map_err(|_| {
        crate::Error::new(format!(
            "VP9 frame width exceeds VisualSampleEntry u16 limit: {width}"
        ))
    })?;
    let height = u16::try_from(height).map_err(|_| {
        crate::Error::new(format!(
            "VP9 frame height exceeds VisualSampleEntry u16 limit: {height}"
        ))
    })?;
    let config = Vp9SampleEntryConfig {
        level: Vp9SampleEntryConfig::LEVEL_UNDEFINED,
        colour_primaries: Vp9SampleEntryConfig::COLOUR_PRIMARIES_BT709,
        transfer_characteristics: Vp9SampleEntryConfig::TRANSFER_CHARACTERISTICS_BT709,
        matrix_coefficients: Vp9SampleEntryConfig::MATRIX_COEFFICIENTS_BT709,
        width,
        height,
    };
    let vp09 = build_vp09_box(&header, &config)?;
    Ok(SampleEntry::Vp09(vp09))
}
