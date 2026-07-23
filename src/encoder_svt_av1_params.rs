use crate::json::JsonObject;
use crate::layout::DEFAULT_LAYOUT_JSON;

pub fn parse_encode_params(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<shiguredo_svt_av1::EncoderConfig, nojson::JsonParseError> {
    // width / height は encoder_svt_av1.rs 側で実値に上書きするため、ここではダミー値でコンストラクトする
    let mut config =
        shiguredo_svt_av1::EncoderConfig::new(0, 0, shiguredo_svt_av1::ColorFormat::I420);

    // デフォルトレイアウトの設定を反映
    let default = nojson::RawJson::parse_jsonc(DEFAULT_LAYOUT_JSON)?.0;
    let params = JsonObject::new(
        default
            .value()
            .to_member("svt_av1_encode_params")?
            .required()?,
    )?;
    update_encode_params(&params, &mut config)?;

    // 実際のレイアウトの設定を反映
    let params = JsonObject::new(value)?;
    update_encode_params(&params, &mut config)?;
    // 未知のキーはデフォルトレイアウト側では出さず、ユーザーレイアウト側でのみ警告する
    // (デフォルト側の未知キーは開発時に気付くべき問題)
    params.warn_unknown_keys("svt_av1");

    Ok(config)
}

/// 旧 JSON 互換: Boolean が来た場合に u8 (false=0, true=1) として受け付ける
fn to_u8_from_bool_or_int(v: nojson::RawJsonValue<'_, '_>) -> Result<u8, nojson::JsonParseError> {
    if let Ok(b) = <bool as TryFrom<nojson::RawJsonValue<'_, '_>>>::try_from(v) {
        Ok(u8::from(b))
    } else {
        v.try_into()
    }
}

/// 旧 JSON 互換: Boolean が来た場合に i32 (false=0, true=1) として受け付ける
fn to_i32_from_bool_or_int(v: nojson::RawJsonValue<'_, '_>) -> Result<i32, nojson::JsonParseError> {
    if let Ok(b) = <bool as TryFrom<nojson::RawJsonValue<'_, '_>>>::try_from(v) {
        Ok(i32::from(b))
    } else {
        v.try_into()
    }
}

fn update_encode_params(
    params: &JsonObject<'_, '_>,
    config: &mut shiguredo_svt_av1::EncoderConfig,
) -> Result<(), nojson::JsonParseError> {
    // [NOTE] 以下は後で別途設定するので、ここではパースしない:
    // - width
    // - height
    // - fps_numerator
    // - fps_denominator
    // - target_bit_rate

    // [NOTE] 以下は外部 crate 化で削除されたフィールドなので、JSON に来ても無視する:
    // - pred_structure / pin_threads / target_socket / enable_tpl_la / force_key_frames
    // - recon_enabled / encoder_bit_depth / encoder_color_format / profile / level / tier

    // === 品質・速度制御関連 ===
    if let Some(v) = params.get::<u8>("enc_mode")? {
        config.enc_mode = v;
    }
    if let Some(v) = params.get::<u8>("qp")? {
        config.qp = Some(v);
    }
    if let Some(v) = params.get::<u8>("min_qp_allowed")? {
        config.min_qp_allowed = Some(v);
    }
    if let Some(v) = params.get::<u8>("max_qp_allowed")? {
        config.max_qp_allowed = Some(v);
    }

    // === レート制御関連 ===
    if let Some(v) = params.get_with("rate_control_mode", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "cqp_or_crf" => Ok(shiguredo_svt_av1::RcMode::CqpOrCrf),
            "vbr" => Ok(shiguredo_svt_av1::RcMode::Vbr),
            "cbr" => Ok(shiguredo_svt_av1::RcMode::Cbr),
            _ => Err(v.invalid("unknown 'rate_control_mode' value")),
        }
    })? {
        config.rate_control_mode = v;
    }

    if let Some(v) = params.get::<usize>("max_bit_rate")? {
        config.max_bit_rate = Some(v);
    }
    if let Some(v) = params.get::<u32>("over_shoot_pct")? {
        config.over_shoot_pct = Some(v);
    }
    if let Some(v) = params.get::<u32>("under_shoot_pct")? {
        config.under_shoot_pct = Some(v);
    }

    // === GOP とフレーム構造関連 ===
    if let Some(v) = params.get("intra_period_length")? {
        config.intra_period_length = Some(v);
    }
    if let Some(v) = params.get::<u32>("hierarchical_levels")? {
        config.hierarchical_levels = Some(v);
    }
    if let Some(v) = params.get::<bool>("scene_change_detection")? {
        config.scene_change_detection = v;
    }
    if let Some(v) = params.get::<usize>("look_ahead_distance")? {
        config.look_ahead_distance = Some(v);
    }

    // === 並列処理関連 ===
    if let Some(v) = params.get("tile_columns")? {
        config.tile_columns = Some(v);
    }
    if let Some(v) = params.get("tile_rows")? {
        config.tile_rows = Some(v);
    }

    // === フィルタリング関連 ===
    // 旧 API では整数型と Boolean が混在していたため、Boolean も整数として受け付ける
    if let Some(v) = params.get_with("enable_dlf_flag", to_u8_from_bool_or_int)? {
        config.enable_dlf_flag = Some(v);
    }
    if let Some(v) = params.get_with("cdef_level", to_i32_from_bool_or_int)? {
        config.cdef_level = Some(v);
    }
    if let Some(v) = params.get_with("enable_restoration_filtering", to_i32_from_bool_or_int)? {
        config.enable_restoration_filtering = Some(v);
    }

    // === 高度な設定 ===
    if let Some(v) = params.get_with("enable_tf", to_u8_from_bool_or_int)? {
        config.enable_tf = Some(v);
    }
    if let Some(v) = params.get::<bool>("enable_overlays")? {
        config.enable_overlays = Some(v);
    }
    if let Some(v) = params.get::<u32>("film_grain_denoise_strength")? {
        config.film_grain_denoise_strength = Some(v);
    }
    if let Some(v) = params.get::<bool>("stat_report")? {
        config.stat_report = v;
    }

    // === エンコーダー固有設定 ===
    if let Some(v) = params.get_with("color_format", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "i420" => Ok(shiguredo_svt_av1::ColorFormat::I420),
            "i42010" => Ok(shiguredo_svt_av1::ColorFormat::I42010),
            _ => Err(v.invalid("unknown 'color_format' value")),
        }
    })? {
        config.color_format = v;
    }

    if let Some(v) = params.get_with("fast_decode", to_u8_from_bool_or_int)? {
        config.fast_decode = Some(v);
    }

    // === 2026.2.0-canary.0 で追加されたパラメーター ===

    // 品質・速度制御関連
    if let Some(v) = params.get::<u8>("aq_mode")? {
        config.aq_mode = Some(v);
    }
    if let Some(v) = params.get::<i8>("sharpness")? {
        config.sharpness = Some(v);
    }
    if let Some(v) = params.get::<bool>("rtc")? {
        config.rtc = Some(v);
    }
    if let Some(v) = params.get_with("tune", |v| match v.to_unquoted_string_str()?.as_ref() {
        "vq" => Ok(shiguredo_svt_av1::Tune::Vq),
        "psnr" => Ok(shiguredo_svt_av1::Tune::Psnr),
        "ssim" => Ok(shiguredo_svt_av1::Tune::Ssim),
        "iq" => Ok(shiguredo_svt_av1::Tune::Iq),
        "ms_ssim" => Ok(shiguredo_svt_av1::Tune::MsSsim),
        _ => Err(v.invalid("unknown 'tune' value")),
    })? {
        config.tune = Some(v);
    }
    if let Some(v) = params.get_with("intra_refresh_type", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "fwdkf_refresh" => Ok(shiguredo_svt_av1::IntraRefreshType::FwdkfRefresh),
            "kf_refresh" => Ok(shiguredo_svt_av1::IntraRefreshType::KfRefresh),
            _ => Err(v.invalid("unknown 'intra_refresh_type' value")),
        }
    })? {
        config.intra_refresh_type = Some(v);
    }
    if let Some(v) = params.get::<u8>("screen_content_mode")? {
        config.screen_content_mode = Some(v);
    }
    if let Some(v) = params.get::<bool>("lossless")? {
        config.lossless = Some(v);
    }
    if let Some(v) = params.get::<f64>("ac_bias")? {
        config.ac_bias = Some(v);
    }
    if let Some(v) = params.get::<u32>("level_of_parallelism")? {
        config.level_of_parallelism = Some(v);
    }

    // レート制御関連
    if let Some(v) = params.get::<u32>("vbr_min_section_pct")? {
        config.vbr_min_section_pct = Some(v);
    }
    if let Some(v) = params.get::<u32>("vbr_max_section_pct")? {
        config.vbr_max_section_pct = Some(v);
    }
    if let Some(v) = params.get::<u32>("mbr_over_shoot_pct")? {
        config.mbr_over_shoot_pct = Some(v);
    }
    if let Some(v) = params.get::<u32>("recode_loop")? {
        config.recode_loop = Some(v);
    }
    if let Some(v) = params.get::<u64>("starting_buffer_level_ms")? {
        config.starting_buffer_level_ms = Some(v);
    }
    if let Some(v) = params.get::<u64>("optimal_buffer_level_ms")? {
        config.optimal_buffer_level_ms = Some(v);
    }
    if let Some(v) = params.get::<u64>("maximum_buffer_size_ms")? {
        config.maximum_buffer_size_ms = Some(v);
    }

    // GOP とフレーム構造関連
    if let Some(v) = params.get::<i32>("sframe_dist")? {
        config.sframe_dist = Some(v);
    }
    if let Some(v) = params.get::<u32>("sframe_mode")? {
        config.sframe_mode = Some(v);
    }
    if let Some(v) = params.get::<u8>("sframe_qp")? {
        config.sframe_qp = Some(v);
    }
    if let Some(v) = params.get::<i8>("sframe_qp_offset")? {
        config.sframe_qp_offset = Some(v);
    }
    if let Some(v) = params.get::<bool>("gop_constraint_rc")? {
        config.gop_constraint_rc = Some(v);
    }
    if let Some(v) = params.get::<bool>("multiply_keyint")? {
        config.multiply_keyint = Some(v);
    }

    // スーパーレゾリューション関連
    if let Some(v) = params.get::<u8>("superres_mode")? {
        config.superres_mode = Some(v);
    }
    if let Some(v) = params.get::<u8>("superres_denom")? {
        config.superres_denom = Some(v);
    }
    if let Some(v) = params.get::<u8>("superres_kf_denom")? {
        config.superres_kf_denom = Some(v);
    }
    if let Some(v) = params.get::<u8>("superres_qthres")? {
        config.superres_qthres = Some(v);
    }
    if let Some(v) = params.get::<u8>("superres_kf_qthres")? {
        config.superres_kf_qthres = Some(v);
    }
    if let Some(v) = params.get::<u8>("superres_auto_search_type")? {
        config.superres_auto_search_type = Some(v);
    }

    // リサイズ関連
    if let Some(v) = params.get::<u8>("resize_mode")? {
        config.resize_mode = Some(v);
    }
    if let Some(v) = params.get::<u8>("resize_denom")? {
        config.resize_denom = Some(v);
    }
    if let Some(v) = params.get::<u8>("resize_kf_denom")? {
        config.resize_kf_denom = Some(v);
    }

    // フィルタリング関連
    if let Some(v) = params.get::<u8>("tf_strength")? {
        config.tf_strength = Some(v);
    }
    if let Some(v) = params.get::<bool>("enable_variance_boost")? {
        config.enable_variance_boost = Some(v);
    }
    if let Some(v) = params.get::<u8>("variance_boost_strength")? {
        config.variance_boost_strength = Some(v);
    }
    if let Some(v) = params.get::<u8>("variance_octile")? {
        config.variance_octile = Some(v);
    }
    if let Some(v) = params.get::<u8>("variance_boost_curve")? {
        config.variance_boost_curve = Some(v);
    }
    if let Some(v) = params.get::<u8>("film_grain_denoise_apply")? {
        config.film_grain_denoise_apply = Some(v);
    }
    if let Some(v) = params.get::<bool>("adaptive_film_grain")? {
        config.adaptive_film_grain = Some(v);
    }

    // 量子化・品質関連
    if let Some(v) = params.get::<bool>("enable_qm")? {
        config.enable_qm = Some(v);
    }
    if let Some(v) = params.get::<u8>("min_qm_level")? {
        config.min_qm_level = Some(v);
    }
    if let Some(v) = params.get::<u8>("max_qm_level")? {
        config.max_qm_level = Some(v);
    }
    if let Some(v) = params.get::<u8>("min_chroma_qm_level")? {
        config.min_chroma_qm_level = Some(v);
    }
    if let Some(v) = params.get::<u8>("max_chroma_qm_level")? {
        config.max_chroma_qm_level = Some(v);
    }
    if let Some(v) = params.get::<u8>("max_tx_size")? {
        config.max_tx_size = Some(v);
    }
    if let Some(v) = params.get::<i32>("enable_mfmv")? {
        config.enable_mfmv = Some(v);
    }
    if let Some(v) = params.get::<bool>("enable_dg")? {
        config.enable_dg = Some(v);
    }
    if let Some(v) = params.get::<bool>("avif")? {
        config.avif = Some(v);
    }
    if let Some(v) = params.get::<u8>("startup_mg_size")? {
        config.startup_mg_size = Some(v);
    }
    if let Some(v) = params.get::<i8>("startup_qp_offset")? {
        config.startup_qp_offset = Some(v);
    }
    if let Some(v) = params.get::<u8>("luminance_qp_bias")? {
        config.luminance_qp_bias = Some(v);
    }
    if let Some(v) = params.get::<u8>("qp_scale_compress_strength")? {
        config.qp_scale_compress_strength = Some(v);
    }
    if let Some(v) = params.get::<u8>("extended_crf_qindex_offset")? {
        config.extended_crf_qindex_offset = Some(v);
    }

    // HDR 関連
    if let Some(v) = params.get_with("color_primaries", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "bt709" => Ok(shiguredo_svt_av1::ColorPrimaries::Bt709),
            "unspecified" => Ok(shiguredo_svt_av1::ColorPrimaries::Unspecified),
            "bt470m" => Ok(shiguredo_svt_av1::ColorPrimaries::Bt470M),
            "bt470bg" => Ok(shiguredo_svt_av1::ColorPrimaries::Bt470Bg),
            "bt601" => Ok(shiguredo_svt_av1::ColorPrimaries::Bt601),
            "smpte240" => Ok(shiguredo_svt_av1::ColorPrimaries::Smpte240),
            "generic_film" => Ok(shiguredo_svt_av1::ColorPrimaries::GenericFilm),
            "bt2020" => Ok(shiguredo_svt_av1::ColorPrimaries::Bt2020),
            "xyz" => Ok(shiguredo_svt_av1::ColorPrimaries::Xyz),
            "smpte431" => Ok(shiguredo_svt_av1::ColorPrimaries::Smpte431),
            "smpte432" => Ok(shiguredo_svt_av1::ColorPrimaries::Smpte432),
            "ebu3213" => Ok(shiguredo_svt_av1::ColorPrimaries::Ebu3213),
            _ => Err(v.invalid("unknown 'color_primaries' value")),
        }
    })? {
        config.color_primaries = Some(v);
    }
    if let Some(v) = params.get_with("transfer_characteristics", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "bt709" => Ok(shiguredo_svt_av1::TransferCharacteristics::Bt709),
            "unspecified" => Ok(shiguredo_svt_av1::TransferCharacteristics::Unspecified),
            "bt470m" => Ok(shiguredo_svt_av1::TransferCharacteristics::Bt470M),
            "bt470bg" => Ok(shiguredo_svt_av1::TransferCharacteristics::Bt470Bg),
            "bt601" => Ok(shiguredo_svt_av1::TransferCharacteristics::Bt601),
            "smpte240" => Ok(shiguredo_svt_av1::TransferCharacteristics::Smpte240),
            "linear" => Ok(shiguredo_svt_av1::TransferCharacteristics::Linear),
            "iec61966" => Ok(shiguredo_svt_av1::TransferCharacteristics::Iec61966),
            "bt1361" => Ok(shiguredo_svt_av1::TransferCharacteristics::Bt1361),
            "srgb" => Ok(shiguredo_svt_av1::TransferCharacteristics::Srgb),
            "bt2020_10bit" => Ok(shiguredo_svt_av1::TransferCharacteristics::Bt202010Bit),
            "bt2020_12bit" => Ok(shiguredo_svt_av1::TransferCharacteristics::Bt202012Bit),
            "pq" => Ok(shiguredo_svt_av1::TransferCharacteristics::Pq),
            "smpte428" => Ok(shiguredo_svt_av1::TransferCharacteristics::Smpte428),
            "hlg" => Ok(shiguredo_svt_av1::TransferCharacteristics::Hlg),
            _ => Err(v.invalid("unknown 'transfer_characteristics' value")),
        }
    })? {
        config.transfer_characteristics = Some(v);
    }
    if let Some(v) = params.get_with("matrix_coefficients", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "identity" => Ok(shiguredo_svt_av1::MatrixCoefficients::Identity),
            "bt709" => Ok(shiguredo_svt_av1::MatrixCoefficients::Bt709),
            "unspecified" => Ok(shiguredo_svt_av1::MatrixCoefficients::Unspecified),
            "fcc" => Ok(shiguredo_svt_av1::MatrixCoefficients::Fcc),
            "bt470bg" => Ok(shiguredo_svt_av1::MatrixCoefficients::Bt470Bg),
            "bt601" => Ok(shiguredo_svt_av1::MatrixCoefficients::Bt601),
            "smpte240" => Ok(shiguredo_svt_av1::MatrixCoefficients::Smpte240),
            "ycgco" => Ok(shiguredo_svt_av1::MatrixCoefficients::Ycgco),
            "bt2020_ncl" => Ok(shiguredo_svt_av1::MatrixCoefficients::Bt2020Ncl),
            "bt2020_cl" => Ok(shiguredo_svt_av1::MatrixCoefficients::Bt2020Cl),
            "smpte2085" => Ok(shiguredo_svt_av1::MatrixCoefficients::Smpte2085),
            "chromat_ncl" => Ok(shiguredo_svt_av1::MatrixCoefficients::ChromatNcl),
            "chromat_cl" => Ok(shiguredo_svt_av1::MatrixCoefficients::ChromatCl),
            "ictcp" => Ok(shiguredo_svt_av1::MatrixCoefficients::Ictcp),
            _ => Err(v.invalid("unknown 'matrix_coefficients' value")),
        }
    })? {
        config.matrix_coefficients = Some(v);
    }
    if let Some(v) = params.get_with("color_range", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "studio" => Ok(shiguredo_svt_av1::ColorRange::Studio),
            "full" => Ok(shiguredo_svt_av1::ColorRange::Full),
            _ => Err(v.invalid("unknown 'color_range' value")),
        }
    })? {
        config.color_range = Some(v);
    }
    if let Some(v) = params.get_with("chroma_sample_position", |v| {
        match v.to_unquoted_string_str()?.as_ref() {
            "unknown" => Ok(shiguredo_svt_av1::ChromaSamplePosition::Unknown),
            "vertical" => Ok(shiguredo_svt_av1::ChromaSamplePosition::Vertical),
            "colocated" => Ok(shiguredo_svt_av1::ChromaSamplePosition::Colocated),
            _ => Err(v.invalid("unknown 'chroma_sample_position' value")),
        }
    })? {
        config.chroma_sample_position = Some(v);
    }
    if let Some(v) = params.get_with("mastering_display", |v| {
        let obj = JsonObject::new(v)?;
        let r = obj.get_required_with("r", parse_chromaticity)?;
        let g = obj.get_required_with("g", parse_chromaticity)?;
        let b = obj.get_required_with("b", parse_chromaticity)?;
        let white_point = obj.get_required_with("white_point", parse_chromaticity)?;
        let max_luminance = obj.get_required::<u32>("max_luminance")?;
        let min_luminance = obj.get_required::<u32>("min_luminance")?;
        Ok(shiguredo_svt_av1::MasteringDisplayInfo {
            r,
            g,
            b,
            white_point,
            max_luminance,
            min_luminance,
        })
    })? {
        config.mastering_display = Some(v);
    }
    if let Some(v) = params.get_with("content_light_level", |v| {
        let obj = JsonObject::new(v)?;
        Ok(shiguredo_svt_av1::ContentLightLevel {
            max_cll: obj.get_required::<u16>("max_cll")?,
            max_fall: obj.get_required::<u16>("max_fall")?,
        })
    })? {
        config.content_light_level = Some(v);
    }

    Ok(())
}

/// 色度座標 (x, y) のペアをパースする
fn parse_chromaticity(
    v: nojson::RawJsonValue<'_, '_>,
) -> Result<(u16, u16), nojson::JsonParseError> {
    let mut array = v.to_array()?;
    let x = array
        .next()
        .ok_or_else(|| v.invalid("expected [x, y] chromaticity pair"))?;
    let y = array
        .next()
        .ok_or_else(|| v.invalid("expected [x, y] chromaticity pair"))?;
    if array.next().is_some() {
        return Err(v.invalid("expected [x, y] chromaticity pair"));
    }
    Ok((x.try_into()?, y.try_into()?))
}
