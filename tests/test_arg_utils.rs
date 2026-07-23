// CLI の --timeout / --trial-timeout が noargs のパースエラーになることを確認する
use std::process::Command;

fn assert_parse_error(args: &[&str], expected: &str) {
    let bin = env!("CARGO_BIN_EXE_sora-archive-compositor");
    let output = Command::new(bin)
        .args(args)
        .output()
        .expect("コマンドの起動に失敗した");
    assert!(
        output.status.code().is_some(),
        "panic でシグナル終了した: {:?}",
        output.status
    );
    assert!(!output.status.success(), "パースエラーになる想定が成功した");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains(expected),
        "想定と異なるエラー: {combined}"
    );
}

#[test]
fn vmaf_timeout_rejects_negative_without_panic() {
    assert_parse_error(
        &["vmaf", "--timeout=-1"],
        "not a non negative finite number",
    );
}

#[test]
fn vmaf_timeout_rejects_nan_without_panic() {
    assert_parse_error(
        &["vmaf", "--timeout=NaN"],
        "not a non negative finite number",
    );
}

#[test]
fn vmaf_timeout_rejects_inf_without_panic() {
    assert_parse_error(
        &["vmaf", "--timeout=inf"],
        "not a non negative finite number",
    );
}

#[test]
fn vmaf_timeout_rejects_overflow_without_panic() {
    // 1e20 は f32 では有限かつ非負だが Duration 上限を超える
    assert_parse_error(&["vmaf", "--timeout=1e20"], "number overflows duration");
}

#[test]
fn tune_trial_timeout_rejects_negative_without_panic() {
    assert_parse_error(
        &["tune", "--trial-timeout=-1"],
        "not a non negative finite number",
    );
}

#[test]
fn tune_trial_timeout_rejects_overflow_without_panic() {
    assert_parse_error(
        &["tune", "--trial-timeout=1e20"],
        "number overflows duration",
    );
}

/// 削除した `--max-cpu-cores` は未知オプションとして拒否される
#[test]
fn vmaf_rejects_removed_max_cpu_cores() {
    assert_parse_error(
        &["vmaf", ".", "--max-cpu-cores", "1"],
        "unexpected argument '--max-cpu-cores'",
    );
}

/// 削除した `--max-cpu-cores` は未知オプションとして拒否される
#[test]
fn tune_rejects_removed_max_cpu_cores() {
    assert_parse_error(
        &["tune", ".", "--max-cpu-cores", "1"],
        "unexpected argument '--max-cpu-cores'",
    );
}
