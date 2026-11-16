//! ffprobe JSON -> MediaInfo.

use crate::spec::{AudioTrack, MediaInfo, SubTrack, VideoInfo};
use crate::tools::command;
use serde_json::Value;
use std::path::Path;

/// Subtitle codecs the `subtitles` filter can render. Everything else
/// (hdmv_pgs_subtitle, dvd_subtitle, dvb_subtitle, xsub, ...) is a bitmap format.
const TEXT_SUB_CODECS: &[&str] = &[
    "subrip", "srt", "ass", "ssa", "webvtt", "mov_text", "text", "microdvd", "subviewer",
    "subviewer1", "jacosub", "realtext", "sami", "stl", "pjs", "mpl2", "vplayer", "ttml",
];

pub fn probe(ffprobe: &Path, path: &str) -> Result<MediaInfo, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    let mut cmd = command(ffprobe);
    cmd.args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams", "-i", path]);
    let out = crate::tools::output_within(cmd, std::time::Duration::from_secs(60), "ffprobe")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "ffprobe could not read the file.".into() } else { err });
    }
    let json: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("Bad ffprobe output: {e}"))?;
    let mut info = parse(&json)?;
    info.path = path.to_string();
    info.file_name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    info.size_bytes = meta.len();
    Ok(info)
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn ratio(s: Option<&str>) -> Option<f64> {
    let s = s?;
    let (a, b) = s.split_once('/').or_else(|| s.split_once(':'))?;
    let a: f64 = a.trim().parse().ok()?;
    let b: f64 = b.trim().parse().ok()?;
    (a > 0.0 && b > 0.0).then(|| a / b)
}

fn tag(stream: &Value, key: &str) -> Option<String> {
    let tags = stream.get("tags")?.as_object()?;
    tags.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .and_then(|(_, v)| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "und")
        .map(str::to_string)
}

fn rotation(stream: &Value) -> f64 {
    if let Some(list) = stream.get("side_data_list").and_then(Value::as_array) {
        for sd in list {
            if let Some(r) = sd.get("rotation").and_then(num) {
                return r;
            }
        }
    }
    tag(stream, "rotate").and_then(|s| s.parse().ok()).unwrap_or(0.0)
}

/// ffmpeg decoders that render text files as video.
const TEXT_ART: [&str; 4] = ["ansi", "bintext", "xbin", "idf"];

pub fn parse(json: &Value) -> Result<MediaInfo, String> {
    let streams = json.get("streams").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut info = MediaInfo::default();
    let mut video: Option<VideoInfo> = None;
    let mut video_duration = None;
    // the first embedded cover picture (attached_pic): (codec, width, height, stream index)
    let mut cover: Option<(String, u32, u32, u32)> = None;
    for s in &streams {
        let kind = s.get("codec_type").and_then(Value::as_str).unwrap_or("");
        let codec = s.get("codec_name").and_then(Value::as_str).unwrap_or("unknown").to_string();
        // text files that ffmpeg draws as pictures (.nfo, .ans, .diz, ...) are not media: the
        // app adds a probed file to what the webview may read, so these must not pass
        if kind == "video" && TEXT_ART.contains(&codec.as_str()) {
            continue;
        }
        match kind {
            "video" if video.is_none() => {
                let attached = s.pointer("/disposition/attached_pic").and_then(num).unwrap_or(0.0) > 0.0;
                if attached {
                    if cover.is_none() {
                        let dim = |k: &str| s.get(k).and_then(num).unwrap_or(0.0) as u32;
                        let index = s.get("index").and_then(num).unwrap_or(0.0) as u32;
                        cover = Some((codec, dim("width"), dim("height"), index));
                    }
                    continue;
                }
                let w = s.get("width").and_then(num).unwrap_or(0.0);
                let h = s.get("height").and_then(num).unwrap_or(0.0);
                let sar = ratio(s.get("sample_aspect_ratio").and_then(Value::as_str)).unwrap_or(1.0);
                let anamorphic = (sar - 1.0).abs() > 1e-6;
                // Same rounding as the filter graph: trunc(iw*sar/2)*2.
                let mut dw = if anamorphic { ((w * sar) / 2.0).floor() * 2.0 } else { w };
                let mut dh = h;
                let rot = rotation(s).round() as i64;
                if rot.rem_euclid(180) == 90 {
                    std::mem::swap(&mut dw, &mut dh);
                }
                let fps = ratio(s.get("avg_frame_rate").and_then(Value::as_str))
                    .or_else(|| ratio(s.get("r_frame_rate").and_then(Value::as_str)))
                    .unwrap_or(0.0);
                let field_order = s.get("field_order").and_then(Value::as_str).unwrap_or("");
                let interlaced = !matches!(field_order, "" | "progressive" | "unknown");
                video_duration = s.get("duration").and_then(num);
                let color = |k: &str| {
                    s.get(k)
                        .and_then(Value::as_str)
                        .filter(|v| !v.is_empty() && *v != "unknown" && *v != "reserved")
                        .map(str::to_string)
                };
                let color_trc = color("color_transfer");
                let hdr = matches!(color_trc.as_deref(), Some("smpte2084" | "arib-std-b67"));
                let tags: Vec<String> = [color("color_space"), color("color_primaries"), color_trc.clone()]
                    .into_iter()
                    .map(|c| c.unwrap_or_else(|| "?".into()))
                    .collect();
                let r_rate = ratio(s.get("r_frame_rate").and_then(Value::as_str));
                let vfr = matches!(r_rate, Some(r) if fps > 0.0 && (r - fps).abs() / fps > 0.01);
                video = Some(VideoInfo {
                    pix_fmt: s.get("pix_fmt").and_then(Value::as_str).map(str::to_string),
                    profile: s.get("profile").and_then(Value::as_str).map(str::to_string),
                    color: (tags.iter().any(|t| t != "?")).then(|| tags.join(" / ")),
                    frames: s.get("nb_frames").and_then(num).map(|n| n as u64),
                    vfr,
                    hdr,
                    color_space: color("color_space"),
                    color_primaries: color("color_primaries"),
                    color_trc,
                    codec,
                    width: dw as u32,
                    height: dh as u32,
                    fps,
                    bitrate_k: s.get("bit_rate").and_then(num).map(|b| (b / 1000.0).round() as u32),
                    interlaced,
                    stream_index: s.get("index").and_then(num).unwrap_or(0.0) as u32,
                    anamorphic,
                });
            }
            "audio" => info.audio.push(AudioTrack {
                track: info.audio.len() as u32,
                codec,
                channels: s.get("channels").and_then(num).unwrap_or(0.0) as u32,
                lang: tag(s, "language"),
                title: tag(s, "title"),
                sample_rate: s.get("sample_rate").and_then(num).map(|n| n as u32),
                bitrate_k: s.get("bit_rate").and_then(num).map(|b| (b / 1000.0).round() as u32),
            }),
            "subtitle" => {
                let text_based = TEXT_SUB_CODECS.contains(&codec.as_str());
                info.subs.push(SubTrack {
                    track: info.subs.len() as u32,
                    codec,
                    lang: tag(s, "language"),
                    title: tag(s, "title"),
                    text_based,
                })
            }
            _ => {}
        }
    }
    match video {
        Some(v) => info.video = v,
        None if info.audio.is_empty() => return Err("The file has no video or audio stream.".into()),
        None => {
            // a sound file: it is made into a video with a still picture
            info.audio_only = true;
            if let Some((codec, width, height, stream_index)) = cover.filter(|c| c.1 > 0 && c.2 > 0) {
                info.video = VideoInfo { codec, width, height, stream_index, ..VideoInfo::default() };
            }
        }
    }
    let fmt = json.get("format");
    info.format = fmt
        .and_then(|f| f.get("format_long_name").or_else(|| f.get("format_name")))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    info.bitrate_k = fmt.and_then(|f| f.get("bit_rate")).and_then(num).map(|b| (b / 1000.0).round() as u32);
    info.created = fmt.and_then(|f| tag(f, "creation_time"));
    info.encoder = fmt.and_then(|f| tag(f, "encoder"));
    info.duration = json
        .pointer("/format/duration")
        .and_then(num)
        .or(video_duration)
        .filter(|d| d.is_finite() && *d > 0.0)
        .unwrap_or(0.0);
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_file_with_cover() {
        let json: Value = serde_json::from_str(
            r#"{"streams":[
                {"index":0,"codec_type":"audio","codec_name":"mp3","channels":2},
                {"index":1,"codec_type":"video","codec_name":"mjpeg","width":600,"height":500,"disposition":{"attached_pic":1}}
            ],"format":{"duration":"8.0"}}"#,
        )
        .unwrap();
        let info = parse(&json).unwrap();
        assert!(info.audio_only);
        assert_eq!((info.video.width, info.video.height, info.video.stream_index), (600, 500, 1));
        assert_eq!(info.duration, 8.0);
        let none: Value = serde_json::from_str(r#"{"streams":[],"format":{"duration":"8.0"}}"#).unwrap();
        assert!(parse(&none).is_err());
    }

    #[test]
    fn text_art_is_not_video() {
        let j: Value = serde_json::from_str(
            r#"{"streams":[{"index":0,"codec_type":"video","codec_name":"ansi","width":640,"height":400}],
                "format":{"format_name":"tty","duration":"1.0"}}"#,
        )
        .unwrap();
        assert!(parse(&j).is_err());
    }
}
