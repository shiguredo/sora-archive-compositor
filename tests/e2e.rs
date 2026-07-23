use std::path::Path;
use std::time::Duration;

use sora_archive_compositor::decoder_libvpx::LibvpxDecoder;
use sora_archive_compositor::{
    decoder::{VideoDecoder, VideoDecoderOptions},
    decoder_opus::OpusDecoder,
    media::MediaStreamId,
    metadata::SourceId,
    processor::{MediaProcessor, MediaProcessorInput, MediaProcessorOutput},
    reader_mp4::{Mp4AudioReader, Mp4VideoReader},
    stats::{Mp4AudioReaderStats, Mp4VideoReaderStats},
    types::{CodecName, EngineName},
    video::VideoFrame,
};

/// compose サブコマンドを実行する。
///
/// 失敗時は標準出力と標準エラー出力を表示してエラーを返す。
/// `--layout-file` と `--stats-file` は指定されたときだけ付与する。
fn run_compose(
    root_dir: &str,
    output_file: &Path,
    layout_file: Option<&str>,
    stats_file: Option<&Path>,
) -> noargs::Result<()> {
    let bin = env!("CARGO_BIN_EXE_sora-archive-compositor");
    let output_path = output_file.display().to_string();
    let stats_path = stats_file.map(|path| path.display().to_string());

    let mut args = vec!["compose", "--no-progress-bar"];
    if let Some(layout) = layout_file {
        args.extend_from_slice(&["--layout-file", layout]);
    }
    args.extend_from_slice(&["--output-file", &output_path]);
    if let Some(stats) = &stats_path {
        args.extend_from_slice(&["--stats-file", stats]);
    }
    args.push(root_dir);

    let output = std::process::Command::new(bin).args(&args).output()?;
    if !output.status.success() {
        eprintln!("標準出力: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!(
            "標準エラー出力: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err("sora-archive-compositor コマンドが失敗した".into());
    }
    Ok(())
}

/// ソースが空の場合
#[test]
fn empty_source() -> noargs::Result<()> {
    // 変換を実行
    let out_file = tempfile::NamedTempFile::new()?;
    run_compose("testdata/e2e/empty_source/", out_file.path(), None, None)?;

    // 結果ファイルを確認（映像・音声トラックが存在しない）
    assert!(out_file.path().exists());
    assert_eq!(
        Mp4AudioReader::new(SourceId::new("dummy"), out_file.path(), audio_stats())?.count(),
        0
    );
    assert_eq!(
        Mp4VideoReader::new(SourceId::new("dummy"), out_file.path(), video_stats())?.count(),
        0
    );

    Ok(())
}

// 共通のテスト関数
fn test_simple_single_source_common(
    test_data_dir: &str,
    expected_video_codec: CodecName,
    expected_video_engine: Option<EngineName>,
    expected_audio_codec: CodecName,
) -> noargs::Result<()> {
    // 変換を実行
    let out_file = tempfile::NamedTempFile::new()?;
    let stats_file = tempfile::NamedTempFile::new()?;
    let layout_file = format!("{test_data_dir}/layout.jsonc");
    run_compose(
        test_data_dir,
        out_file.path(),
        Some(&layout_file),
        Some(stats_file.path()),
    )?;

    if let Some(expected_engine) = expected_video_engine {
        check_engine_in_stats(&stats_file, expected_engine)?;
    }

    // 変換結果ファイルを読み込む
    assert!(out_file.path().exists());

    if expected_audio_codec == CodecName::Aac {
        // Mp4AudioReader は Opus の sample entry しか扱わない。
        // AudioDecoder に AAC エンジンが無いので、音声のコンテナ読みとデコードはしない。
        check_mp4_writer_audio_codec(&stats_file, CodecName::Aac)?;

        let mut video_reader =
            Mp4VideoReader::new(SourceId::new("dummy"), out_file.path(), video_stats())?;
        video_reader
            .by_ref()
            .collect::<sora_archive_compositor::Result<Vec<_>>>()?;

        let video_stats = video_reader.stats();
        assert_eq!(video_stats.codec.get(), Some(expected_video_codec));
        assert_eq!(
            video_stats
                .resolutions
                .get()
                .into_iter()
                .map(|r| (r.width, r.height))
                .collect::<Vec<_>>(),
            [(320, 240)]
        );
        assert_eq!(video_stats.total_sample_count.get(), 25);
        assert_eq!(
            video_stats.total_track_duration.get(),
            Duration::from_secs(1)
        );
        return Ok(());
    }

    let mut audio_reader =
        Mp4AudioReader::new(SourceId::new("dummy"), out_file.path(), audio_stats())?;
    let mut video_reader =
        Mp4VideoReader::new(SourceId::new("dummy"), out_file.path(), video_stats())?;

    // 後でデコードするために読み込み結果を覚えておく
    let audio_samples = audio_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;
    let video_samples = video_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;

    // 統計値を確認
    let audio_stats = audio_reader.stats();
    assert_eq!(audio_stats.codec, Some(CodecName::Opus));

    // 一秒分 + 一サンプル (25 ms)
    // => これは入力データのサンプル数と等しい
    assert_eq!(audio_stats.total_sample_count.get(), 51);
    assert_eq!(
        audio_stats.total_track_duration.get(),
        Duration::from_millis(1020)
    );

    let video_stats = video_reader.stats();
    assert_eq!(video_stats.codec.get(), Some(expected_video_codec));
    assert_eq!(
        video_stats
            .resolutions
            .get()
            .into_iter()
            .map(|r| (r.width, r.height))
            .collect::<Vec<_>>(),
        [(320, 240)]
    );

    // 一秒分 (25 fps = 40 ms)
    assert_eq!(video_stats.total_sample_count.get(), 25);
    assert_eq!(
        video_stats.total_track_duration.get(),
        Duration::from_secs(1)
    );

    // 音声をデコードをして中身を確認する
    let mut decoder = OpusDecoder::new()?;
    for data in audio_samples {
        let decoded = decoder.decode(&data)?;

        // 無音期間があるのは想定外
        assert!(!decoded.data.iter().all(|v| *v == 0));
    }

    // 映像をデコードをして中身を確認する
    const DECODER_INPUT_STREAM_ID: MediaStreamId = MediaStreamId::new(0);
    const DECODER_OUTPUT_STREAM_ID: MediaStreamId = MediaStreamId::new(1);

    let check_decoded_frame = |decoded: &VideoFrame| -> sora_archive_compositor::Result<()> {
        // 画像が赤一色かどうかの確認する
        let (y_plane, u_plane, v_plane) = decoded
            .as_yuv_planes()
            .ok_or_else(|| sora_archive_compositor::Error::new("YUV プレーンの取得に失敗した"))?;
        y_plane
            .iter()
            .for_each(|x| assert!(matches!(x, 80..=83), "y={x}"));
        u_plane
            .iter()
            .for_each(|x| assert!(matches!(*x, 90 | 91), "u={x}"));
        // macos-14 の VideoToolbox H.264 エンコーダーは 239 を返すことがありこの範囲に収まらない。
        // CI の macOS ジョブは macos-15 のみを対象にしているので許容幅は広げない
        v_plane
            .iter()
            .for_each(|x| assert!(matches!(x, 240 | 241), "v={x}"));
        Ok(())
    };

    let mut decoder = VideoDecoder::new(
        DECODER_INPUT_STREAM_ID,
        DECODER_OUTPUT_STREAM_ID,
        VideoDecoderOptions::default(),
    );

    for frame in video_samples {
        decoder.process_input(MediaProcessorInput::video_frame(
            DECODER_INPUT_STREAM_ID,
            frame,
        ))?;
        while let MediaProcessorOutput::Processed { sample, .. } = decoder.process_output()? {
            let decoded = sample.expect_video_frame()?;
            check_decoded_frame(&decoded)?;
        }
    }

    decoder.process_input(MediaProcessorInput::eos(DECODER_INPUT_STREAM_ID))?;

    while let MediaProcessorOutput::Processed { sample, .. } = decoder.process_output()? {
        let decoded = sample.expect_video_frame()?;
        check_decoded_frame(&decoded)?;
    }

    Ok(())
}

/// stats_file を確認して、デコーダーとエンコーダーの engine が期待通りかをチェックする
#[expect(
    clippy::collapsible_if,
    reason = "hisui 2025.3.2 由来。実装修正は Phase 2 で扱う"
)]
fn check_engine_in_stats(
    stats_file: &tempfile::NamedTempFile,
    expected_engine: EngineName,
) -> noargs::Result<()> {
    // stats_file を読み込んでパース
    let stats_json = std::fs::read_to_string(stats_file.path())
        .map_err(|e| format!("stats ファイルの読み込みに失敗した: {e}"))?;
    let stats = nojson::RawJson::parse(&stats_json)
        .map_err(|e| format!("stats JSON のパースに失敗した: {e}"))?;

    // processors 配列を取得
    let processors = stats
        .value()
        .to_member("processors")?
        .required()?
        .to_array()?;

    // デコーダーとエンコーダーの engine をチェック
    let mut found_decoder = false;
    let mut found_encoder = false;

    for processor in processors {
        let processor_type = processor
            .to_member("type")?
            .required()?
            .to_unquoted_string_str()?;

        match processor_type.as_ref() {
            "video_decoder" => {
                if let Some(engine_value) = processor.to_member("engine")?.optional() {
                    if let Ok(engine_str) = engine_value.to_unquoted_string_str() {
                        assert_eq!(
                            engine_str.as_ref(),
                            expected_engine.as_str(),
                            "映像デコーダーの engine が一致しない"
                        );
                        found_decoder = true;
                    }
                }
            }
            "video_encoder" => {
                if let Some(engine_value) = processor.to_member("engine")?.optional() {
                    let engine_str = engine_value
                        .to_unquoted_string_str()
                        .map_err(|e| format!("engine が文字列ではない: {e}"))?;
                    assert_eq!(
                        engine_str.as_ref(),
                        expected_engine.as_str(),
                        "映像エンコーダーの engine が一致しない"
                    );
                    found_encoder = true;
                }
            }
            _ => {}
        }
    }

    // デコーダーとエンコーダーが両方とも見つかったことを確認
    assert!(found_decoder, "stats に映像デコーダーが見つからない");
    assert!(found_encoder, "stats に映像エンコーダーが見つからない");

    Ok(())
}

/// `--stats-file` の `mp4_writer` の `audio_codec` が期待どおりかを確認する
fn check_mp4_writer_audio_codec(
    stats_file: &tempfile::NamedTempFile,
    expected_codec: CodecName,
) -> noargs::Result<()> {
    let stats_json = std::fs::read_to_string(stats_file.path())
        .map_err(|e| format!("stats ファイルの読み込みに失敗した: {e}"))?;
    let stats = nojson::RawJson::parse(&stats_json)
        .map_err(|e| format!("stats JSON のパースに失敗した: {e}"))?;

    let processors = stats
        .value()
        .to_member("processors")?
        .required()?
        .to_array()?;

    for processor in processors {
        let processor_type = processor
            .to_member("type")?
            .required()?
            .to_unquoted_string_str()?;
        if processor_type.as_ref() != "mp4_writer" {
            continue;
        }

        let codec_value = processor
            .to_member("audio_codec")?
            .required()
            .map_err(|_| "mp4_writer の audio_codec が無い")?;
        let codec_str = codec_value
            .to_unquoted_string_str()
            .map_err(|e| format!("audio_codec が文字列ではない: {e}"))?;
        assert_eq!(
            codec_str.as_ref(),
            expected_codec.as_str(),
            "mp4_writer の audio_codec が一致しない"
        );
        return Ok(());
    }

    Err("stats に mp4_writer が見つからない".into())
}

/// 単一のソースをそのまま変換する場合
/// - 入力:
///   - 映像:
///     - VP9
///     - 30 fps
///     - 320x240
///     - 赤一色
///   - 音声:
///     - OPUS
///     - ホワイトノイズ
/// - 出力:
///   - VP9, OPUS, 25 fps, 320x240
#[test]
fn simple_single_source_vp9() -> noargs::Result<()> {
    test_simple_single_source_common(
        "testdata/e2e/simple_single_source_vp9/",
        CodecName::Vp9,
        Some(EngineName::Libvpx),
        CodecName::Opus,
    )
}

/// simple_single_source_vp9 とほぼ同様だけど nvcodec は VP9 エンコードをサポートしていないので、
/// 出力では H.264 を使っている
#[test]
#[cfg(feature = "nvcodec")]
fn simple_single_source_vp9_nvcodec() -> noargs::Result<()> {
    test_simple_single_source_common(
        "testdata/e2e/simple_single_source_vp9_nvcodec/",
        CodecName::H264,
        Some(EngineName::Nvcodec),
        CodecName::Opus,
    )
}

/// simple_single_source_vp9 とほぼ同様だけどエンコードに AAC を指定している
#[test]
#[cfg(any(feature = "fdk-aac", target_os = "macos"))]
fn simple_single_source_aac_encode() -> noargs::Result<()> {
    test_simple_single_source_common(
        "testdata/e2e/simple_single_source_aac_encode/",
        CodecName::Av1,
        None,
        CodecName::Aac,
    )
}

/// 単一のソースをそのまま変換する場合 (H.265 版)
/// - 入力:
///   - 映像:
///     - H.265
///     - 30 fps
///     - 320x240
///     - 赤一色
///   - 音声:
///     - OPUS
///     - ホワイトノイズ
/// - 出力:
///   - VP9, OPUS, 25 fps, 320x240
#[test]
#[cfg(any(feature = "nvcodec", target_os = "macos"))]
fn simple_single_source_h265() -> noargs::Result<()> {
    test_simple_single_source_common(
        "testdata/e2e/simple_single_source_h265/",
        CodecName::H265,
        None,
        CodecName::Opus,
    )
}

/// 単一のソースをそのまま変換する場合 (H.264 版)
/// - 入力:
///   - 映像:
///     - H.264
///     - 30 fps
///     - 320x240
///     - 赤一色
///   - 音声:
///     - OPUS
///     - ホワイトノイズ
/// - 出力:
///   - VP9, OPUS, 25 fps, 320x240
#[test]
#[cfg(any(feature = "nvcodec", target_os = "macos"))]
fn simple_single_source_h264() -> noargs::Result<()> {
    test_simple_single_source_common(
        "testdata/e2e/simple_single_source_h264/",
        CodecName::H264,
        None,
        CodecName::Opus,
    )
}

/// 単一のソースをそのまま変換する場合 (AV1 版)
/// - 入力:
///   - 映像:
///     - AV1
///     - 30 fps
///     - 320x240
///     - 赤一色
///   - 音声:
///     - OPUS
///     - ホワイトノイズ
/// - 出力:
///   - VP9, OPUS, 25 fps, 320x240
#[test]
fn simple_single_source_av1() -> noargs::Result<()> {
    test_simple_single_source_common(
        "testdata/e2e/simple_single_source_av1/",
        CodecName::Av1,
        None,
        CodecName::Opus,
    )
}

/// 単一のソースをそのまま変換する場合（奇数解像度版）
/// - 入力:
///   - 映像:
///     - VP9
///     - 30 fps
///     - 319x239
///     - 赤一色
///   - 音声:
///     - OPUS
///     - ホワイトノイズ
/// - 出力:
///   - VP9, OPUS, 25 fps, 319x239
#[test]
fn odd_resolution_single_source() -> noargs::Result<()> {
    // 変換を実行
    let out_file = tempfile::NamedTempFile::new()?;
    run_compose(
        "testdata/e2e/odd_resolution_single_source/",
        out_file.path(),
        None,
        None,
    )?;

    // 変換結果ファイルを読み込む
    assert!(out_file.path().exists());
    let mut audio_reader =
        Mp4AudioReader::new(SourceId::new("dummy"), out_file.path(), audio_stats())?;
    let mut video_reader =
        Mp4VideoReader::new(SourceId::new("dummy"), out_file.path(), video_stats())?;

    // 後でデコードするために読み込み結果を覚えておく
    let audio_samples = audio_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;
    let video_samples = video_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;

    // 統計値を確認
    let audio_stats = audio_reader.stats();
    assert_eq!(audio_stats.codec, Some(CodecName::Opus));

    // 一秒分 + 一サンプル (25 ms)
    // => これは入力データのサンプル数と等しい
    assert_eq!(audio_stats.total_sample_count.get(), 51);
    assert_eq!(
        audio_stats.total_track_duration.get(),
        Duration::from_millis(1020)
    );

    let video_stats = video_reader.stats();
    assert_eq!(video_stats.codec.get(), Some(CodecName::Vp9));
    assert_eq!(
        video_stats
            .resolutions
            .get()
            .into_iter()
            .map(|r| (r.width, r.height))
            .collect::<Vec<_>>(),
        // 合成後は偶数解像度になる
        //（下と右に枠線が入る）
        [(320, 240)]
    );

    // 一秒分 (25 fps = 40 ms)
    assert_eq!(video_stats.total_sample_count.get(), 25);
    assert_eq!(
        video_stats.total_track_duration.get(),
        Duration::from_secs(1)
    );

    // 音声をデコードをして中身を確認する
    let mut decoder = OpusDecoder::new()?;
    for data in audio_samples {
        let decoded = decoder.decode(&data)?;

        // 無音期間があるのは想定外
        assert!(!decoded.data.iter().all(|v| *v == 0));
    }

    // 映像をデコードをして中身を確認する
    let check_decoded_frames =
        |decoder: &mut LibvpxDecoder| -> sora_archive_compositor::Result<()> {
            while let Some(decoded) = decoder.next_decoded_frame() {
                // 画像が赤一色かどうかの確認する（ただし、右と下の枠線は黒色になる）
                let (y_plane, u_plane, v_plane) = decoded.as_yuv_planes().ok_or_else(|| {
                    sora_archive_compositor::Error::new("YUV プレーンの取得に失敗した")
                })?;

                y_plane.iter().enumerate().for_each(|(i, &x)| {
                    let col = i % 320;
                    let row = i / 320;
                    if col >= 318 || row >= 238 {
                        // 2026 系 shiguredo_libvpx へ更新後、境界サンプルの Y 値のばらつきが広がったため tolerance を大きめに緩める
                        assert!(matches!(x, 0..=15), "黒の Y 値を期待したが y={x} だった",);
                    } else {
                        // 2026 系 shiguredo_libvpx へ更新後、Y 値のばらつきが少し広がったため tolerance を +5 に緩める
                        assert!(matches!(x, 74..=85), "赤の Y 値を期待したが y={x} だった",);
                    }
                });

                u_plane.iter().enumerate().for_each(|(i, &x)| {
                    let col = (i % 160) * 2;
                    let row = (i / 160) * 2;
                    if col >= 318 || row >= 238 {
                        assert!(matches!(x, 122..=131), "黒の U 値を期待したが u={x} だった");
                    } else {
                        assert!(matches!(x, 86..=95), "赤の U 値を期待したが u={x} だった");
                    }
                });

                v_plane.iter().enumerate().for_each(|(i, &x)| {
                    let col = (i % 160) * 2;
                    let row = (i / 160) * 2;
                    if col >= 318 || row >= 238 {
                        assert!(matches!(x, 122..=131), "黒の V 値を期待したが v={x} だった");
                    } else {
                        assert!(matches!(x, 235..=244), "赤の V 値を期待したが v={x} だった");
                    }
                });
            }
            Ok(())
        };

    let mut decoder = LibvpxDecoder::new_vp9()?;
    for frame in video_samples {
        decoder.decode(&frame)?;
        check_decoded_frames(&mut decoder)?;
    }
    decoder.finish()?;
    check_decoded_frames(&mut decoder)?;

    Ok(())
}

/// 複数のソースをレイアウト指定なしで変換する場合
// nvcodec feature が有効な場合、デコーダ選択で NVDEC が優先されるが、
// テスト用の小さい解像度の映像（16x16 等）は NVDEC の制約で処理できないためスキップする
#[test]
#[cfg(not(feature = "nvcodec"))]
#[expect(
    clippy::identity_op,
    reason = "hisui 2025.3.2 由来。実装修正は Phase 2 で扱う"
)]
fn simple_multi_sources() -> noargs::Result<()> {
    // 変換を実行
    let out_file = tempfile::NamedTempFile::new()?;
    run_compose(
        "testdata/e2e/simple_multi_sources/",
        out_file.path(),
        None,
        None,
    )?;

    // 変換結果ファイルを読み込む
    assert!(out_file.path().exists());
    let mut audio_reader =
        Mp4AudioReader::new(SourceId::new("dummy"), out_file.path(), audio_stats())?;
    let mut video_reader =
        Mp4VideoReader::new(SourceId::new("dummy"), out_file.path(), video_stats())?;

    // [NOTE]
    // レイアウトファイル未指定だと映像の解像度が大きめになって
    // テスト内でデコード結果を確認するのが少し面倒なので、このテストでは省略している
    // （統計値を取得するためにイテレーターを最後まで実行する必要はある）
    let _audio_samples = audio_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;
    let _video_samples = video_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;

    // 統計値を確認
    let audio_stats = audio_reader.stats();
    assert_eq!(audio_stats.codec, Some(CodecName::Opus));

    // 一秒分 + 一サンプル (25 ms)
    // => これは入力データのサンプル数と等しい
    assert_eq!(audio_stats.total_sample_count.get(), 51);
    assert_eq!(
        audio_stats.total_track_duration.get(),
        Duration::from_millis(1020)
    );

    let video_stats = video_reader.stats();
    assert_eq!(video_stats.codec.get(), Some(CodecName::Vp9));

    // レイアウトファイル未指定の場合には、一つのセルの解像度は 320x240 で、
    // 今回はソースが三つなのでグリッドは 3x1 となり、
    // 以下の解像度になる
    assert_eq!(
        video_stats
            .resolutions
            .get()
            .into_iter()
            .map(|r| (r.width, r.height))
            .collect::<Vec<_>>(),
        // NOTE: +4 は枠線用
        [(320 * 3 + 4, 240 * 1)]
    );

    // 一秒分 (25 fps = 40 ms)
    assert_eq!(video_stats.total_sample_count.get(), 25);
    assert_eq!(
        video_stats.total_track_duration.get(),
        Duration::from_secs(1)
    );

    Ok(())
}

/// 分割録画の変換テスト
/// - 同一接続から時系列で分割された複数のソースファイル（R -> G -> B）を一つにまとめる
/// - 各ソースファイルは 16x16 の解像度
/// - レイアウトファイルで縦に並べて配置
// nvcodec feature が有効な場合、デコーダ選択で NVDEC が優先されるが、
// テスト用の小さい解像度の映像（16x16 等）は NVDEC の制約で処理できないためスキップする
#[test]
#[cfg(not(feature = "nvcodec"))]
fn simple_split_archive() -> noargs::Result<()> {
    // 変換を実行
    let out_file = tempfile::NamedTempFile::new()?;
    run_compose(
        "testdata/e2e/simple_split_archive/",
        out_file.path(),
        Some("testdata/e2e/simple_split_archive/layout.jsonc"),
        None,
    )?;

    // 変換結果ファイルを読み込む
    assert!(out_file.path().exists());
    let mut audio_reader =
        Mp4AudioReader::new(SourceId::new("dummy"), out_file.path(), audio_stats())?;
    let mut video_reader =
        Mp4VideoReader::new(SourceId::new("dummy"), out_file.path(), video_stats())?;

    // 後でデコードするために読み込み結果を覚えておく
    let audio_samples = audio_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;
    let video_samples = video_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;

    // 統計値を確認
    let audio_stats = audio_reader.stats();
    assert_eq!(audio_stats.codec, Some(CodecName::Opus));

    // 分割ファイルが 3 つ（各 1 秒）なので合計 3 秒分 + 3 サンプル (25 ms * 3)
    assert_eq!(audio_stats.total_sample_count.get(), 153); // 51 * 3
    assert_eq!(
        audio_stats.total_track_duration.get(),
        Duration::from_millis(3060) // 1020 * 3
    );

    let video_stats = video_reader.stats();
    assert_eq!(video_stats.codec.get(), Some(CodecName::Vp9));
    assert_eq!(
        video_stats
            .resolutions
            .get()
            .into_iter()
            .map(|r| (r.width, r.height))
            .collect::<Vec<_>>(),
        [(16, 16)] // 単一ソース（分割された部分）なので 16x16
    );

    // 3 秒分 (25 fps = 40 ms * 75 フレーム)
    assert_eq!(video_stats.total_sample_count.get(), 75); // 25 * 3
    assert_eq!(
        video_stats.total_track_duration.get(),
        Duration::from_secs(3)
    );

    // 音声をデコードをして中身を確認する
    let mut decoder = OpusDecoder::new()?;
    for data in audio_samples {
        let decoded = decoder.decode(&data)?;

        // 無音期間があるのは想定外
        assert!(!decoded.data.iter().all(|v| *v == 0));
    }

    // 映像をデコードをして中身を確認する
    // 時系列順に R -> G -> B の色変化を確認
    let check_decoded_frames = |decoder: &mut LibvpxDecoder,
                                frame_index: &mut usize|
     -> sora_archive_compositor::Result<()> {
        while let Some(decoded) = decoder.next_decoded_frame() {
            // Y 成分だけを確認して色の変化を検証
            let (y_plane, _u_plane, v_plane) = decoded.as_yuv_planes().ok_or_else(|| {
                sora_archive_compositor::Error::new("YUV プレーンの取得に失敗した")
            })?;

            // フレーム番号に基づいて期待される色を判定
            // 0-24: 赤, 25-49: 緑, 50-74: 青
            //
            // なお赤と緑は同じような Y 値でエンコードされているので、 V の値も考慮している

            if *frame_index < 25 {
                // 赤色の期間
                (y_plane.iter().zip(v_plane.iter())).for_each(|(&y, &v)| {
                    assert!(
                        matches!(y, 80..=82) && matches!(v, 240),
                        "赤の Y / V 値を期待したが y={y} / v={v} (フレーム {}) だった",
                        *frame_index
                    );
                });
            } else if *frame_index < 50 {
                // 緑色の期間
                (y_plane.iter().zip(v_plane.iter())).for_each(|(&y, &v)| {
                    assert!(
                        matches!(y, 80..=82) && matches!(v, 81),
                        "緑の Y / V 値を期待したが y={y} / v={v} (フレーム {}) だった",
                        *frame_index
                    );
                });
            } else if *frame_index < 75 {
                // 青色の期間
                y_plane.iter().for_each(|&y| {
                    assert!(
                        matches!(y, 40..=42),
                        "青の Y 値を期待したが y={y} (フレーム {}) だった",
                        *frame_index
                    );
                });
            }
            *frame_index += 1;
        }
        Ok(())
    };

    let mut decoder = LibvpxDecoder::new_vp9()?;
    let mut frame_index = 0;
    for frame in video_samples {
        decoder.decode(&frame)?;
        check_decoded_frames(&mut decoder, &mut frame_index)?;
    }
    decoder.finish()?;
    check_decoded_frames(&mut decoder, &mut frame_index)?;

    // 全フレームが処理されたことを確認
    assert_eq!(frame_index, 75);

    Ok(())
}

/// 複数のソースをレイアウト指定で、縦に並べて変換する場合
// nvcodec feature が有効な場合、デコーダ選択で NVDEC が優先されるが、
// テスト用の小さい解像度の映像（16x16 等）は NVDEC の制約で処理できないためスキップする
#[test]
#[cfg(not(feature = "nvcodec"))]
fn multi_sources_single_column() -> noargs::Result<()> {
    // 変換を実行
    let out_file = tempfile::NamedTempFile::new()?;
    run_compose(
        "testdata/e2e/multi_sources_single_column/",
        out_file.path(),
        Some("testdata/e2e/multi_sources_single_column/layout.json"),
        None,
    )?;

    // 変換結果ファイルを読み込む
    assert!(out_file.path().exists());
    let mut audio_reader =
        Mp4AudioReader::new(SourceId::new("dummy"), out_file.path(), audio_stats())?;
    let mut video_reader =
        Mp4VideoReader::new(SourceId::new("dummy"), out_file.path(), video_stats())?;

    // 後でデコードするために読み込み結果を覚えておく
    let audio_samples = audio_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;
    let video_samples = video_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;

    // 統計値を確認
    let audio_stats = audio_reader.stats();
    assert_eq!(audio_stats.codec, Some(CodecName::Opus));

    // 一秒分 + 一サンプル (25 ms)
    // => これは入力データのサンプル数と等しい
    assert_eq!(audio_stats.total_sample_count.get(), 51);
    assert_eq!(
        audio_stats.total_track_duration.get(),
        Duration::from_millis(1020)
    );

    let video_stats = video_reader.stats();
    assert_eq!(video_stats.codec.get(), Some(CodecName::Vp9));
    assert_eq!(
        video_stats
            .resolutions
            .get()
            .into_iter()
            .map(|r| (r.width, r.height))
            .collect::<Vec<_>>(),
        [(16, 52)]
    );

    // 一秒分 (25 fps = 40 ms)
    assert_eq!(video_stats.total_sample_count.get(), 25);
    assert_eq!(
        video_stats.total_track_duration.get(),
        Duration::from_secs(1)
    );

    // 音声をデコードをして中身を確認する
    let mut decoder = OpusDecoder::new()?;
    for data in audio_samples {
        let decoded = decoder.decode(&data)?;

        // 無音期間があるのは想定外
        assert!(!decoded.data.iter().all(|v| *v == 0));
    }

    // 映像をデコードをして中身を確認する
    let check_decoded_frames =
        |decoder: &mut LibvpxDecoder| -> sora_archive_compositor::Result<()> {
            while let Some(decoded) = decoder.next_decoded_frame() {
                // 完全なチェックは面倒なので Y 成分だけを確認する
                let (y_plane, _u_plane, _v_plane) = decoded.as_yuv_planes().ok_or_else(|| {
                    sora_archive_compositor::Error::new("YUV プレーンの取得に失敗した")
                })?;

                let width = 16;
                for (i, y) in y_plane.iter().copied().enumerate() {
                    if i / width < 16 {
                        // 最初の 16 行は青
                        assert!(matches!(y, 40..=43), "y={y}");
                    } else if i / width < 16 + 2 {
                        // 次の 2 行は黒色（枠線）
                        // 2026 系 shiguredo_libvpx へ更新後、境界サンプルの Y 値のばらつきが広がったため tolerance を大きめに緩める
                        assert!(matches!(y, 0..=15), "y={y}");
                    } else if i / width < 16 + 2 + 16 {
                        // 次の 16 行は緑
                        // 2026 系 shiguredo_libvpx へ更新後、Y 値のばらつきが少し広がったため tolerance を緩める
                        assert!(matches!(y, 180..=195), "y={y}");
                    } else if i / width < 16 + 2 + 16 + 2 {
                        // 次の 2 行は黒色（枠線）
                        // 2026 系 shiguredo_libvpx へ更新後、境界サンプルの Y 値のばらつきが広がったため tolerance を大きめに緩める
                        assert!(matches!(y, 0..=15), "y={y}");
                    } else if i / width < 16 + 2 + 16 + 2 + 16 {
                        // 最後の 16 行は赤
                        // 2026 系 shiguredo_libvpx へ更新後、Y 値のばらつきが少し広がったため tolerance を緩める
                        assert!(matches!(y, 73..=90), "y={y}");
                    } else {
                        unreachable!()
                    }
                }
            }
            Ok(())
        };

    let mut decoder = LibvpxDecoder::new_vp9()?;
    for frame in video_samples {
        decoder.decode(&frame)?;
        check_decoded_frames(&mut decoder)?;
    }
    decoder.finish()?;
    check_decoded_frames(&mut decoder)?;

    Ok(())
}

/// リージョンが二つあるレイアウトのテスト
/// - 全体の解像度は 16x34
/// - 一つ目のリージョンには縦並びの二つのセルがある（青と緑）
/// - 二つ目のリージョンは中央に一つのセルがある（赤） => 後ろに別のリージョンがあるので外枠がつく
/// - 音声ソースはなし
// nvcodec feature が有効な場合、デコーダ選択で NVDEC が優先されるが、
// テスト用の小さい解像度の映像（16x16 等）は NVDEC の制約で処理できないためスキップする
#[test]
#[cfg(not(feature = "nvcodec"))]
fn two_regions() -> noargs::Result<()> {
    // 変換を実行
    let out_file = tempfile::NamedTempFile::new()?;
    run_compose(
        "testdata/e2e/two_regions/",
        out_file.path(),
        Some("testdata/e2e/two_regions/layout.json"),
        None,
    )?;

    // 変換結果ファイルを読み込む
    assert!(out_file.path().exists());
    let mut video_reader =
        Mp4VideoReader::new(SourceId::new("dummy"), out_file.path(), video_stats())?;

    // 音声はなし
    assert_eq!(
        Mp4AudioReader::new(SourceId::new("dummy"), out_file.path(), audio_stats())?.count(),
        0
    );

    // 後でデコードするために読み込み結果を覚えておく
    let video_samples = video_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;

    // 統計値を確認
    let video_stats = video_reader.stats();
    assert_eq!(video_stats.codec.get(), Some(CodecName::Vp9));
    assert_eq!(
        video_stats
            .resolutions
            .get()
            .into_iter()
            .map(|r| (r.width, r.height))
            .collect::<Vec<_>>(),
        [(16, 34)]
    );

    // 一秒分 (25 fps = 40 ms)
    assert_eq!(video_stats.total_sample_count.get(), 25);
    assert_eq!(
        video_stats.total_track_duration.get(),
        Duration::from_secs(1)
    );

    // 映像をデコードをして中身を確認する
    let check_decoded_frames =
        |decoder: &mut LibvpxDecoder| -> sora_archive_compositor::Result<()> {
            while let Some(decoded) = decoder.next_decoded_frame() {
                // 完全なチェックは面倒なので Y 成分だけを確認する
                let (y_plane, _u_plane, _v_plane) = decoded.as_yuv_planes().ok_or_else(|| {
                    sora_archive_compositor::Error::new("YUV プレーンの取得に失敗した")
                })?;

                let width = 16;
                for (i, y) in y_plane.iter().copied().enumerate() {
                    if i / width < 8 {
                        // 最初の 8 行は青
                        assert!(matches!(y, 40..=44), "y={y}");
                    } else if i / width < 8 + 2 {
                        // 次の 2 行は黒色（枠線）
                        // 2026 系 shiguredo_libvpx へ更新後、境界サンプルの Y 値のばらつきが広がったため tolerance を大きめに緩める
                        assert!(matches!(y, 0..=15), "y={y}");
                    } else if i / width < 8 + 2 + 16 {
                        // 次の 16 行は赤
                        // 2026 系 shiguredo_libvpx へ更新後、Y 値のばらつきが少し広がったため tolerance を +2 に緩める
                        assert!(matches!(y, 75..=85), "y={y}");
                    } else if i / width < 8 + 2 + 16 + 2 {
                        // 次の 2 行は黒色（枠線）
                        // 2026 系 shiguredo_libvpx へ更新後、境界サンプルの Y 値のばらつきが広がったため tolerance を大きめに緩める
                        assert!(matches!(y, 0..=15), "y={y}");
                    } else if i / width < 8 + 2 + 16 + 2 + 6 {
                        // 最後の 6 行は緑
                        // 2026 系 shiguredo_libvpx へ更新後、Y 値のばらつきが少し広がったため tolerance を緩める
                        assert!(matches!(y, 180..=195), "y={y}");
                    } else {
                        unreachable!()
                    }
                }
            }
            Ok(())
        };

    let mut decoder = LibvpxDecoder::new_vp9()?;
    for frame in video_samples {
        decoder.decode(&frame)?;
        check_decoded_frames(&mut decoder)?;
    }
    decoder.finish()?;
    check_decoded_frames(&mut decoder)?;

    Ok(())
}

fn audio_stats() -> Mp4AudioReaderStats {
    Mp4AudioReaderStats {
        codec: Some(CodecName::Opus),
        ..Default::default()
    }
}

fn video_stats() -> Mp4VideoReaderStats {
    Mp4VideoReaderStats::default()
}

/// generate-archive で途中の解像度変更を指定し、解像度が切り替わる MP4 が生成されることを確認する
///
/// オプションは位置引数 (OUTPUT_DIR) より前に指定できる。
/// - 1280x720 で 1 秒 (フレーム 0 から 29)、フレーム 30 から 640x360 に切り替わる
/// - 解像度変更点 (フレーム 30) は新しいエンコーダーの最初のフレームであり、キーフレームになっている
#[test]
fn generate_archive_with_resolution_change() -> noargs::Result<()> {
    let out_dir = tempfile::TempDir::new()?;

    // ビルド済みバイナリのパスを取得
    let bin = env!("CARGO_BIN_EXE_sora-archive-compositor");
    let output = std::process::Command::new(bin)
        .args([
            "generate-archive",
            "--connection-id",
            "alice",
            "--duration",
            "3",
            "--resolution-change",
            "1:640x360",
            out_dir.path().to_str().unwrap(),
        ])
        .output()?;

    if !output.status.success() {
        eprintln!("標準出力: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!(
            "標準エラー出力: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err("generate-archive コマンドが失敗した".into());
    }

    // 変換結果ファイルを読み込む
    let mp4_path = out_dir.path().join("archive-alice.mp4");
    let mut video_reader = Mp4VideoReader::new(SourceId::new("dummy"), &mp4_path, video_stats())?;
    let video_frames = video_reader
        .by_ref()
        .collect::<sora_archive_compositor::Result<Vec<_>>>()?;

    // 統計値を確認する
    let video_stats = video_reader.stats();
    assert_eq!(video_stats.codec.get(), Some(CodecName::Vp9));

    // 2 つの解像度が記録される (BTreeSet のため (width, height) の昇順)
    assert_eq!(
        video_stats
            .resolutions
            .get()
            .into_iter()
            .map(|r| (r.width, r.height))
            .collect::<Vec<_>>(),
        [(640, 360), (1280, 720)]
    );

    // 3 秒分 (30 fps = 33.3 ms * 90 フレーム)
    // マイクロ秒精度のため 2.99997 秒になる
    assert_eq!(video_stats.total_sample_count.get(), 90);
    assert_eq!(
        video_stats.total_track_duration.get(),
        Duration::from_micros(2_999_970)
    );

    // 解像度変更点 (フレーム 30) 以降は 640x360 になる
    assert_eq!((video_frames[0].width, video_frames[0].height), (1280, 720));
    assert_eq!(
        (video_frames[29].width, video_frames[29].height),
        (1280, 720)
    );
    assert_eq!(
        (video_frames[30].width, video_frames[30].height),
        (640, 360)
    );
    assert_eq!(
        (video_frames[89].width, video_frames[89].height),
        (640, 360)
    );

    // 先頭と解像度変更点はキーフレームになっている
    assert!(video_frames[0].keyframe);
    assert!(video_frames[30].keyframe);

    Ok(())
}
