//! 雑多な型定義をまとめたモジュール
use std::str::FromStr;
use std::time::Duration;

/// コーデック名
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CodecName {
    // 音声
    Aac,
    Opus,

    // 映像
    H264,
    H265,
    Vp8,
    Vp9,
    Av1,
}

impl nojson::DisplayJson for CodecName {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.value(self.as_str())
    }
}

impl CodecName {
    pub fn as_str(self) -> &'static str {
        match self {
            CodecName::Opus => "OPUS",
            CodecName::Aac => "AAC",
            CodecName::H264 => "H264",
            CodecName::H265 => "H265",
            CodecName::Vp8 => "VP8",
            CodecName::Vp9 => "VP9",
            CodecName::Av1 => "AV1",
        }
    }

    pub fn parse_audio(s: &str) -> Result<Self, String> {
        match s {
            "OPUS" => Ok(Self::Opus),
            "AAC" => Ok(Self::Aac),
            _ => Err(format!("unknown audio codec name: {s}")),
        }
    }

    pub fn parse_video(s: &str) -> Result<Self, String> {
        let codec = s.parse()?;
        if matches!(
            codec,
            Self::H264 | Self::H265 | Self::Vp8 | Self::Vp9 | Self::Av1
        ) {
            Ok(codec)
        } else {
            Err(format!("{s} is not a video codec"))
        }
    }
}

impl FromStr for CodecName {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "OPUS" => Ok(Self::Opus),
            "AAC" => Ok(Self::Aac),
            "H264" => Ok(Self::H264),
            "H265" => Ok(Self::H265),
            "VP8" => Ok(Self::Vp8),
            "VP9" => Ok(Self::Vp9),
            "AV1" => Ok(Self::Av1),
            _ => Err(format!("unknown codec name: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EngineName {
    AudioToolbox,
    Dav1d,
    FdkAac,
    Libvpx,
    Nvcodec,
    Openh264,
    Opus,
    SvtAv1,
    VideoToolbox,
}

/// 映像コーデックに対するエンジンの利用方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoCodecDirection {
    Encode,
    Decode,
}

impl EngineName {
    /// 指定コーデックを encode / decode できるか
    pub fn supports_video(self, codec: CodecName, direction: VideoCodecDirection) -> bool {
        match direction {
            VideoCodecDirection::Decode => match self {
                EngineName::Libvpx => matches!(codec, CodecName::Vp8 | CodecName::Vp9),
                #[cfg(feature = "nvcodec")]
                EngineName::Nvcodec => {
                    matches!(
                        codec,
                        CodecName::H264
                            | CodecName::H265
                            | CodecName::Vp8
                            | CodecName::Vp9
                            | CodecName::Av1
                    )
                }
                EngineName::Openh264 => matches!(codec, CodecName::H264),
                EngineName::Dav1d => matches!(codec, CodecName::Av1),
                #[cfg(target_os = "macos")]
                EngineName::VideoToolbox => matches!(codec, CodecName::H264 | CodecName::H265),
                _ => false,
            },
            VideoCodecDirection::Encode => match self {
                EngineName::Libvpx => matches!(codec, CodecName::Vp8 | CodecName::Vp9),
                #[cfg(feature = "nvcodec")]
                EngineName::Nvcodec => {
                    matches!(codec, CodecName::H264 | CodecName::H265 | CodecName::Av1)
                }
                EngineName::Openh264 => matches!(codec, CodecName::H264),
                EngineName::SvtAv1 => matches!(codec, CodecName::Av1),
                #[cfg(target_os = "macos")]
                EngineName::VideoToolbox => matches!(codec, CodecName::H264 | CodecName::H265),
                _ => false,
            },
        }
    }

    /// コーデックと方向ごとの候補エンジン列（先頭の方が優先）
    ///
    /// OpenH264 / CUDA / VideoToolbox の実行時・ビルド時可用性もここで判定する。
    pub fn video_candidate_engines(
        codec: CodecName,
        direction: VideoCodecDirection,
        is_openh264_available: bool,
    ) -> Vec<Self> {
        let mut engines = Vec::new();
        match (direction, codec) {
            (VideoCodecDirection::Decode, CodecName::Vp8 | CodecName::Vp9) => {
                #[cfg(feature = "nvcodec")]
                if shiguredo_nvcodec::is_cuda_library_available() {
                    engines.push(Self::Nvcodec);
                }
                engines.push(Self::Libvpx);
            }
            (VideoCodecDirection::Encode, CodecName::Vp8 | CodecName::Vp9) => {
                engines.push(Self::Libvpx);
            }
            (VideoCodecDirection::Decode, CodecName::H264)
            | (VideoCodecDirection::Encode, CodecName::H264) => {
                if is_openh264_available {
                    engines.push(Self::Openh264);
                }
                #[cfg(feature = "nvcodec")]
                if shiguredo_nvcodec::is_cuda_library_available() {
                    engines.push(Self::Nvcodec);
                }
                #[cfg(target_os = "macos")]
                {
                    engines.push(Self::VideoToolbox);
                }
            }
            (VideoCodecDirection::Decode, CodecName::H265)
            | (VideoCodecDirection::Encode, CodecName::H265) => {
                #[cfg(feature = "nvcodec")]
                if shiguredo_nvcodec::is_cuda_library_available() {
                    engines.push(Self::Nvcodec);
                }
                #[cfg(target_os = "macos")]
                {
                    engines.push(Self::VideoToolbox);
                }
            }
            (VideoCodecDirection::Decode, CodecName::Av1) => {
                #[cfg(feature = "nvcodec")]
                if shiguredo_nvcodec::is_cuda_library_available() {
                    engines.push(Self::Nvcodec);
                }
                engines.push(Self::Dav1d);
            }
            (VideoCodecDirection::Encode, CodecName::Av1) => {
                #[cfg(feature = "nvcodec")]
                if shiguredo_nvcodec::is_cuda_library_available() {
                    engines.push(Self::Nvcodec);
                }
                engines.push(Self::SvtAv1);
            }
            _ => {}
        }
        engines
    }

    pub fn is_available_video_decode_codec(self, codec: CodecName) -> bool {
        self.supports_video(codec, VideoCodecDirection::Decode)
    }

    pub fn is_available_video_encode_codec(self, codec: CodecName) -> bool {
        self.supports_video(codec, VideoCodecDirection::Encode)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EngineName::AudioToolbox => "audio_toolbox",
            EngineName::Dav1d => "dav1d",
            EngineName::FdkAac => "fdk_aac",
            EngineName::Libvpx => "libvpx",
            EngineName::Nvcodec => "nvcodec",
            EngineName::Openh264 => "openh264",
            EngineName::Opus => "opus",
            EngineName::SvtAv1 => "svt_av1",
            EngineName::VideoToolbox => "video_toolbox",
        }
    }

    pub fn parse_video_encoder(
        value: nojson::RawJsonValue<'_, '_>,
    ) -> Result<Self, nojson::JsonParseError> {
        let s = value.to_unquoted_string_str()?;
        match s.as_ref() {
            "libvpx" => Ok(Self::Libvpx),
            "nvcodec" => {
                #[cfg(feature = "nvcodec")]
                {
                    Ok(Self::Nvcodec)
                }
                #[cfg(not(feature = "nvcodec"))]
                {
                    Err(value.invalid("nvcodec feature is not enabled"))
                }
            }
            "openh264" => Ok(Self::Openh264),
            "svt_av1" => Ok(Self::SvtAv1),
            "video_toolbox" => {
                #[cfg(target_os = "macos")]
                {
                    Ok(Self::VideoToolbox)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Err(value.invalid("video_toolbox is only available on macOS"))
                }
            }
            "audio_toolbox" | "dav1d" | "fdk_aac" | "opus" => {
                Err(value.invalid(format!("{s} is not a video encoder")))
            }
            _ => Err(value.invalid(format!("unknown video encoder: {s}"))),
        }
    }

    pub fn parse_video_decoder(
        value: nojson::RawJsonValue<'_, '_>,
    ) -> Result<Self, nojson::JsonParseError> {
        let s = value.to_unquoted_string_str()?;
        match s.as_ref() {
            "libvpx" => Ok(Self::Libvpx),
            "nvcodec" => {
                #[cfg(feature = "nvcodec")]
                {
                    Ok(Self::Nvcodec)
                }
                #[cfg(not(feature = "nvcodec"))]
                {
                    Err(value.invalid("nvcodec feature is not enabled"))
                }
            }
            "openh264" => Ok(Self::Openh264),
            "dav1d" => Ok(Self::Dav1d),
            "video_toolbox" => {
                #[cfg(target_os = "macos")]
                {
                    Ok(Self::VideoToolbox)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Err(value.invalid("video_toolbox is only available on macOS"))
                }
            }
            "audio_toolbox" | "fdk_aac" | "opus" | "svt_av1" => {
                Err(value.invalid(format!("{s} is not a video decoder")))
            }
            _ => Err(value.invalid(format!("unknown video decoder: {s}"))),
        }
    }
}

impl nojson::DisplayJson for EngineName {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.value(self.as_str())
    }
}

/// 画像内でのピクセル位置を表現するための構造体
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PixelPosition {
    pub x: EvenUsize,
    pub y: EvenUsize,
}

/// 奇数が表現できない usize のための構造体
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EvenUsize(usize);

impl EvenUsize {
    pub const MIN_CELL_SIZE: Self = Self(16);

    pub const fn new(n: usize) -> Option<Self> {
        if n.is_multiple_of(2) {
            Some(Self(n))
        } else {
            None
        }
    }

    pub const fn truncating_new(n: usize) -> Self {
        if n.is_multiple_of(2) {
            Self(n)
        } else {
            Self(n - 1)
        }
    }

    pub const fn ceiling_new(n: usize) -> Self {
        if n.is_multiple_of(2) {
            Self(n)
        } else {
            Self(n + 1)
        }
    }

    pub const fn get(self) -> usize {
        self.0
    }
}

impl nojson::DisplayJson for EvenUsize {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.value(self.0)
    }
}

impl<'text, 'raw> TryFrom<nojson::RawJsonValue<'text, 'raw>> for EvenUsize {
    type Error = nojson::JsonParseError;

    fn try_from(value: nojson::RawJsonValue<'text, 'raw>) -> Result<Self, Self::Error> {
        let n = value.try_into()?;
        Self::new(n).ok_or_else(|| value.invalid(format!("expected even number, got {n}")))
    }
}

impl std::ops::Add for EvenUsize {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::AddAssign for EvenUsize {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl std::ops::Sub for EvenUsize {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl std::ops::Mul for EvenUsize {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

impl std::ops::Mul<usize> for EvenUsize {
    type Output = Self;

    fn mul(self, rhs: usize) -> Self::Output {
        Self(self.0 * rhs)
    }
}

// タイムオフセット
//
// フォーマット:
// - 数値 (単位: 秒)
// - "時:分:秒[.小数秒]" 形式の文字列
#[derive(Debug, Default, Clone, Copy)]
pub struct TimeOffset(Duration);

impl TimeOffset {
    pub fn get(self) -> Duration {
        self.0
    }
}

impl<'text, 'raw> TryFrom<nojson::RawJsonValue<'text, 'raw>> for TimeOffset {
    type Error = nojson::JsonParseError;

    fn try_from(value: nojson::RawJsonValue<'text, 'raw>) -> Result<Self, Self::Error> {
        if let Ok(n) = value.as_number_str() {
            let secs = n
                .parse()
                .map_err(|_| value.invalid("not a non negative finite number"))?;
            Ok(Self(duration_from_json_seconds(value, secs)?))
        } else if let Ok(s) = value.to_unquoted_string_str() {
            let parts: Vec<&str> = s.split(':').collect();
            if parts.len() != 3 {
                return Err(value.invalid("time string must be in format HH:MM:SS[.fraction]"));
            }

            let hours: u64 = parts[0]
                .parse()
                .map_err(|_| value.invalid("invalid hour value"))?;
            let minutes: u64 = parts[1]
                .parse()
                .map_err(|_| value.invalid("invalid minute value"))?;
            let seconds: f64 = parts[2]
                .parse()
                .map_err(|_| value.invalid("invalid second value"))?;

            if minutes >= 60 {
                return Err(value.invalid("minutes must be less than 60"));
            }
            if seconds >= 60.0 {
                return Err(value.invalid("seconds must be less than 60"));
            }

            // 時・分の秒換算は u64 溢れをエラーにする (debug panic / release wrap を防ぐ)
            let hour_secs = hours
                .checked_mul(3600)
                .ok_or_else(|| value.invalid("time overflows duration"))?;
            let minute_secs = minutes
                .checked_mul(60)
                .ok_or_else(|| value.invalid("time overflows duration"))?;
            let total_secs = hour_secs
                .checked_add(minute_secs)
                .ok_or_else(|| value.invalid("time overflows duration"))?;

            // 秒成分と合算するときも Duration 溢れをエラーにする
            let total_duration = Duration::from_secs(total_secs)
                .checked_add(duration_from_json_seconds(value, seconds)?)
                .ok_or_else(|| value.invalid("time overflows duration"))?;
            Ok(Self(total_duration))
        } else {
            Err(value.invalid("expected number or time string in format HH:MM:SS[.fraction]"))
        }
    }
}

// JSON の秒数を Duration に変換する
//
// 負数・非有限は既存メッセージ、有限かつ非負でも Duration に入らない値は溢れとして扱う。
// is_finite かつ非負を確認してから from_secs_f64 に渡すと、1e300 のような有限の巨大数で panic する。
fn duration_from_json_seconds(
    value: nojson::RawJsonValue<'_, '_>,
    secs: f64,
) -> Result<Duration, nojson::JsonParseError> {
    if !secs.is_finite() || secs < 0.0 {
        return Err(value.invalid("not a non negative finite number"));
    }
    Duration::try_from_secs_f64(secs).map_err(|_| value.invalid("number overflows duration"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_time_offset(json: &str) -> crate::Result<TimeOffset> {
        crate::json::parse_str(json)
    }

    #[test]
    fn rejects_negative_number_without_panic() {
        // 数値 -1 は f64 としてはパースできるが Duration にはできない
        let err = parse_time_offset("-1").expect_err("負数はパースエラーになる");
        assert!(
            err.reason.contains("not a non negative finite number"),
            "想定と異なるエラー: {}",
            err.reason
        );
    }

    #[test]
    fn rejects_overflowing_finite_number_without_panic() {
        // 1e300 は有限かつ非負だが Duration 上限を超える
        let err = parse_time_offset("1e300").expect_err("巨大数はパースエラーになる");
        assert!(
            err.reason.contains("number overflows duration"),
            "想定と異なるエラー: {}",
            err.reason
        );
        assert!(
            !err.reason.contains("not a non negative finite number"),
            "溢れを非負有限の失敗として扱ってはいけない: {}",
            err.reason
        );
    }

    #[test]
    fn rejects_hour_u64_overflow_without_panic() {
        // hours * 3600 が u64 を溢れる
        let err = parse_time_offset(r#""5124095576030432:00:00""#)
            .expect_err("時の乗算溢れはパースエラーになる");
        assert!(
            err.reason.contains("time overflows duration"),
            "想定と異なるエラー: {}",
            err.reason
        );
    }

    #[test]
    fn rejects_duration_add_overflow_without_panic() {
        // 整数演算は収まるが Duration 加算が溢れる
        let err = parse_time_offset(r#""5124095576030431:00:16""#)
            .expect_err("Duration 加算溢れはパースエラーになる");
        assert!(
            err.reason.contains("time overflows duration"),
            "想定と異なるエラー: {}",
            err.reason
        );
    }

    #[test]
    fn parses_normal_number() {
        let offset = parse_time_offset("1.5").expect("通常の秒数のパースに失敗した");
        assert_eq!(
            offset.get(),
            Duration::try_from_secs_f64(1.5).expect("1.5 秒は Duration に入る")
        );
    }

    #[test]
    fn parses_normal_time_string() {
        // 1 時間 2 分 3.5 秒
        let offset = parse_time_offset(r#""01:02:03.5""#).expect("通常の時分秒のパースに失敗した");
        let expected = Duration::from_secs(3723)
            + Duration::try_from_secs_f64(0.5).expect("0.5 秒は Duration に入る");
        assert_eq!(offset.get(), expected);
    }
}
