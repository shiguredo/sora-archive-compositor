//! `*_sample_entry_from_frame` のユニットテスト

use shiguredo_mp4::boxes::SampleEntry;
use sora_archive_compositor::{video_av1, video_vp8, video_vp9};

// ===== VP8 helpers =====

const VP8_KEY_FRAME_START_CODE: [u8; 3] = [0x9D, 0x01, 0x2A];

fn vp8_keyframe_bytes(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = Vec::new();
    // frame_type=Key(0), version=0, show_frame=1, first_partition_size=0
    let tag = 1u32 << 4;
    bytes.push((tag & 0xFF) as u8);
    bytes.push(((tag >> 8) & 0xFF) as u8);
    bytes.push(((tag >> 16) & 0xFF) as u8);
    bytes.extend_from_slice(&VP8_KEY_FRAME_START_CODE);
    bytes.extend_from_slice(&(width & 0x3FFF).to_le_bytes());
    bytes.extend_from_slice(&(height & 0x3FFF).to_le_bytes());
    bytes
}

fn vp8_interframe_bytes() -> Vec<u8> {
    // frame_type=Inter(1), version=0, show_frame=0, first_partition_size=0
    vec![0x01, 0x00, 0x00]
}

// ===== VP9 helpers =====

#[derive(Debug, Default)]
struct BitWriter {
    bytes: Vec<u8>,
    bit_pos: u8,
}

impl BitWriter {
    fn push_bits(&mut self, value: u32, n: u32) {
        for i in (0..n).rev() {
            let bit = ((value >> i) & 1) as u8;
            if self.bit_pos == 0 {
                self.bytes.push(0);
            }
            let last = self.bytes.len() - 1;
            self.bytes[last] |= bit << (7 - self.bit_pos);
            self.bit_pos = (self.bit_pos + 1) % 8;
        }
    }

    fn push_bit(&mut self, bit: u8) {
        self.push_bits(u32::from(bit), 1);
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.push_bits(u32::from(*b), 8);
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

fn vp9_keyframe_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.push_bits(2, 2); // frame_marker
    w.push_bit(0); // profile low
    w.push_bit(0); // profile high
    w.push_bit(0); // show_existing_frame
    w.push_bit(0); // KEY
    w.push_bit(1); // show_frame
    w.push_bit(0); // error_resilient_mode
    w.push_bytes(&[0x49, 0x83, 0x42]); // sync_code
    w.push_bits(1, 3); // color_space
    w.push_bit(0); // color_range
    w.push_bits((width - 1) & 0xFFFF, 16);
    w.push_bits((height - 1) & 0xFFFF, 16);
    w.push_bit(0); // render_and_frame_size_different
    w.into_bytes()
}

fn vp9_show_existing_frame_bytes() -> Vec<u8> {
    let mut w = BitWriter::default();
    w.push_bits(2, 2);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(1); // show_existing_frame
    w.push_bits(0, 3); // frame_to_show_map_idx
    w.into_bytes()
}

// ===== AV1 helpers =====

const OBU_SEQUENCE_HEADER: u8 = 1;
const OBU_FRAME_HEADER: u8 = 3;

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

fn wrap_obu(obu_type: u8, payload: &[u8], has_size: bool) -> Vec<u8> {
    let mut out = vec![(obu_type << 3) | (u8::from(has_size) << 1)];
    if has_size {
        out.extend(encode_leb128(payload.len() as u32));
    }
    out.extend_from_slice(payload);
    out
}

fn av1_reduced_still_sequence_header(width: u32, height: u32) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.push_bits(0, 3); // seq_profile
    w.push_bit(1); // still_picture
    w.push_bit(1); // reduced_still_picture_header
    w.push_bits(0, 5); // seq_level_idx[0]
    w.push_bits(15, 4);
    w.push_bits(15, 4);
    w.push_bits(width - 1, 16);
    w.push_bits(height - 1, 16);
    w.push_bit(0); // use_128x128_superblock
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0); // high_bitdepth
    w.push_bit(0); // mono_chrome
    w.push_bit(0); // color_description_present_flag
    w.push_bit(0); // color_range
    w.push_bits(0, 2);
    w.push_bit(0);
    w.push_bit(0);
    w.into_bytes()
}

fn av1_rap_sample(width: u32, height: u32) -> Vec<u8> {
    // Sequence Header (size あり) + Frame Header (size 省略、reduced still では payload 空で RAP)
    let mut sample = wrap_obu(
        OBU_SEQUENCE_HEADER,
        &av1_reduced_still_sequence_header(width, height),
        true,
    );
    sample.extend(wrap_obu(OBU_FRAME_HEADER, &[], false));
    sample
}

fn av1_non_rap_sample() -> Vec<u8> {
    // reduced still ではない Sequence Header + Inter / show_frame=0 の Frame Header
    let mut w = BitWriter::default();
    w.push_bits(0, 3); // seq_profile
    w.push_bit(1); // still_picture
    w.push_bit(0); // reduced_still_picture_header
    w.push_bit(0); // timing_info_present_flag
    w.push_bit(0); // initial_display_delay_present_flag
    w.push_bits(0, 5); // operating_points_cnt_minus_1
    w.push_bits(0, 12); // operating_point_idc[0]
    w.push_bits(0, 5); // seq_level_idx[0]
    w.push_bits(15, 4);
    w.push_bits(15, 4);
    w.push_bits(319, 16);
    w.push_bits(239, 16);
    w.push_bit(0); // frame_id_numbers_present_flag
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0); // enable_order_hint
    w.push_bit(1); // seq_choose_screen_content_tools
    w.push_bit(1); // seq_choose_integer_mv
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0); // high_bitdepth
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bits(0, 2);
    w.push_bit(0);
    w.push_bit(0);
    let mut sample = wrap_obu(OBU_SEQUENCE_HEADER, &w.into_bytes(), true);
    let mut fh = BitWriter::default();
    fh.push_bit(0); // show_existing_frame
    fh.push_bits(1, 2); // INTER
    fh.push_bit(0); // show_frame
    sample.extend(wrap_obu(OBU_FRAME_HEADER, &fh.into_bytes(), false));
    sample
}

fn sample_entry_resolution(entry: &SampleEntry) -> (u16, u16) {
    match entry {
        SampleEntry::Vp08(b) => (b.visual.width, b.visual.height),
        SampleEntry::Vp09(b) => (b.visual.width, b.visual.height),
        SampleEntry::Av01(b) => (b.visual.width, b.visual.height),
        other => panic!("想定外の SampleEntry: {other:?}"),
    }
}

// ===== VP8 =====

#[test]
fn vp8_keyframe_builds_sample_entry_with_resolution() {
    let entry = video_vp8::vp8_sample_entry_from_frame(&vp8_keyframe_bytes(320, 240))
        .expect("VP8 キーフレームから sample entry を構築できる");
    assert!(matches!(entry, SampleEntry::Vp08(_)));
    assert_eq!(sample_entry_resolution(&entry), (320, 240));
}

#[test]
fn vp8_interframe_returns_error() {
    let result = video_vp8::vp8_sample_entry_from_frame(&vp8_interframe_bytes());
    assert!(result.is_err(), "interframe なのに成功した: {result:?}");
}

#[test]
fn vp8_different_resolutions_produce_different_entries() {
    let a = video_vp8::vp8_sample_entry_from_frame(&vp8_keyframe_bytes(320, 240))
        .expect("320x240 の構築に失敗した");
    let b = video_vp8::vp8_sample_entry_from_frame(&vp8_keyframe_bytes(640, 480))
        .expect("640x480 の構築に失敗した");
    assert_eq!(sample_entry_resolution(&a), (320, 240));
    assert_eq!(sample_entry_resolution(&b), (640, 480));
}

// ===== VP9 =====

#[test]
fn vp9_keyframe_builds_sample_entry_with_resolution() {
    let entry = video_vp9::vp9_sample_entry_from_frame(&vp9_keyframe_bytes(320, 240))
        .expect("VP9 キーフレームから sample entry を構築できる");
    assert!(matches!(entry, SampleEntry::Vp09(_)));
    assert_eq!(sample_entry_resolution(&entry), (320, 240));
}

#[test]
fn vp9_show_existing_frame_returns_error() {
    let result = video_vp9::vp9_sample_entry_from_frame(&vp9_show_existing_frame_bytes());
    assert!(
        result.is_err(),
        "show_existing_frame なのに成功した: {result:?}"
    );
}

#[test]
fn vp9_different_resolutions_produce_different_entries() {
    let a = video_vp9::vp9_sample_entry_from_frame(&vp9_keyframe_bytes(320, 240))
        .expect("320x240 の構築に失敗した");
    let b = video_vp9::vp9_sample_entry_from_frame(&vp9_keyframe_bytes(640, 360))
        .expect("640x360 の構築に失敗した");
    assert_eq!(sample_entry_resolution(&a), (320, 240));
    assert_eq!(sample_entry_resolution(&b), (640, 360));
}

// ===== AV1 =====

#[test]
fn av1_rap_frame_builds_sample_entry_with_resolution() {
    let entry = video_av1::av1_sample_entry_from_frame(&av1_rap_sample(320, 240))
        .expect("AV1 RAP フレームから sample entry を構築できる");
    assert!(matches!(entry, SampleEntry::Av01(_)));
    assert_eq!(sample_entry_resolution(&entry), (320, 240));
}

#[test]
fn av1_non_rap_frame_returns_error() {
    let result = video_av1::av1_sample_entry_from_frame(&av1_non_rap_sample());
    assert!(result.is_err(), "非 RAP なのに成功した: {result:?}");
}

#[test]
fn av1_different_resolutions_produce_different_entries() {
    let a = video_av1::av1_sample_entry_from_frame(&av1_rap_sample(320, 240))
        .expect("320x240 の構築に失敗した");
    let b = video_av1::av1_sample_entry_from_frame(&av1_rap_sample(1280, 720))
        .expect("1280x720 の構築に失敗した");
    assert_eq!(sample_entry_resolution(&a), (320, 240));
    assert_eq!(sample_entry_resolution(&b), (1280, 720));
}
