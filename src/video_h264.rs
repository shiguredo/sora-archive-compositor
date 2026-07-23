use shiguredo_mp4::{
    bitstream::h264::{
        self, H264NalUnitType, H264SampleEntryConfig, LengthSize, build_avc1_box,
        build_avc1_box_from_annexb, parse_annexb_nal_units,
    },
    boxes::SampleEntry,
};

// H.264 の NAL ユニット前に付与されるサイズのバイト数
// Sora / Hisui が生成するものは全て 4 バイトなので固定値でいい
pub const NALU_HEADER_LENGTH: usize = 4;

// H.264 の NAL ユニットタイプ
pub const H264_NALU_TYPE_IDR: u8 = 5;
pub const H264_NALU_TYPE_SEI: u8 = 6;
pub const H264_NALU_TYPE_SPS: u8 = 7;
pub const H264_NALU_TYPE_PPS: u8 = 8;

/// sample entry 構築時に使う NAL 長フィールド幅
pub fn h264_length_size() -> LengthSize {
    LengthSize::FourBytes
}

fn h264_sample_entry_config() -> H264SampleEntryConfig {
    H264SampleEntryConfig {
        length_size: h264_length_size(),
    }
}

fn h264_nal_unit_type_to_u8(nal_unit_type: H264NalUnitType) -> u8 {
    match nal_unit_type {
        H264NalUnitType::NonIdrSlice => 1,
        H264NalUnitType::IdrSlice => H264_NALU_TYPE_IDR,
        H264NalUnitType::Sei => H264_NALU_TYPE_SEI,
        H264NalUnitType::Sps => H264_NALU_TYPE_SPS,
        H264NalUnitType::Pps => H264_NALU_TYPE_PPS,
        H264NalUnitType::Aud => 9,
        H264NalUnitType::Other(value) => value,
    }
}

/// Annex.B 形式の H.264 をパースして、含まれている NAL ユニットを走査するためのイテレーター
#[derive(Debug)]
pub struct H264AnnexBNalUnits<'a> {
    nals: Vec<h264::H264NalUnit<'a>>,
    index: usize,
    parse_error: Option<crate::Error>,
}

impl<'a> H264AnnexBNalUnits<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        match parse_annexb_nal_units(data) {
            Ok(nals) => Self {
                nals,
                index: 0,
                parse_error: None,
            },
            Err(error) => Self {
                nals: Vec::new(),
                index: 0,
                parse_error: Some(error.into()),
            },
        }
    }
}

impl<'a> Iterator for H264AnnexBNalUnits<'a> {
    type Item = crate::Result<H264NalUnit<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(error) = self.parse_error.take() {
            return Some(Err(error));
        }
        let nal = self.nals.get(self.index)?;
        self.index += 1;
        Some(Ok(H264NalUnit {
            ty: h264_nal_unit_type_to_u8(nal.nal_unit_type),
            data: nal.data,
        }))
    }
}

#[derive(Debug)]
pub struct H264NalUnit<'a> {
    pub ty: u8,
    pub data: &'a [u8],
}

/// SPS / PPS リストから `SampleEntry::Avc1` を構築する
pub fn h264_sample_entry_from_parameter_sets(
    sps_list: &[Vec<u8>],
    pps_list: &[Vec<u8>],
) -> crate::Result<SampleEntry> {
    let avc1 = build_avc1_box(sps_list, pps_list, &h264_sample_entry_config())?;
    Ok(SampleEntry::Avc1(avc1))
}

pub fn h264_sample_entry_from_annexb(
    _width: usize,
    _height: usize,
    data: &[u8],
) -> crate::Result<SampleEntry> {
    let avc1 = build_avc1_box_from_annexb(data, &h264_sample_entry_config())?;
    Ok(SampleEntry::Avc1(avc1))
}

/// Annex B 形式の H.264 を length-prefixed 形式へ変換する
pub fn h264_annexb_to_length_prefixed(data: &[u8]) -> crate::Result<Vec<u8>> {
    Ok(h264::annexb_to_length_prefixed(data, h264_length_size())?)
}

/// Annex B 形式の H.264 から SEI を除いて length-prefixed 形式へ変換する
pub fn h264_annexb_to_length_prefixed_skip_sei(data: &[u8]) -> crate::Result<Vec<u8>> {
    let nals = parse_annexb_nal_units(data)?;
    let mut annexb = Vec::new();
    for nal in nals {
        if nal.nal_unit_type == H264NalUnitType::Sei {
            continue;
        }
        annexb.extend_from_slice(&[0, 0, 0, 1]);
        annexb.extend_from_slice(nal.data);
    }
    Ok(h264::annexb_to_length_prefixed(
        &annexb,
        h264_length_size(),
    )?)
}

/// length-prefixed 形式の H.264 を Annex B 形式へ変換する
pub fn h264_length_prefixed_to_annexb(data: &[u8]) -> crate::Result<Vec<u8>> {
    Ok(h264::length_prefixed_to_annexb(data, h264_length_size())?)
}
