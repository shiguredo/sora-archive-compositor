use crate::json::JsonObject;
use crate::layout::DEFAULT_LAYOUT_JSON;

pub fn parse_encode_params(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<shiguredo_openh264::EncoderConfig, nojson::JsonParseError> {
    // width / height / target_bitrate / fps は encoder_openh264.rs 側で実値を上書きするため、
    // ここではダミー値でコンストラクトする
    let mut config = shiguredo_openh264::EncoderConfig::new(1, 1, 1, 1, 1);

    // デフォルトレイアウトの設定を反映
    let default = nojson::RawJson::parse_jsonc(DEFAULT_LAYOUT_JSON)?.0;
    let params = JsonObject::new(
        default
            .value()
            .to_member("openh264_encode_params")?
            .required()?,
    )?;
    update_encode_params(&params, &mut config)?;

    // 実際のレイアウトの設定を反映
    let params = JsonObject::new(value)?;
    update_encode_params(&params, &mut config)?;
    // 未知のキーはデフォルトレイアウト側では出さず、ユーザーレイアウト側でのみ警告する
    params.warn_unknown_keys("openh264");

    Ok(config)
}

fn update_encode_params(
    params: &JsonObject<'_, '_>,
    config: &mut shiguredo_openh264::EncoderConfig,
) -> Result<(), nojson::JsonParseError> {
    // [NOTE] 以下は後で別途設定するので、ここではパースしない:
    // - width
    // - height
    // - fps_numerator
    // - fps_denominator
    // - target_bitrate

    // 基本的なエンコーダーパラメーター
    if let Some(v) = params.get::<usize>("max_qp")? {
        config.max_qp = Some(v);
    }
    if let Some(v) = params.get::<usize>("min_qp")? {
        config.min_qp = Some(v);
    }

    // 複雑度モード
    if let Some(v) = params.get_with("complexity_mode", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "low" => Ok(shiguredo_openh264::ComplexityMode::Low),
            "medium" => Ok(shiguredo_openh264::ComplexityMode::Medium),
            "high" => Ok(shiguredo_openh264::ComplexityMode::High),
            _ => Err(v.invalid("unknown 'complexity_mode' value")),
        }
    })? {
        config.complexity_mode = Some(v);
    }

    // エントロピー符号化モード。2026.1.0 で名称が entropy_coding -> entropy_coding_mode に変わり、
    // 値も bool から enum ("cavlc" / "cabac") に変わったので新旧両方の JSON キーを受け付ける
    if let Some(v) = params.get_with("entropy_coding_mode", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "cavlc" => Ok(shiguredo_openh264::EntropyCodingMode::Cavlc),
            "cabac" => Ok(shiguredo_openh264::EntropyCodingMode::Cabac),
            _ => Err(v.invalid("unknown 'entropy_coding_mode' value")),
        }
    })? {
        config.entropy_coding_mode = Some(v);
    } else if let Some(enabled) = params.get::<bool>("entropy_coding")? {
        // 互換のため旧キー entropy_coding (bool) も受け付ける
        config.entropy_coding_mode = Some(if enabled {
            shiguredo_openh264::EntropyCodingMode::Cabac
        } else {
            shiguredo_openh264::EntropyCodingMode::Cavlc
        });
    }

    // 参照フレーム数
    if let Some(v) = params.get("ref_frame_count")? {
        config.ref_frame_count = Some(v);
    }

    // スレッド数
    if let Some(v) = params.get("thread_count")? {
        config.thread_count = Some(v);
    }

    // 空間レイヤー数
    if let Some(v) = params.get("spatial_layers")? {
        config.spatial_layers = Some(v);
    }

    // 時間レイヤー数
    if let Some(v) = params.get("temporal_layers")? {
        config.temporal_layers = Some(v);
    }

    // Intra フレーム間隔
    if let Some(v) = params.get::<usize>("intra_period")? {
        config.intra_period = Some(v);
    }

    // レート制御モード
    if let Some(v) = params.get_with("rate_control_mode", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "off" => Ok(shiguredo_openh264::RateControlMode::Off),
            "quality" => Ok(shiguredo_openh264::RateControlMode::Quality),
            "bitrate" => Ok(shiguredo_openh264::RateControlMode::Bitrate),
            "timestamp" => Ok(shiguredo_openh264::RateControlMode::Timestamp),
            _ => Err(v.invalid("unknown 'rate_control_mode' value")),
        }
    })? {
        config.rate_control_mode = Some(v);
    }

    // 前処理機能設定
    if let Some(v) = params.get::<bool>("denoise")? {
        config.denoise = Some(v);
    }
    if let Some(v) = params.get::<bool>("background_detection")? {
        config.background_detection = Some(v);
    }
    if let Some(v) = params.get::<bool>("adaptive_quantization")? {
        config.adaptive_quantization = Some(v);
    }
    if let Some(v) = params.get::<bool>("scene_change_detection")? {
        config.scene_change_detection = Some(v);
    }
    if let Some(v) = params.get::<bool>("deblocking_filter")? {
        config.deblocking_filter = Some(v);
    }
    if let Some(v) = params.get::<bool>("long_term_reference")? {
        config.long_term_reference = Some(v);
    }

    // スライスモード
    if let Some(v) = params.get_with("slice_mode", |v| {
        let slice_obj = JsonObject::new(v)?;
        let mode_type: String = slice_obj.get_required("type")?;
        match mode_type.as_str() {
            "single" => Ok(shiguredo_openh264::SliceMode::Single),
            "fixed_count" => {
                let count = slice_obj.get_required("count")?;
                Ok(shiguredo_openh264::SliceMode::FixedCount(count))
            }
            "size_constrained" => {
                let size = slice_obj.get_required("size")?;
                Ok(shiguredo_openh264::SliceMode::SizeConstrained(size))
            }
            _ => Err(v.invalid("unknown 'slice_mode.type' value")),
        }
    })? {
        config.slice_mode = Some(v);
    }

    Ok(())
}
