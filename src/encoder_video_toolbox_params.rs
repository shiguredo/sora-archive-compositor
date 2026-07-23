use crate::json::JsonObject;
use crate::layout::DEFAULT_LAYOUT_JSON;
use shiguredo_video_toolbox::{
    CodecConfig, DataRateLimit, EncoderConfig, H264EncoderConfig, H264EntropyMode, H264Profile,
    HevcEncoderConfig, HevcProfile, PixelFormat,
};
use std::time::Duration;

pub fn parse_h264_encode_params(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<EncoderConfig, nojson::JsonParseError> {
    let mut config = default_h264_encoder_config();

    // デフォルトレイアウトの設定を反映
    let default = nojson::RawJson::parse_jsonc(DEFAULT_LAYOUT_JSON)?.0;
    let params = JsonObject::new(
        default
            .value()
            .to_member("video_toolbox_h264_encode_params")?
            .required()?,
    )?;
    update_h264_encode_params(&params, &mut config)?;

    // 実際のレイアウトの設定を反映
    let params = JsonObject::new(value)?;
    update_h264_encode_params(&params, &mut config)?;
    // 未知のキーはデフォルトレイアウト側では出さず、ユーザーレイアウト側でのみ警告する
    params.warn_unknown_keys("video_toolbox_h264");

    Ok(config)
}

pub fn parse_h265_encode_params(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<EncoderConfig, nojson::JsonParseError> {
    let mut config = default_h265_encoder_config();

    // デフォルトレイアウトの設定を反映
    let default = nojson::RawJson::parse_jsonc(DEFAULT_LAYOUT_JSON)?.0;
    let params = JsonObject::new(
        default
            .value()
            .to_member("video_toolbox_h265_encode_params")?
            .required()?,
    )?;
    update_h265_encode_params(&params, &mut config)?;

    // 実際のレイアウトの設定を反映
    let params = JsonObject::new(value)?;
    update_h265_encode_params(&params, &mut config)?;
    // 未知のキーはデフォルトレイアウト側では出さず、ユーザーレイアウト側でのみ警告する
    params.warn_unknown_keys("video_toolbox_h265");

    Ok(config)
}

fn update_h264_encode_params(
    params: &JsonObject<'_, '_>,
    config: &mut EncoderConfig,
) -> Result<(), nojson::JsonParseError> {
    // [NOTE] 以下は後で別途設定するので、ここではパースしない:
    // - width
    // - height
    // - fps_numerator
    // - fps_denominator
    // - average_bitrate

    // [NOTE] 2026.1.0 で以下のフィールドは非対応になったため、指定されても無視する:
    // - use_parallelization (Video Toolbox 側で撤廃された)
    // - allow_open_gop (H.264 側では効かないため HevcEncoderConfig のみに残った)

    // 2026.1.0 で prioritize_speed_over_quality -> prioritize_encoding_speed_over_quality にリネーム
    if let Some(v) = params.get::<bool>("prioritize_speed_over_quality")? {
        config.prioritize_encoding_speed_over_quality = v;
    }

    if let Some(v) = params.get::<bool>("real_time")? {
        config.real_time = v;
    }

    if let Some(v) = params.get::<bool>("maximize_power_efficiency")? {
        config.maximize_power_efficiency = v;
    }

    if let Some(v) = params.get::<bool>("allow_temporal_compression")? {
        config.allow_temporal_compression = v;
    }

    // フレーム再順序付けを許可 (false で B フレーム無効化)
    if let Some(v) = params.get::<bool>("allow_frame_reordering")? {
        config.allow_frame_reordering = v;
    }

    // キーフレーム間隔設定（フレーム数）
    if let Some(max_key_frame_interval) = params.get("max_key_frame_interval")? {
        config.max_key_frame_interval = Some(max_key_frame_interval);
    }

    // キーフレーム間隔設定（秒数）
    if let Some(duration) = params.get_with("max_key_frame_interval_duration", |v| {
        Ok(Duration::from_secs_f64(v.try_into()?))
    })? {
        config.max_key_frame_interval_duration = Some(duration);
    }

    // フレーム遅延制限
    if let Some(max_frame_delay_count) = params.get("max_frame_delay_count")? {
        config.max_frame_delay_count = Some(max_frame_delay_count);
    }

    // データレートのハードリミット (2026.2.0-canary.2 で追加)
    if let Some(limits) = params.get_with("data_rate_limits", parse_data_rate_limits)? {
        config.data_rate_limits = Some(limits);
    }

    // 2026.1.0 で CodecConfig::H264(H264EncoderConfig) にネストされた
    let CodecConfig::H264(codec) = &mut config.codec else {
        // default_h264_encoder_config で H264 として初期化しているので、この分岐は起きない
        unreachable!("video_toolbox encoder config is not H.264");
    };

    // プロファイルレベル設定
    if let Some(v) = params.get_with("profile_level", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "baseline" => Ok(H264Profile::Baseline),
            "main" => Ok(H264Profile::Main),
            "high" => Ok(H264Profile::High),
            _ => Err(v.invalid("unknown 'profile_level' value for H.264")),
        }
    })? {
        codec.profile = v;
    }

    // H.264 エントロピー符号化モード
    if let Some(v) = params.get_with("h264_entropy_mode", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "cavlc" => Ok(H264EntropyMode::Cavlc),
            "cabac" => Ok(H264EntropyMode::Cabac),
            _ => Err(v.invalid("unknown 'h264_entropy_mode' value")),
        }
    })? {
        codec.entropy_mode = v;
    }

    Ok(())
}

fn update_h265_encode_params(
    params: &JsonObject<'_, '_>,
    config: &mut EncoderConfig,
) -> Result<(), nojson::JsonParseError> {
    // [NOTE] 以下は後で別途設定するので、ここではパースしない:
    // - width
    // - height
    // - fps_numerator
    // - fps_denominator
    // - average_bitrate

    // [NOTE] 2026.1.0 で use_parallelization は Video Toolbox 側で撤廃された

    // H.265 ではこれが false だとエラーになるため、常に true を指定する
    config.prioritize_encoding_speed_over_quality = true;

    if let Some(v) = params.get::<bool>("real_time")? {
        config.real_time = v;
    }

    if let Some(v) = params.get::<bool>("maximize_power_efficiency")? {
        config.maximize_power_efficiency = v;
    }

    if let Some(v) = params.get::<bool>("allow_temporal_compression")? {
        config.allow_temporal_compression = v;
    }

    // フレーム再順序付けを許可 (false で B フレーム無効化)
    if let Some(v) = params.get::<bool>("allow_frame_reordering")? {
        config.allow_frame_reordering = v;
    }

    // キーフレーム間隔設定（フレーム数）
    if let Some(max_key_frame_interval) = params.get("max_key_frame_interval")? {
        config.max_key_frame_interval = Some(max_key_frame_interval);
    }

    // キーフレーム間隔設定（秒数）
    if let Some(duration) = params.get_with("max_key_frame_interval_duration", |v| {
        Ok(Duration::from_secs_f64(v.try_into()?))
    })? {
        config.max_key_frame_interval_duration = Some(duration);
    }

    // フレーム遅延制限
    if let Some(max_frame_delay_count) = params.get("max_frame_delay_count")? {
        config.max_frame_delay_count = Some(max_frame_delay_count);
    }

    // データレートのハードリミット (2026.2.0-canary.2 で追加)
    if let Some(limits) = params.get_with("data_rate_limits", parse_data_rate_limits)? {
        config.data_rate_limits = Some(limits);
    }

    // 2026.1.0 で CodecConfig::Hevc(HevcEncoderConfig) にネストされた
    let CodecConfig::Hevc(codec) = &mut config.codec else {
        // default_h265_encoder_config で Hevc として初期化しているので、この分岐は起きない
        unreachable!("video_toolbox encoder config is not H.265");
    };

    // allow_open_gop は HevcEncoderConfig にのみ残ったので H.265 側でパースする
    if let Some(v) = params.get::<bool>("allow_open_gop")? {
        codec.allow_open_gop = v;
    }

    // プロファイルレベル設定 (H.265)
    if let Some(v) = params.get_with("profile_level", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "main" => Ok(HevcProfile::Main),
            "main10" => Ok(HevcProfile::Main10),
            _ => Err(v.invalid("unknown 'profile_level' value for H.265")),
        }
    })? {
        codec.profile = v;
    }

    Ok(())
}

fn default_h264_encoder_config() -> EncoderConfig {
    default_encoder_config(CodecConfig::H264(H264EncoderConfig {
        profile: H264Profile::Main,
        entropy_mode: H264EntropyMode::Cabac,
    }))
}

fn default_h265_encoder_config() -> EncoderConfig {
    default_encoder_config(CodecConfig::Hevc(HevcEncoderConfig {
        profile: HevcProfile::Main,
        allow_open_gop: true,
    }))
}

fn default_encoder_config(codec: CodecConfig) -> EncoderConfig {
    // width / height / average_bitrate / fps は encoder_video_toolbox.rs 側で実値に上書きするため、
    // ここではダミー値でコンストラクトする
    EncoderConfig {
        width: 640,
        height: 480,
        codec,
        pixel_format: PixelFormat::I420,
        average_bitrate: Some(5_000_000),
        fps_numerator: 30,
        fps_denominator: 1,
        prioritize_encoding_speed_over_quality: false,
        real_time: false,
        maximize_power_efficiency: false,
        allow_frame_reordering: false,
        allow_temporal_compression: true,
        max_key_frame_interval: None,
        max_key_frame_interval_duration: None,
        max_frame_delay_count: None,
        data_rate_limits: None,
    }
}

/// `data_rate_limits` をパースする
///
/// JSON 形式 (Video Toolbox の仕様上 0〜2 個):
///
/// ```json
/// [
///   { "bytes": 125000, "window_seconds": 1 },
///   { "bytes": 62500, "window_seconds": 0.5 }
/// ]
/// ```
fn parse_data_rate_limits(
    v: nojson::RawJsonValue<'_, '_>,
) -> Result<Vec<DataRateLimit>, nojson::JsonParseError> {
    v.to_array()?
        .map(|limit| {
            let obj = JsonObject::new(limit)?;
            let bytes = obj.get_required::<u64>("bytes")?;
            let window_seconds = obj.get_required::<f64>("window_seconds")?;
            Ok(DataRateLimit {
                bytes,
                window: Duration::from_secs_f64(window_seconds),
            })
        })
        .collect()
}
