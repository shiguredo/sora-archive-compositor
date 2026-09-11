#![cfg(target_os = "macos")]

use sora_archive_compositor::encoder_video_toolbox_params::{
    parse_h264_encode_params, parse_h265_encode_params,
};

#[test]
fn h264_ignores_allow_frame_reordering() {
    // MP4 writer はフレームの並べ替えに対応していないため、true を指定しても false のままにする
    let raw =
        nojson::RawJson::parse(r#"{"allow_frame_reordering": true}"#).expect("JSON のパースに失敗");
    let config =
        parse_h264_encode_params(raw.value()).expect("H.264 エンコードパラメーターのパースに失敗");

    assert!(!config.allow_frame_reordering);
}

#[test]
fn h265_ignores_allow_frame_reordering() {
    // H.264 と同様に、H.265 でもフレームの並べ替えを常に無効にする
    let raw =
        nojson::RawJson::parse(r#"{"allow_frame_reordering": true}"#).expect("JSON のパースに失敗");
    let config =
        parse_h265_encode_params(raw.value()).expect("H.265 エンコードパラメーターのパースに失敗");

    assert!(!config.allow_frame_reordering);
}
