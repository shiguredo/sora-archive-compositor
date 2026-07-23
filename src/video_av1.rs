//! AV1 の config OBU / フレームから `SampleEntry` を構築する

use shiguredo_mp4::{
    bitstream::av1::{
        Av1ObuParseContext, Av1ObuType, Av1SampleEntryConfig, build_av01_box,
        build_av01_box_from_config_obus, parse_frame_header_prefix, parse_obus,
        parse_sequence_header,
    },
    boxes::SampleEntry,
};

use crate::types::EvenUsize;

/// Sequence Header OBU の `obu_type` 値 (AV1 spec §6.2.2)
const OBU_TYPE_SEQUENCE_HEADER: u8 = 1;

/// config OBU 列から `SampleEntry::Av01` を構築する
///
/// 呼び出し側が Sequence Header を含む config OBU を事前に持っている経路向け。
/// `width` / `height` 引数は未使用で、寸法は Sequence Header 由来。
pub fn av1_sample_entry(
    _width: EvenUsize,
    _height: EvenUsize,
    config_obus: &[u8],
) -> crate::Result<SampleEntry> {
    let av01 = build_av01_box_from_config_obus(
        config_obus,
        &Av1SampleEntryConfig {
            initial_presentation_delay_minus_one: None,
        },
    )?;
    Ok(SampleEntry::Av01(av01))
}

/// AV1 のフレームバイト列 (Sample 文脈の OBU 列) から `SampleEntry::Av01` を構築する
///
/// Sequence Header と RAP (Key かつ `show_frame = 1`) の Frame Header が必要。
/// `av1C.configOBUs` には Sequence Header OBU だけを入れる。
pub fn av1_sample_entry_from_frame(data: &[u8]) -> crate::Result<SampleEntry> {
    let obus = parse_obus(data, Av1ObuParseContext::Sample)?;

    let sequence_obu = obus
        .iter()
        .find(|obu| matches!(obu.obu_type, Av1ObuType::SequenceHeader))
        .ok_or_else(|| {
            crate::Error::new("AV1 sample entry requires a Sequence Header OBU in the frame")
        })?;
    let seq = parse_sequence_header(sequence_obu.payload)?;

    let frame_header_obu = obus
        .iter()
        .find(|obu| {
            matches!(
                obu.obu_type,
                Av1ObuType::FrameHeader | Av1ObuType::RedundantFrameHeader | Av1ObuType::Frame
            )
        })
        .ok_or_else(|| {
            crate::Error::new(
                "AV1 sample entry requires a Frame Header, Redundant Frame Header, or Frame OBU",
            )
        })?;
    let prefix = parse_frame_header_prefix(frame_header_obu.payload, &seq)?;
    if !prefix.is_rap() {
        return Err(crate::Error::new(
            "AV1 sample entry requires a RAP frame (Key with show_frame = 1)",
        ));
    }

    // ConfigObus は全 OBU で has_size_field = 1 が必須なので、payload から組み立て直す
    let config_obus = sequence_header_as_config_obus(sequence_obu.payload);
    let av01 = build_av01_box(
        &seq,
        &config_obus,
        &Av1SampleEntryConfig {
            initial_presentation_delay_minus_one: None,
        },
    )?;
    Ok(SampleEntry::Av01(av01))
}

/// Sequence Header payload を ConfigObus 規則の 1 OBU に包む
fn sequence_header_as_config_obus(payload: &[u8]) -> Vec<u8> {
    // obu_type=SequenceHeader, extension=0, has_size=1, reserved=0
    let header0 = (OBU_TYPE_SEQUENCE_HEADER << 3) | (1 << 1);
    let mut out = Vec::new();
    out.push(header0);
    out.extend(encode_leb128(payload.len() as u32));
    out.extend_from_slice(payload);
    out
}

/// 最短の LEB128 を符号化する
fn encode_leb128(mut value: u32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
    out
}
