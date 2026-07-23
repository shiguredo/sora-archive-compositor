use std::path::PathBuf;
use std::time::Duration;

pub fn parse_non_default_opt<T>(opt: noargs::Opt) -> Result<Option<T>, T::Err>
where
    T: std::str::FromStr,
{
    if matches!(opt, noargs::Opt::Default { .. }) {
        Ok(None)
    } else {
        opt.value().parse().map(Some)
    }
}

pub fn validate_existing_directory_path(
    arg: noargs::Arg,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path: PathBuf = arg.value().parse()?;

    if matches!(arg, noargs::Arg::Example { .. }) {
        // ここに来るのは --help によるヘルプ表示の時なのでチェックは不要
    } else if !path.exists() {
        return Err("no such directory".into());
    } else if !path.is_dir() {
        return Err("not a directory".into());
    }

    Ok(path)
}

/// CLI 引数の秒数文字列を `Duration` に変換する
///
/// 負数・NaN・無限大は `"not a non negative finite number"` を返す。
/// 有限かつ非負でも `Duration` に収まらない値は `"number overflows duration"` を返す。
pub fn parse_duration_secs(s: &str) -> Result<Duration, Box<dyn std::error::Error>> {
    let secs: f32 = s.parse()?;
    if !secs.is_finite() || secs < 0.0 {
        return Err("not a non negative finite number".into());
    }
    // is_finite かつ非負を確認してから from_secs_f32 に渡すと、
    // 1e20 のような有限の巨大数で panic する
    Duration::try_from_secs_f32(secs).map_err(|_| "number overflows duration".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_negative_without_panic() {
        let err = parse_duration_secs("-1").expect_err("負数はパースエラーになる");
        assert!(
            err.to_string().contains("not a non negative finite number"),
            "想定と異なるエラー: {err}"
        );
    }

    #[test]
    fn rejects_nan_without_panic() {
        let err = parse_duration_secs("NaN").expect_err("NaN はパースエラーになる");
        assert!(
            err.to_string().contains("not a non negative finite number"),
            "想定と異なるエラー: {err}"
        );
    }

    #[test]
    fn rejects_inf_without_panic() {
        let err = parse_duration_secs("inf").expect_err("無限大はパースエラーになる");
        assert!(
            err.to_string().contains("not a non negative finite number"),
            "想定と異なるエラー: {err}"
        );
    }

    #[test]
    fn rejects_overflowing_finite_number_without_panic() {
        // 1e20 は f32 では有限かつ非負だが Duration 上限を超える
        let err = parse_duration_secs("1e20").expect_err("巨大数はパースエラーになる");
        assert!(
            err.to_string().contains("number overflows duration"),
            "想定と異なるエラー: {err}"
        );
        assert!(
            !err.to_string().contains("not a non negative finite number"),
            "溢れを非負有限の失敗として扱ってはいけない: {err}"
        );
    }

    #[test]
    fn parses_normal_seconds() {
        let duration = parse_duration_secs("1.5").expect("通常の秒数のパースに失敗した");
        assert_eq!(
            duration,
            Duration::try_from_secs_f32(1.5).expect("1.5 秒は Duration に入る")
        );
    }
}
