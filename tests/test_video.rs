use std::{num::NonZeroUsize, time::Duration};

use sora_archive_compositor::{
    types::EvenUsize,
    video::{FrameRate, VideoFormat, VideoFrame},
};

fn dummy_input() -> VideoFrame {
    VideoFrame::black(EvenUsize::truncating_new(2), EvenUsize::truncating_new(2))
}

/// packed（stride が幅と等しい）で Y プレーンが短いと Err になる
#[test]
fn new_i420_rejects_short_packed_y_plane() {
    let result = VideoFrame::new_i420(
        dummy_input(),
        2,
        2,
        &[0; 3], // y_size は 4
        &[128],
        &[128],
        2,
        1,
        1,
    );
    assert!(result.is_err(), "プレーン長不足なのに成功した: {result:?}");
}

/// padded（stride が幅より大きい）で最終行まで届かないと Err になる
#[test]
fn new_i420_rejects_short_padded_y_plane() {
    // 幅 2・高さ 2・stride 4 なら最終行は offset 4 + 幅 2 で 6 バイト必要
    let result = VideoFrame::new_i420(dummy_input(), 2, 2, &[0; 5], &[128], &[128], 4, 1, 1);
    assert!(
        result.is_err(),
        "パディング付きプレーン長不足なのに成功した: {result:?}"
    );
}

/// 十分なプレーン長なら packed で組み立てられる
#[test]
fn new_i420_accepts_sufficient_packed_planes() {
    let frame = VideoFrame::new_i420(dummy_input(), 2, 2, &[1, 2, 3, 4], &[10], &[20], 2, 1, 1)
        .expect("十分なプレーン長なのに失敗した");
    assert_eq!(frame.width, 2);
    assert_eq!(frame.height, 2);
    assert_eq!(frame.format, VideoFormat::I420);
    let (y, u, v) = frame
        .as_yuv_planes()
        .expect("組み立て直後の I420 プレーン取得に失敗した");
    assert_eq!(y, &[1, 2, 3, 4]);
    assert_eq!(u, &[10]);
    assert_eq!(v, &[20]);
}

/// I420 だが data が短いフレームは panic せず None になる
#[test]
fn as_yuv_planes_returns_none_for_short_i420_data() {
    let frame = VideoFrame {
        source_id: None,
        data: vec![0; 1],
        format: VideoFormat::I420,
        keyframe: true,
        width: 2,
        height: 2,
        timestamp: Duration::ZERO,
        duration: Duration::ZERO,
        sample_entry: None,
    };
    assert!(
        frame.as_yuv_planes().is_none(),
        "短い I420 data なのにプレーンが取れた"
    );
}

/// 分数 fps (30/2 = 15 fps) でフレーム数・タイムスタンプ・duration が一致する
#[test]
fn frame_rate_fractional_fps_calculations() {
    let fps = FrameRate {
        numerator: NonZeroUsize::MIN.saturating_add(29),
        denominator: NonZeroUsize::MIN.saturating_add(1),
    };
    assert_eq!(fps.frames_per_second(), 15.0);
    assert_eq!(fps.frame_count_for_secs(10), 150);
    assert_eq!(fps.frame_duration(), Duration::from_secs(2) / 30);
    assert_eq!(fps.frame_timestamp(30), Duration::from_secs(2));
}
