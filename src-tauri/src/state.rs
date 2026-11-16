//! App state: settings file, located tools, probe cache.

use comfywebm_engine::config::{read_json, Config};
use crate::queue::Queue;
use comfywebm_engine::tools::{self, Tools};
use comfywebm_engine::{probe, MediaInfo, Settings, ToolsStatus};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Size and modification time; a file replaced in place gets a new stamp.
type FileStamp = Option<(u64, Option<std::time::SystemTime>)>;

fn stamp(path: &str) -> FileStamp {
    std::fs::metadata(path).ok().map(|m| (m.len(), m.modified().ok()))
}

/// Most probe results kept; older ones are dropped when it is reached.
const PROBE_CACHE: usize = 256;

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub cache_dir: PathBuf,
    /// filled on first use and cleared when the ffmpeg path setting changes
    tools: Mutex<Option<(Tools, ToolsStatus)>>,
    /// probe results by path with the file's size and mtime when probed, so
    /// `plan` and the queue do not run ffprobe again; a changed file is probed again
    probes: Mutex<HashMap<String, (FileStamp, MediaInfo)>>,
    pub queue: Arc<Queue>,
    /// config.toml and the preset files
    pub config: Config,
    /// the user's Desktop, the default output folder
    pub desktop_dir: Option<PathBuf>,
    /// problems reading settings.json or queue.json at start, shown once through `config_get`
    pub load_errors: Mutex<Vec<String>>,
}

impl AppState {
    pub fn new(config_dir: PathBuf, cache_dir: PathBuf, desktop_dir: Option<PathBuf>) -> Self {
        let settings_path = config_dir.join("settings.json");
        let mut errors = Vec::new();
        let settings = read_json::<Settings>(&settings_path, |_| Ok(()), &mut errors).unwrap_or_default();
        let queue_path = config_dir.join("queue.json");
        let queue = Queue::load(queue_path, &mut errors);
        let config = Config::new(config_dir);
        let moved = config.ensure(&settings.presets);
        let state = AppState {
            settings: Mutex::new(settings.clone()),
            settings_path,
            cache_dir,
            tools: Mutex::new(None),
            probes: Mutex::new(HashMap::new()),
            queue: Arc::new(queue),
            config,
            desktop_dir,
            load_errors: Mutex::new(errors),
        };
        if moved {
            // the presets now live in presets/saved/*.toml
            let _ = state.save_settings(Settings { presets: Vec::new(), ..settings });
        }
        state
    }

    /// The output folder for new jobs; None = beside the source file.
    pub fn output_dir(&self) -> Option<String> {
        let s = self.settings();
        let custom = s.output_dir.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
        let mode = s.output_mode.unwrap_or_else(|| if custom.is_some() { "folder".into() } else { "desktop".into() });
        let desktop = || self.desktop_dir.as_ref().map(|d| d.to_string_lossy().into_owned());
        match mode.as_str() {
            "beside" => None,
            "folder" => custom.or_else(desktop),
            _ => desktop(),
        }
    }

    /// Arguments added to every yt-dlp call: cookies from a browser, then Settings.ytdlpArgs.
    pub fn ytdlp_extra(&self) -> Vec<String> {
        let s = self.settings();
        let mut a = Vec::new();
        if let Some(b) = s.ytdlp_cookies.map(|b| b.trim().to_string()).filter(|b| !b.is_empty()) {
            a.push("--cookies-from-browser".to_string());
            a.push(b);
        }
        a.extend(s.ytdlp_args);
        a
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn save_settings(&self, new: Settings) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&new).map_err(|e| e.to_string())?;
        if let Some(dir) = self.settings_path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        }
        let changed = {
            // held through the write, so two saves cannot interleave on the temp file
            let mut cur = self.settings.lock().unwrap();
            // write a temp file and rename it, so a crash never leaves half a file
            let tmp = self.settings_path.with_extension("json.tmp");
            std::fs::write(&tmp, json).map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
            std::fs::rename(&tmp, &self.settings_path)
                .map_err(|e| format!("Cannot write {}: {e}", self.settings_path.display()))?;
            let changed = cur.ffmpeg_path != new.ffmpeg_path || cur.ytdlp_path != new.ytdlp_path;
            *cur = new;
            changed
        };
        // `tools()` locks tools then settings, so settings is released first here
        if changed {
            *self.tools.lock().unwrap() = None;
        }
        Ok(())
    }

    /// The next `tools()` searches again.
    pub fn forget_tools(&self) {
        *self.tools.lock().unwrap() = None;
    }

    pub fn tools(&self) -> (Tools, ToolsStatus) {
        let mut slot = self.tools.lock().unwrap();
        if slot.is_none() {
            let (ff, yt) = {
                let s = self.settings.lock().unwrap();
                (s.ffmpeg_path.clone(), s.ytdlp_path.clone())
            };
            let t = tools::find(ff.as_deref(), yt.as_deref());
            let st = tools::status(&t);
            *slot = Some((t, st));
        }
        slot.clone().unwrap()
    }

    pub fn ffmpeg(&self) -> Result<PathBuf, String> {
        self.tools().0.ffmpeg.ok_or_else(|| "ffmpeg was not found. Set its location in Settings.".to_string())
    }

    pub fn ytdlp(&self) -> Result<PathBuf, String> {
        self.tools().0.ytdlp.ok_or_else(|| "yt-dlp was not found. Set its location in Settings.".to_string())
    }

    /// ffmpeg has the zscale filter (tone mapping)
    pub fn zscale(&self) -> bool {
        self.tools().1.filters.zscale
    }

    /// `Settings.downloadDir`, or "downloads" in the app cache dir.
    pub fn download_dir(&self) -> PathBuf {
        self.settings()
            .download_dir
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.cache_dir.join("downloads"))
    }

    /// Probes `path` with ffprobe and caches the result.
    pub fn probe(&self, path: &str) -> Result<MediaInfo, String> {
        let ffprobe = self.tools().0.ffprobe.ok_or("ffprobe was not found. Set the ffmpeg location in Settings.")?;
        let st = stamp(path);
        let info = probe::probe(&ffprobe, path)?;
        let mut cache = self.probes.lock().unwrap();
        if cache.len() >= PROBE_CACHE {
            cache.clear();
        }
        cache.insert(path.to_string(), (st, info.clone()));
        Ok(info)
    }

    /// Probe results of the other files a spec joins (`segment_sources`).
    pub fn sources(&self, spec: &comfywebm_engine::EncodeSpec) -> Result<HashMap<String, MediaInfo>, String> {
        let mut m = HashMap::new();
        for p in spec.segment_sources.iter().flatten() {
            if p != &spec.input && !m.contains_key(p) {
                m.insert(p.clone(), self.media(p)?);
            }
        }
        Ok(m)
    }

    /// Cached probe result, probing when there is none or the file changed.
    pub fn media(&self, path: &str) -> Result<MediaInfo, String> {
        if let Some((st, i)) = self.probes.lock().unwrap().get(path) {
            // URLs and other non-files have no stamp; they stay cached
            if st.is_none() || *st == stamp(path) {
                return Ok(i.clone());
            }
        }
        self.probe(path)
    }
}

