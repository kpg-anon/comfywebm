//! Hand-editable TOML configuration in the app config dir:
//!
//! - `config.toml`: the lists the UI offers (resolution steps, percentages,
//!   frame rate caps, audio bitrates). Written once with comments; the app
//!   never rewrites it.
//! - `presets/platform/*.toml` and `presets/saved/*.toml`: one preset per
//!   file. The platform files are written on first start (and by "Restore
//!   platform presets"); "Save as new" writes to saved/.
//!
//! Every read goes to disk, so an edited file applies on "Reload config".

use crate::{Codec, Container, Effort, Preset, Rate};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CONFIG_TOML: &str = r#"# comfywebm configuration you can edit by hand.
# Save this file, then press "Reload config" (the pencil button next to the
# preset list), or restart comfywebm.
# Presets are in the "presets" folder next to this file, one file per preset:
# presets/platform holds the platform presets, presets/saved your own.

[resolution]
# Steps offered under Video > Resolution > Steps, in pixels.
heights = [240, 360, 480, 540, 576, 720, 900, 1080, 1440, 2160]
widths = [426, 640, 854, 960, 1024, 1280, 1440, 1600, 1920, 2048, 2560, 3840]
# Steps are by height unless this is "width".
step_by = "height"
# Percentages offered under Video > Resolution > %.
percents = [25, 33, 50, 67, 75, 100]

[motion]
# Frame rate caps offered under Motion > Frame rate.
fps_caps = [10, 12, 15, 20, 24, 25, 30, 48, 50, 60, 120]

[audio]
# Audio bitrates offered, in kbps.
bitrates = [32, 48, 64, 80, 96, 112, 128, 160, 192, 256]
"#;

#[derive(Deserialize, Default)]
#[serde(default)]
struct ConfigFile {
    resolution: ResolutionSection,
    motion: MotionSection,
    audio: AudioSection,
}

#[derive(Deserialize)]
#[serde(default)]
struct ResolutionSection {
    heights: Vec<u32>,
    widths: Vec<u32>,
    step_by: String,
    percents: Vec<u32>,
}

impl Default for ResolutionSection {
    fn default() -> Self {
        ResolutionSection {
            heights: vec![240, 360, 480, 540, 576, 720, 900, 1080, 1440, 2160],
            widths: vec![426, 640, 854, 960, 1024, 1280, 1440, 1600, 1920, 2048, 2560, 3840],
            step_by: "height".into(),
            percents: vec![25, 33, 50, 67, 75, 100],
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct MotionSection {
    fps_caps: Vec<f64>,
}

impl Default for MotionSection {
    fn default() -> Self {
        MotionSection { fps_caps: vec![10.0, 12.0, 15.0, 20.0, 24.0, 25.0, 30.0, 48.0, 50.0, 60.0, 120.0] }
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct AudioSection {
    bitrates: Vec<u32>,
}

impl Default for AudioSection {
    fn default() -> Self {
        AudioSection { bitrates: vec![32, 48, 64, 80, 96, 112, 128, 160, 192, 256] }
    }
}

/// One preset file, flat and snake_case for people editing it.
#[derive(Deserialize, Default)]
#[serde(default)]
struct PresetFile {
    name: String,
    /// sort key among the platform presets
    order: Option<i64>,
    container: Option<Container>,
    codec: Option<Codec>,
    max_bytes: Option<f64>,
    crf: Option<f64>,
    max_bitrate_k: Option<f64>,
    auto_scale: Option<bool>,
    max_width: Option<u32>,
    max_height: Option<u32>,
    fps: Option<f64>,
    audio_bitrate_k: Option<f64>,
    effort: Option<Effort>,
    two_pass: Option<bool>,
    max_duration: Option<f64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PresetEntry {
    pub preset: Preset,
    /// "platform" or "saved"
    pub group: String,
    /// path relative to the presets folder, with forward slashes ("saved/x.toml")
    pub file: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigInfo {
    /// the app config folder (holds config.toml and presets/)
    pub dir: String,
    pub heights: Vec<u32>,
    pub widths: Vec<u32>,
    pub step_by: String,
    pub percents: Vec<u32>,
    pub fps_caps: Vec<f64>,
    pub audio_bitrates: Vec<u32>,
    pub presets: Vec<PresetEntry>,
    /// files that could not be read, with the reason
    pub errors: Vec<String>,
}

fn fourchan(name: &str, order: i64, max_bytes: f64, audio: bool, max_duration: f64) -> (i64, Preset) {
    (
        order,
        Preset {
            name: name.into(),
            container: Container::Webm,
            codec: Codec::Vp9,
            rate: Rate::Size { max_bytes },
            auto_scale: true,
            max_width: Some(2048),
            max_height: Some(2048),
            fps: None,
            audio_bitrate_k: audio.then_some(96.0),
            effort: Effort::Balanced,
            two_pass: true,
            max_duration: Some(max_duration),
        },
    )
}

/// The platform presets written on first start (PLAN.md "Built-in presets").
fn default_platform() -> Vec<(&'static str, i64, Preset)> {
    let discord = |name: &str, max_bytes: f64| Preset {
        name: name.into(),
        container: Container::Mp4,
        codec: Codec::H264,
        rate: Rate::Size { max_bytes },
        auto_scale: true,
        max_width: None,
        max_height: None,
        fps: None,
        audio_bitrate_k: Some(128.0),
        effort: Effort::Balanced,
        two_pass: true,
        max_duration: None,
    };
    let quality = |name: &str, container, codec, crf: f64, audio: f64| Preset {
        name: name.into(),
        container,
        codec,
        rate: Rate::Crf { crf, max_bitrate_k: None },
        auto_scale: false,
        max_width: None,
        max_height: None,
        fps: None,
        audio_bitrate_k: Some(audio),
        effort: Effort::Balanced,
        two_pass: false,
        max_duration: None,
    };
    let mut v: Vec<(&'static str, i64, Preset)> = Vec::new();
    for (file, (order, p)) in [
        ("4chan-4mib.toml", fourchan("4chan (4 MiB, no sound)", 10, 4194304.0, false, 120.0)),
        ("4chan-2mib-b-bant.toml", fourchan("4chan /b/ /bant/ (2 MiB, no sound)", 20, 2097152.0, false, 120.0)),
        ("4chan-gif.toml", fourchan("4chan /gif/ (4 MiB, sound)", 30, 4194304.0, true, 300.0)),
        ("4chan-r-wsr.toml", fourchan("4chan /r/ /wsr/ (4 MiB, sound)", 40, 4194304.0, true, 120.0)),
        ("4chan-wsg.toml", fourchan("4chan /wsg/ (6 MiB, sound)", 50, 6291456.0, true, 400.0)),
    ] {
        v.push((file, order, p));
    }
    v.push(("discord-20mb.toml", 60, discord("Discord (20 MB)", 20_000_000.0)));
    v.push(("discord-nitro-basic-50mb.toml", 70, discord("Discord Nitro Basic (50 MB)", 50_000_000.0)));
    v.push((
        "x-twitter.toml",
        80,
        Preset {
            name: "X / Twitter".into(),
            container: Container::Mp4,
            codec: Codec::H264,
            rate: Rate::Crf { crf: 20.0, max_bitrate_k: None },
            auto_scale: false,
            max_width: Some(1280),
            max_height: Some(1280),
            fps: Some(60.0),
            audio_bitrate_k: Some(128.0),
            effort: Effort::Balanced,
            two_pass: false,
            max_duration: Some(1200.0),
        },
    ));
    v.push(("quality-webm.toml", 90, quality("Quality WebM", Container::Webm, Codec::Vp9, 30.0, 128.0)));
    v.push(("quality-mp4.toml", 100, quality("Quality MP4", Container::Mp4, Codec::H264, 20.0, 160.0)));
    v
}

fn lower<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

fn num(v: f64) -> String {
    crate::plan::fmt_num(v)
}

fn toml_str(s: &str) -> String {
    // a TOML basic string: escape backslash and quote, drop control characters
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            c if c.is_control() => {}
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// A preset as a commented TOML file. Unused keys are written commented out,
/// so the file shows every option.
pub fn preset_toml(p: &Preset, order: Option<i64>) -> String {
    // an unset key is written commented out with an example value
    let opt = |key: &str, v: Option<String>, example: &str, note: &str| match v {
        Some(v) => format!("{key} = {v}{note}\n"),
        None => format!("# {key} = {example}{note}\n"),
    };
    let mut s = String::from(
        "# comfywebm preset. Edit, save, then press \"Reload config\" (the pencil button\n# next to the preset list).\n",
    );
    s.push_str("# A line starting with # is not used.\n\n");
    s.push_str(&format!("name = {}\n", toml_str(&p.name)));
    if let Some(o) = order {
        s.push_str(&format!("order = {o}                   # position in the list\n"));
    }
    s.push_str(&format!("container = \"{}\"            # webm or mp4\n", lower(&p.container)));
    s.push_str(&format!("codec = \"{}\"                  # webm: vp9 vp8 av1, mp4: h264 hevc av1\n\n", lower(&p.codec)));
    s.push_str("# Rate: a size limit in bytes (1 MiB = 1048576), or a quality level (crf).\n");
    match p.rate {
        Rate::Size { max_bytes } => {
            s.push_str(&format!("max_bytes = {}\n", num(max_bytes)));
            s.push_str("# crf = 30                     # lower = better and larger\n");
            s.push_str("# max_bitrate_k = 4000         # quality mode: bitrate cap in kbps\n");
        }
        Rate::Crf { crf, max_bitrate_k } => {
            s.push_str("# max_bytes = 4194304\n");
            s.push_str(&format!("crf = {}                     # lower = better and larger\n", num(crf)));
            s.push_str(&opt("max_bitrate_k", max_bitrate_k.map(num), "4000", "       # bitrate cap in kbps"));
        }
    }
    s.push('\n');
    s.push_str(&format!("auto_scale = {}             # size limit mode: lower the resolution to fit the bitrate\n", p.auto_scale));
    s.push_str(&opt("max_width", p.max_width.map(|v| v.to_string()), "2048", "              # largest picture the destination accepts"));
    s.push_str(&opt("max_height", p.max_height.map(|v| v.to_string()), "2048", ""));
    s.push_str(&opt("fps", p.fps.map(num), "30", "                      # frame rate cap"));
    s.push_str(&opt("audio_bitrate_k", p.audio_bitrate_k.map(num), "96", "          # remove for no audio"));
    s.push_str(&format!("effort = \"{}\"           # fast, balanced or best\n", lower(&p.effort)));
    s.push_str(&format!("two_pass = {}\n", p.two_pass));
    s.push_str(&opt("max_duration", p.max_duration.map(num), "120", "           # seconds; the app warns when the output is longer"));
    s
}

fn parse_preset(text: &str, fallback_name: &str) -> Result<(Option<i64>, Preset), String> {
    let f: PresetFile = toml::from_str(text).map_err(|e| e.message().to_string())?;
    let rate = match (f.max_bytes, f.crf) {
        (Some(b), _) if b > 0.0 => Rate::Size { max_bytes: b },
        (_, Some(c)) => Rate::Crf { crf: c, max_bitrate_k: f.max_bitrate_k.filter(|k| *k > 0.0) },
        _ => return Err("needs max_bytes (size limit) or crf (quality)".into()),
    };
    let name = if f.name.trim().is_empty() { fallback_name.to_string() } else { f.name.trim().to_string() };
    Ok((
        f.order,
        Preset {
            name,
            container: f.container.unwrap_or_default(),
            codec: f.codec.unwrap_or(Codec::Vp9),
            rate,
            auto_scale: f.auto_scale.unwrap_or(false),
            max_width: f.max_width.filter(|v| *v > 0),
            max_height: f.max_height.filter(|v| *v > 0),
            fps: f.fps.filter(|v| *v > 0.0),
            audio_bitrate_k: f.audio_bitrate_k.filter(|v| *v > 0.0),
            effort: f.effort.unwrap_or(Effort::Balanced),
            two_pass: f.two_pass.unwrap_or(false),
            max_duration: f.max_duration.filter(|v| *v > 0.0),
        },
    ))
}

/// A file name from a preset name: lower case letters, digits and dashes.
fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s = s.trim_end_matches('-').to_string();
    if s.is_empty() { "preset".into() } else { s.chars().take(60).collect() }
}

pub struct Config {
    pub dir: PathBuf,
}

impl Config {
    pub fn new(dir: PathBuf) -> Self {
        Config { dir }
    }

    fn presets_dir(&self) -> PathBuf {
        self.dir.join("presets")
    }

    /// Creates config.toml and the platform presets when they do not exist,
    /// and moves presets from settings.json (1.1.0 and earlier) into saved/.
    /// Returns true when it moved any.
    pub fn ensure(&self, legacy: &[Preset]) -> bool {
        let _ = std::fs::create_dir_all(self.presets_dir().join("saved"));
        let cfg = self.dir.join("config.toml");
        if !cfg.exists() {
            let _ = std::fs::write(&cfg, CONFIG_TOML);
        }
        let platform = self.presets_dir().join("platform");
        if !platform.exists() {
            self.write_platform();
        }
        for p in legacy {
            let _ = self.save(p, None);
        }
        !legacy.is_empty()
    }

    pub fn write_platform(&self) {
        let dir = self.presets_dir().join("platform");
        let _ = std::fs::create_dir_all(&dir);
        for (file, order, p) in default_platform() {
            let _ = std::fs::write(dir.join(file), preset_toml(&p, Some(order)));
        }
    }

    /// The presets folder path for a relative file name, or an error when the
    /// name would leave that folder.
    fn resolve(&self, file: &str) -> Result<PathBuf, String> {
        let f = file.replace('\\', "/");
        let mut parts = f.split('/');
        let (Some(group), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(format!("Bad preset file name: {file}"));
        };
        if !matches!(group, "platform" | "saved") || name.is_empty() || name.starts_with('.') || !name.ends_with(".toml") {
            return Err(format!("Bad preset file name: {file}"));
        }
        Ok(self.presets_dir().join(group).join(name))
    }

    pub fn load(&self) -> ConfigInfo {
        let mut errors = Vec::new();
        let cfg_path = self.dir.join("config.toml");
        let cfg: ConfigFile = match std::fs::read_to_string(&cfg_path) {
            Ok(t) => toml::from_str(&t).unwrap_or_else(|e| {
                errors.push(format!("config.toml: {}", e.message()));
                ConfigFile::default()
            }),
            Err(_) => ConfigFile::default(),
        };
        let mut presets: Vec<(String, Option<i64>, PresetEntry)> = Vec::new();
        for group in ["platform", "saved"] {
            let dir = self.presets_dir().join(group);
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) != Some("toml") {
                    continue;
                }
                let fname = e.file_name().to_string_lossy().into_owned();
                let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| parse_preset(&t, &stem)) {
                    Ok((order, preset)) => presets.push((
                        group.to_string(),
                        order,
                        PresetEntry { preset, group: group.into(), file: format!("{group}/{fname}") },
                    )),
                    Err(err) => errors.push(format!("presets/{group}/{fname}: {err}")),
                }
            }
        }
        // platform first, by order then name; then saved by name. A repeated name keeps the first.
        presets.sort_by(|a, b| {
            (a.0 != "platform")
                .cmp(&(b.0 != "platform"))
                .then(a.1.unwrap_or(i64::MAX).cmp(&b.1.unwrap_or(i64::MAX)))
                .then(a.2.preset.name.to_lowercase().cmp(&b.2.preset.name.to_lowercase()))
        });
        let mut seen = std::collections::HashSet::new();
        let mut list = Vec::new();
        for (_, _, e) in presets {
            if seen.insert(e.preset.name.clone()) {
                list.push(e);
            } else {
                errors.push(format!("presets/{}: the name \"{}\" is used by another preset; skipped", e.file, e.preset.name));
            }
        }
        let clean_u = |v: Vec<u32>| {
            let mut v: Vec<u32> = v.into_iter().filter(|x| *x >= 2).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let mut fps: Vec<f64> = cfg.motion.fps_caps.into_iter().filter(|f| f.is_finite() && *f > 0.0).collect();
        fps.sort_by(f64::total_cmp);
        fps.dedup();
        ConfigInfo {
            dir: self.dir.to_string_lossy().into_owned(),
            heights: clean_u(cfg.resolution.heights),
            widths: clean_u(cfg.resolution.widths),
            step_by: if cfg.resolution.step_by.eq_ignore_ascii_case("width") { "width".into() } else { "height".into() },
            percents: {
                let mut v: Vec<u32> = cfg.resolution.percents.into_iter().filter(|p| (1..=100).contains(p)).collect();
                v.sort_unstable();
                v.dedup();
                v
            },
            fps_caps: fps,
            audio_bitrates: clean_u(cfg.audio.bitrates),
            presets: list,
            errors,
        }
    }

    /// Writes a preset. `file` = an existing preset file to replace; None = a
    /// new file in saved/. Returns the relative file name.
    pub fn save(&self, p: &Preset, file: Option<&str>) -> Result<String, String> {
        let (path, rel) = match file {
            Some(f) => (self.resolve(f)?, f.replace('\\', "/")),
            None => {
                let dir = self.presets_dir().join("saved");
                std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
                let base = slug(&p.name);
                let mut n = 1;
                let mut name = format!("{base}.toml");
                while dir.join(&name).exists() {
                    n += 1;
                    name = format!("{base}-{n}.toml");
                }
                (dir.join(&name), format!("saved/{name}"))
            }
        };
        // keep an existing order key when a platform file is overwritten
        let order = std::fs::read_to_string(&path).ok().and_then(|t| parse_preset(&t, "").ok()).and_then(|(o, _)| o);
        write_atomic(&path, &preset_toml(p, order))?;
        Ok(rel)
    }

    pub fn delete(&self, file: &str) -> Result<(), String> {
        let path = self.resolve(file)?;
        std::fs::remove_file(&path).map_err(|e| format!("Cannot delete {}: {e}", path.display()))
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_round_trip() {
        for (_, order, p) in default_platform() {
            let text = preset_toml(&p, Some(order));
            let (o, back) = parse_preset(&text, "x").unwrap_or_else(|e| panic!("{e}\n{text}"));
            assert_eq!(o, Some(order));
            assert_eq!(serde_json::to_value(&back).unwrap(), serde_json::to_value(&p).unwrap(), "{text}");
        }
    }

    #[test]
    fn default_config_parses() {
        let c: ConfigFile = toml::from_str(CONFIG_TOML).unwrap();
        assert_eq!(c.resolution.heights[0], 240);
        assert_eq!(c.motion.fps_caps.len(), 11);
    }

    #[test]
    fn names() {
        assert_eq!(slug("4chan /b/ /bant/ (2 MiB)"), "4chan-b-bant-2-mib");
        let c = Config::new(PathBuf::from("x"));
        assert!(c.resolve("saved/a.toml").is_ok());
        assert!(c.resolve("../a.toml").is_err());
        assert!(c.resolve("saved/../../a.toml").is_err());
        assert!(c.resolve("other/a.toml").is_err());
    }
}
