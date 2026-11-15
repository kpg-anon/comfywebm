//! App state: settings file, located tools, probe cache.

use crate::queue::Queue;
use comfywebm_engine::tools::{self, Tools};
use comfywebm_engine::{probe, MediaInfo, Settings, ToolsStatus};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub cache_dir: PathBuf,
    /// filled on first use and cleared when the ffmpeg path setting changes
    tools: Mutex<Option<(Tools, ToolsStatus)>>,
    /// probe results by path, so `plan` and the queue do not run ffprobe again
    probes: Mutex<HashMap<String, MediaInfo>>,
    pub queue: Arc<Queue>,
}

impl AppState {
    pub fn new(config_dir: PathBuf, cache_dir: PathBuf) -> Self {
        let settings_path = config_dir.join("settings.json");
        let settings = std::fs::read(&settings_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or_default();
        AppState {
            settings: Mutex::new(settings),
            settings_path,
            cache_dir,
            tools: Mutex::new(None),
            probes: Mutex::new(HashMap::new()),
            queue: Arc::new(Queue::default()),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn save_settings(&self, new: Settings) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&new).map_err(|e| e.to_string())?;
        if let Some(dir) = self.settings_path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        }
        // write a temp file and rename it, so a crash never leaves half a file
        let tmp = self.settings_path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.settings_path)
            .map_err(|e| format!("Cannot write {}: {e}", self.settings_path.display()))?;
        let changed = {
            let mut cur = self.settings.lock().unwrap();
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
        let info = probe::probe(&ffprobe, path)?;
        self.probes.lock().unwrap().insert(path.to_string(), info.clone());
        Ok(info)
    }

    /// Cached probe result, probing when there is none.
    pub fn media(&self, path: &str) -> Result<MediaInfo, String> {
        if let Some(i) = self.probes.lock().unwrap().get(path) {
            return Ok(i.clone());
        }
        self.probe(path)
    }
}
