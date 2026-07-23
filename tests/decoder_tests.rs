use shiguredo_mp4::boxes::{Avc1Box, AvccBox, SampleEntry};
use shiguredo_openh264::Openh264Library;
use sora_archive_compositor::{
    decoder::{VideoDecoder, VideoDecoderOptions},
    media::MediaStreamId,
    metadata::SourceId,
    processor::{MediaProcessor, MediaProcessorInput, MediaProcessorOutput},
    reader_mp4::Mp4VideoReader,
    video::VideoFrame,
};
#[cfg(feature = "nvcodec")]
use sora_archive_compositor::{
    types::{CodecName, EngineName},
    video_h264::{H264_NALU_TYPE_PPS, H264_NALU_TYPE_SPS},
    video_h265::{H265_NALU_TYPE_PPS, H265_NALU_TYPE_SPS, H265_NALU_TYPE_VPS, NALU_HEADER_LENGTH},
};

const DECODER_INPUT_STREAM_ID: MediaStreamId = MediaStreamId::new(0);
const DECODER_OUTPUT_STREAM_ID: MediaStreamId = MediaStreamId::new(1);

#[test]
fn h264_multi_resolutions() -> sora_archive_compositor::Result<()> {
    // Linux では OpenH264 が無いと H.264 をデコードできない。
    // macOS は VideoToolbox があるので OPENH264_PATH なしでも進める。
    #[cfg(not(target_os = "macos"))]
    if std::env::var("OPENH264_PATH").is_err() {
        eprintln!("利用可能な H.264 デコーダーがない");
        return Ok(());
    }

    let source_id0 = SourceId::new("archive-blue-640x480-h264");
    let source_id1 = SourceId::new("archive-blue-640x480-h264");
    let reader0 = Mp4VideoReader::new(
        source_id0,
        "testdata/archive-blue-640x480-h264.mp4",
        Default::default(),
    )?;
    let reader1 = Mp4VideoReader::new(
        source_id1,
        "testdata/archive-red-320x320-h264.mp4",
        Default::default(),
    )?;
    multi_resolutions_test(reader0, reader1)?;
    Ok(())
}

#[test]
#[cfg(target_os = "macos")]
fn h265_multi_resolutions() -> sora_archive_compositor::Result<()> {
    let source_id0 = SourceId::new("archive-blue-640x480-h265");
    let source_id1 = SourceId::new("archive-red-320x320-h265");
    let reader0 = Mp4VideoReader::new(
        source_id0,
        "testdata/archive-blue-640x480-h265.mp4",
        Default::default(),
    )?;
    let reader1 = Mp4VideoReader::new(
        source_id1,
        "testdata/archive-red-320x320-h265.mp4",
        Default::default(),
    )?;
    multi_resolutions_test(reader0, reader1)?;
    Ok(())
}

#[test]
fn vp9_multi_resolutions() -> sora_archive_compositor::Result<()> {
    let source_id0 = SourceId::new("archive-blue-640x480-vp9");
    let source_id1 = SourceId::new("archive-red-320x320-vp9");
    let reader0 = Mp4VideoReader::new(
        source_id0,
        "testdata/archive-blue-640x480-vp9.mp4",
        Default::default(),
    )?;
    let reader1 = Mp4VideoReader::new(
        source_id1,
        "testdata/archive-red-320x320-vp9.mp4",
        Default::default(),
    )?;
    multi_resolutions_test(reader0, reader1)?;
    Ok(())
}

#[test]
fn av1_multi_resolutions() -> sora_archive_compositor::Result<()> {
    let source_id0 = SourceId::new("archive-blue-640x480-av1");
    let source_id1 = SourceId::new("archive-red-320x320-av1");
    let reader0 = Mp4VideoReader::new(
        source_id0,
        "testdata/archive-blue-640x480-av1.mp4",
        Default::default(),
    )?;
    let reader1 = Mp4VideoReader::new(
        source_id1,
        "testdata/archive-red-320x320-av1.mp4",
        Default::default(),
    )?;
    multi_resolutions_test(reader0, reader1)?;
    Ok(())
}

fn multi_resolutions_test<I>(reader0: I, reader1: I) -> sora_archive_compositor::Result<()>
where
    I: Iterator<Item = sora_archive_compositor::Result<VideoFrame>>,
{
    // VP9 / AV1 は OpenH264 を使わない。H.264 の Linux 未設定時スキップは
    // h264_multi_resolutions 側で行う。
    let options = VideoDecoderOptions {
        openh264_lib: match std::env::var("OPENH264_PATH") {
            Ok(path) => Some(Openh264Library::load(path)?),
            Err(_) => None,
        },
        decode_params: Default::default(),
        engines: None,
    };

    // デコードする
    let mut decoder = VideoDecoder::new(DECODER_INPUT_STREAM_ID, DECODER_OUTPUT_STREAM_ID, options);
    let mut output_frames = Vec::new();
    let mut blue_count = 0;
    let mut red_count = 0;

    for input_frame in reader0 {
        let input = prepend_h264_sps_pps(input_frame?);
        decoder.process_input(input)?;
        blue_count += 1;
    }

    // このタイミングで解像度などが切り替わる
    for input_frame in reader1 {
        let input = prepend_h264_sps_pps(input_frame?);
        decoder.process_input(input)?;
        red_count += 1;
    }

    decoder.process_input(MediaProcessorInput::eos(DECODER_INPUT_STREAM_ID))?;
    while let MediaProcessorOutput::Processed { sample, .. } = decoder.process_output()? {
        let output_frame = sample.expect_video_frame()?;
        output_frames.push(output_frame);
    }

    // デコード結果を確認する
    for output_frame in output_frames {
        if blue_count > 0 {
            blue_count -= 1;
            assert_eq!(output_frame.width, 640);
            assert_eq!(output_frame.height, 480);

            // 単色青色かどうかのチェック
            let (y_plane, u_plane, v_plane) = output_frame.as_yuv_planes().ok_or_else(|| {
                sora_archive_compositor::Error::new("YUV プレーンの取得に失敗した")
            })?;
            y_plane.iter().for_each(|&y| assert_eq!(y, 41));
            u_plane.iter().for_each(|&y| assert_eq!(y, 240));
            v_plane.iter().for_each(|&y| assert_eq!(y, 110));
        } else {
            red_count -= 1;
            assert_eq!(output_frame.width, 320);
            assert_eq!(output_frame.height, 320);

            // 単色赤色かどうかのチェック
            let (y_plane, u_plane, v_plane) = output_frame.as_yuv_planes().ok_or_else(|| {
                sora_archive_compositor::Error::new("YUV プレーンの取得に失敗した")
            })?;
            y_plane.iter().for_each(|&y| assert_eq!(y, 81));
            u_plane.iter().for_each(|&u| assert_eq!(u, 90));
            v_plane.iter().for_each(|&v| assert_eq!(v, 240));
        }
    }
    assert_eq!(blue_count, 0);
    assert_eq!(red_count, 0);

    Ok(())
}

#[expect(
    clippy::useless_conversion,
    reason = "hisui 2025.3.2 由来。実装修正は Phase 2 で扱う"
)]
fn prepend_h264_sps_pps(mut frame: VideoFrame) -> MediaProcessorInput {
    if let Some(SampleEntry::Avc1(Avc1Box {
        avcc_box: AvccBox {
            sps_list, pps_list, ..
        },
        ..
    })) = frame.sample_entry.clone()
    {
        // openh264 用に映像データ本体にも SPS / PPS を含める
        let mut data = Vec::new();
        for nalu in sps_list.into_iter().chain(pps_list.into_iter()) {
            data.extend_from_slice(&(nalu.len() as u32).to_be_bytes());
            data.extend_from_slice(&nalu);
        }
        data.extend_from_slice(&frame.data);
        frame.data = data;
    };

    // 対象外のフレームはそのまま返す
    MediaProcessorInput::video_frame(DECODER_INPUT_STREAM_ID, frame)
}

/// 1 トラック内でキーフレーム毎に解像度が変わる多エントリ stsd の MP4 を NVDEC でデコードし、
/// sample_entry 変化に伴う parameter_sets キャッシュ更新が働くことを検証する。
#[test]
#[cfg(feature = "nvcodec")]
fn h264_single_track_resolution_change_nvcodec() -> sora_archive_compositor::Result<()> {
    single_track_resolution_change_nvcodec_test(
        "testdata/archive-h264-resolution-change.mp4",
        CodecName::H264,
    )
}

/// H.264 版と同様、sample entry 変化に伴う parameter_sets キャッシュ更新を NVDEC 経路で検証する。
#[test]
#[cfg(feature = "nvcodec")]
fn h265_single_track_resolution_change_nvcodec() -> sora_archive_compositor::Result<()> {
    single_track_resolution_change_nvcodec_test(
        "testdata/archive-h265-resolution-change.mp4",
        CodecName::H265,
    )
}

#[cfg(feature = "nvcodec")]
fn single_track_resolution_change_nvcodec_test(
    path: &str,
    codec: CodecName,
) -> sora_archive_compositor::Result<()> {
    if !shiguredo_nvcodec::is_cuda_library_available() {
        eprintln!("skip: CUDA ライブラリが利用できない");
        return Ok(());
    }

    let source_id = SourceId::new("resolution-change");
    let reader = Mp4VideoReader::new(source_id, path, Default::default())?;
    let mut input_frames = Vec::new();
    for input_frame in reader {
        input_frames.push(input_frame?);
    }
    assert_keyframes_have_no_in_band_parameter_sets(&input_frames, codec);

    let options = VideoDecoderOptions {
        openh264_lib: None,
        decode_params: Default::default(),
        engines: Some(vec![EngineName::Nvcodec]),
    };
    let output_frames = decode_video_frames_with_pipeline(input_frames, options)?;
    assert_expected_resolution_sequence(&output_frames);
    Ok(())
}

#[cfg(feature = "nvcodec")]
fn decode_video_frames_with_pipeline(
    input_frames: Vec<VideoFrame>,
    options: VideoDecoderOptions,
) -> sora_archive_compositor::Result<Vec<std::sync::Arc<VideoFrame>>> {
    let mut decoder = VideoDecoder::new(DECODER_INPUT_STREAM_ID, DECODER_OUTPUT_STREAM_ID, options);
    for input_frame in input_frames {
        decoder.process_input(MediaProcessorInput::video_frame(
            DECODER_INPUT_STREAM_ID,
            input_frame,
        ))?;
    }
    decoder.process_input(MediaProcessorInput::eos(DECODER_INPUT_STREAM_ID))?;

    let mut output_frames = Vec::new();
    while let MediaProcessorOutput::Processed { sample, .. } = decoder.process_output()? {
        output_frames.push(sample.expect_video_frame()?);
    }
    Ok(output_frames)
}

/// キーフレームが in-band パラメータセットを先頭 NAL に持たないことを検証する。
///
/// 先頭 NAL がパラメータセットだと、修正前の初回限定キャッシュでもテストが通ってしまう。
/// `archive-*-resolution-change.mp4` はこの前提を満たす。
#[cfg(feature = "nvcodec")]
fn assert_keyframes_have_no_in_band_parameter_sets(frames: &[VideoFrame], codec: CodecName) {
    for frame in frames.iter().filter(|f| f.keyframe) {
        let first_nal_type = frame.data.get(NALU_HEADER_LENGTH).map(|b| match codec {
            CodecName::H265 => (b >> 1) & 0x3F,
            _ => b & 0x1F,
        });
        let is_parameter_set = match codec {
            CodecName::H265 => matches!(
                first_nal_type,
                Some(H265_NALU_TYPE_VPS) | Some(H265_NALU_TYPE_SPS) | Some(H265_NALU_TYPE_PPS)
            ),
            CodecName::H264 => matches!(
                first_nal_type,
                Some(H264_NALU_TYPE_SPS) | Some(H264_NALU_TYPE_PPS)
            ),
            _ => false,
        };
        assert!(
            !is_parameter_set,
            "キーフレームが in-band パラメータセットを先頭に持つ (テストデータ前提が崩れている)"
        );
    }
}

/// 出力フレームの解像度シーケンスが期待どおりか確認する。
///
/// `archive-h264-resolution-change.mp4` / `archive-h265-resolution-change.mp4` は
/// 15 fps × 3 秒 = 45 フレームで、キーフレームが frame 0 / 15 / 30 にある:
/// - frame 0..15 → 320x240
/// - frame 15..30 → 224x160
/// - frame 30..45 → 320x240
#[cfg(feature = "nvcodec")]
fn assert_expected_resolution_sequence(output_frames: &[std::sync::Arc<VideoFrame>]) {
    let expected: Vec<(usize, usize)> = (0..15)
        .map(|_| (320, 240))
        .chain((0..15).map(|_| (224, 160)))
        .chain((0..15).map(|_| (320, 240)))
        .collect();

    assert_eq!(
        output_frames.len(),
        expected.len(),
        "出力フレーム数が想定と異なる"
    );
    for (i, (frame, (expected_width, expected_height))) in
        output_frames.iter().zip(expected.iter()).enumerate()
    {
        assert_eq!(
            (frame.width, frame.height),
            (*expected_width, *expected_height),
            "フレーム {i} の解像度が期待値と一致しない"
        );
    }
}
