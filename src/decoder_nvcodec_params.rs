use crate::json::JsonObject;
use crate::layout::DEFAULT_LAYOUT_JSON;

pub fn parse_h264_decode_params(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<shiguredo_nvcodec::DecoderConfig, nojson::JsonParseError> {
    parse_decode_params(
        value,
        "nvcodec_h264_decode_params",
        shiguredo_nvcodec::DecoderCodec::H264,
    )
}

pub fn parse_h265_decode_params(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<shiguredo_nvcodec::DecoderConfig, nojson::JsonParseError> {
    parse_decode_params(
        value,
        "nvcodec_h265_decode_params",
        shiguredo_nvcodec::DecoderCodec::Hevc,
    )
}

pub fn parse_av1_decode_params(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<shiguredo_nvcodec::DecoderConfig, nojson::JsonParseError> {
    parse_decode_params(
        value,
        "nvcodec_av1_decode_params",
        shiguredo_nvcodec::DecoderCodec::Av1,
    )
}

pub fn parse_vp8_decode_params(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<shiguredo_nvcodec::DecoderConfig, nojson::JsonParseError> {
    parse_decode_params(
        value,
        "nvcodec_vp8_decode_params",
        shiguredo_nvcodec::DecoderCodec::Vp8,
    )
}

pub fn parse_vp9_decode_params(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<shiguredo_nvcodec::DecoderConfig, nojson::JsonParseError> {
    parse_decode_params(
        value,
        "nvcodec_vp9_decode_params",
        shiguredo_nvcodec::DecoderCodec::Vp9,
    )
}

fn parse_decode_params(
    value: nojson::RawJsonValue<'_, '_>,
    default_key: &'static str,
    codec: shiguredo_nvcodec::DecoderCodec,
) -> Result<shiguredo_nvcodec::DecoderConfig, nojson::JsonParseError> {
    // 2026.2.0 で DecoderConfig::default() は撤廃され、codec / surface_format 等を明示的に指定する必要がある
    // 2026.3.0-canary.0 で reconfigure_enabled が追加された (推奨値は false)
    let mut config = shiguredo_nvcodec::DecoderConfig {
        codec,
        device_id: 0,
        // 一般的な既定値
        max_num_decode_surfaces: 4,
        max_display_delay: 0,
        reconfigure_enabled: false,
        // 現状 shiguredo_nvcodec は NV12 のみサポート
        surface_format: shiguredo_nvcodec::SurfaceFormat::Nv12,
    };

    // デフォルトレイアウトの設定を反映
    let default = nojson::RawJson::parse_jsonc(DEFAULT_LAYOUT_JSON)?.0;
    let params = JsonObject::new(default.value().to_member(default_key)?.required()?)?;
    update_decode_params(params, &mut config)?;

    // 実際のレイアウトの設定を反映
    let params = JsonObject::new(value)?;
    update_decode_params(params, &mut config)?;

    Ok(config)
}

fn update_decode_params(
    params: JsonObject<'_, '_>,
    config: &mut shiguredo_nvcodec::DecoderConfig,
) -> Result<(), nojson::JsonParseError> {
    // デバイス ID
    if let Some(v) = params.get::<i32>("device_id")? {
        config.device_id = v;
    }

    // デコード用サーフェスの最大数
    if let Some(v) = params.get::<u32>("max_num_decode_surfaces")? {
        config.max_num_decode_surfaces = v;
    }

    // 表示遅延
    if let Some(v) = params.get::<u32>("max_display_delay")? {
        config.max_display_delay = v;
    }

    // 解像度変化時に cuvidReconfigureDecoder を使うかどうか (推奨値は false)
    if let Some(v) = params.get::<bool>("reconfigure_enabled")? {
        config.reconfigure_enabled = v;
    }

    Ok(())
}
