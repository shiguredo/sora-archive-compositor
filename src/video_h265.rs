use shiguredo_mp4::{
    bitstream::h265::{
        self, H265ConstantFrameRate, H265SampleEntryConfig, build_hvc1_box,
        build_hvc1_box_from_annexb,
    },
    boxes::SampleEntry,
};

use crate::video::FrameRate;

pub type NalUnitArray = Vec<Vec<u8>>;

// H.265 の NAL ユニット前に付与されるサイズのバイト数
// Sora / Hisui が生成するものは全て 4 バイトなので固定値でいい（H.264 と同様）
pub use crate::video_h264::NALU_HEADER_LENGTH;

// H.265 の NAL ユニットタイプ
pub const H265_NALU_TYPE_VPS: u8 = 32;
pub const H265_NALU_TYPE_SPS: u8 = 33;
pub const H265_NALU_TYPE_PPS: u8 = 34;

fn h265_sample_entry_config(fps: FrameRate) -> H265SampleEntryConfig {
    let fps_per_second = fps.numerator.get().div_ceil(fps.denominator.get());
    // ISO/IEC 14496-15 の avgFrameRate は 256 秒あたりのフレーム数
    let avg_frame_rate = fps_per_second
        .checked_mul(256)
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(H265SampleEntryConfig::AVG_FRAME_RATE_UNSPECIFIED);
    H265SampleEntryConfig {
        length_size: crate::video_h264::h264_length_size(),
        avg_frame_rate,
        constant_frame_rate: H265ConstantFrameRate::Constant,
    }
}

/// VPS / SPS / PPS リストから `SampleEntry::Hvc1` を構築する
pub fn h265_sample_entry(
    _width: usize,
    _height: usize,
    fps: FrameRate,
    vps_list: NalUnitArray,
    sps_list: NalUnitArray,
    pps_list: NalUnitArray,
) -> crate::Result<SampleEntry> {
    // Apple 系プレイヤーは hev1 を再生できず hvc1 のみをサポートするため、
    // 出力は hvc1 に統一する（hev1 / hvc1 のフィールド構成は同一）
    let hvc1 = build_hvc1_box(
        &vps_list,
        &sps_list,
        &pps_list,
        &h265_sample_entry_config(fps),
    )?;
    Ok(SampleEntry::Hvc1(hvc1))
}

/// Annex B 形式の H.265 データから VPS, SPS, PPS を抽出して sample entry を生成する
pub fn h265_sample_entry_from_annexb(
    _width: usize,
    _height: usize,
    fps: FrameRate,
    data: &[u8],
) -> crate::Result<SampleEntry> {
    let hvc1 = build_hvc1_box_from_annexb(data, &h265_sample_entry_config(fps))?;
    Ok(SampleEntry::Hvc1(hvc1))
}

/// Annex B 形式の H.265 を length-prefixed 形式へ変換する
pub fn h265_annexb_to_length_prefixed(data: &[u8]) -> crate::Result<Vec<u8>> {
    Ok(h265::annexb_to_length_prefixed(
        data,
        crate::video_h264::h264_length_size(),
    )?)
}

/// length-prefixed 形式の H.265 を Annex B 形式へ変換する
pub fn h265_length_prefixed_to_annexb(data: &[u8]) -> crate::Result<Vec<u8>> {
    Ok(h265::length_prefixed_to_annexb(
        data,
        crate::video_h264::h264_length_size(),
    )?)
}
