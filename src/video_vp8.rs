//! VP8 フレームから `SampleEntry` を構築する

use shiguredo_mp4::{
    bitstream::vp8::{Vp8SampleEntryConfig, build_vp08_box, parse_frame_header},
    boxes::SampleEntry,
};

/// VP8 のキーフレームバイト列から `SampleEntry::Vp08` を構築する
///
/// interframe (`header.keyframe == None`) ではエラーを返す。
/// visual 寸法はキーフレーム header の width / height を使う。
pub fn vp8_sample_entry_from_frame(data: &[u8]) -> crate::Result<SampleEntry> {
    let header = parse_frame_header(data)?;
    let key = header.keyframe.ok_or_else(|| {
        crate::Error::new("VP8 sample entry requires a keyframe (interframe has no keyframe info)")
    })?;
    let config = Vp8SampleEntryConfig {
        video_full_range_flag: false,
        colour_primaries: Vp8SampleEntryConfig::COLOUR_PRIMARIES_BT709,
        transfer_characteristics: Vp8SampleEntryConfig::TRANSFER_CHARACTERISTICS_BT709,
        matrix_coefficients: Vp8SampleEntryConfig::MATRIX_COEFFICIENTS_BT709,
        width: key.width,
        height: key.height,
    };
    Ok(SampleEntry::Vp08(build_vp08_box(&config)))
}
