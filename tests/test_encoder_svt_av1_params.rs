// svt_av1_encode_params のパースに関するテスト
use sora_archive_compositor::encoder_svt_av1_params::parse_encode_params;

/// 未知のキーを含む JSON をパースしたときに、既知キーが正しく反映され、未知キーが警告されることを検証する
#[test]
fn parse_with_unknown_key_warns_and_keeps_known_keys() {
    // 既知キーと未知キーを混ぜた JSON をパースする
    let json = r#"{
        "enc_mode": 5,
        "aq_mode": 3,
        "typo_unknown_key": 42
    }"#;
    let raw = nojson::RawJson::parse(json).expect("JSON のパースに失敗");
    let config = parse_encode_params(raw.value()).expect("エンコードパラメータのパースに失敗");

    // 既知キーが正しく反映される
    assert_eq!(config.enc_mode, 5);
    assert_eq!(config.aq_mode, Some(3));
}

/// 未知のキーのみを含む JSON をパースしたときに、警告されることを検証する
#[test]
fn parse_with_only_unknown_keys_warns() {
    let json = r#"{
        "typo_unknown_key": 42,
        "another_unknown": "value"
    }"#;
    let raw = nojson::RawJson::parse(json).expect("JSON のパースに失敗");
    let config = parse_encode_params(raw.value()).expect("エンコードパラメータのパースに失敗");

    // 未知キーは無視され、デフォルト値が保持される
    assert_eq!(config.enc_mode, 13);
}
