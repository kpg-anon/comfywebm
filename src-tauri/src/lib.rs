mod queue;
mod state;

use comfywebm_engine::run::{self, Cancel, RunError};
use comfywebm_engine::{
    naming, plan, proxy, ytdl, Codec, Container, Crop, DownloadProgress, EncodeSpec, Job, MediaInfo, PlanInfo, Preset,
    Segment, Settings, ToolsStatus, UrlInfo,
};
use serde::Serialize;
use state::AppState;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// Running downloads by the exact url string the frontend passed.
#[derive(Default)]
struct Downloads(Mutex<HashMap<String, Arc<Cancel>>>);

/// Runs blocking work (ffprobe, ffmpeg, yt-dlp) off the main thread and the async workers.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

fn run_err(e: RunError) -> String {
    match e {
        RunError::Canceled => "Canceled".into(),
        RunError::Failed(s) => s,
    }
}

#[tauri::command]
async fn tools_status(app: AppHandle) -> Result<ToolsStatus, String> {
    blocking(move || Ok(app.state::<AppState>().tools().1)).await
}

/// First command-line argument, if it is an existing file.
#[tauri::command]
fn initial_file() -> Option<String> {
    let arg = std::env::args_os().nth(1)?;
    let p = std::path::Path::new(&arg);
    if !p.is_file() {
        return None;
    }
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    Some(abs.to_string_lossy().into_owned())
}

#[tauri::command]
async fn probe(app: AppHandle, path: String) -> Result<MediaInfo, String> {
    blocking(move || app.state::<AppState>().probe(&path)).await
}

#[derive(Clone, Serialize)]
struct ProxyProgress {
    path: String,
    fraction: f64,
}

#[tauri::command]
async fn make_proxy(app: AppHandle, path: String) -> Result<String, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ffmpeg = state.ffmpeg()?;
        let duration = state.media(&path).map(|i| i.duration).unwrap_or(0.0);
        let mut last = Instant::now() - Duration::from_secs(1);
        let out = proxy::make_proxy(&ffmpeg, &path, &state.cache_dir, duration, |fraction| {
            if fraction >= 1.0 || last.elapsed() >= Duration::from_millis(200) {
                last = Instant::now();
                let _ = app.emit("proxy-progress", ProxyProgress { path: path.clone(), fraction });
            }
        })?;
        Ok(out.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
async fn plan(app: AppHandle, spec: EncodeSpec) -> Result<PlanInfo, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let info = state.media(&spec.input)?;
        let settings = state.settings();
        let (_, st) = state.tools();
        let zscale = st.filters.zscale;
        // first build: output height for the name template
        let blank = plan::Opts { display: true, zscale, ..plan::Opts::new("", "") };
        let height = plan::build(&spec, &info, &blank)?.out_height;
        let out = naming::output_path(&spec, &info, height, settings.output_dir.as_deref(), &settings.name_template);
        let out = out.to_string_lossy().into_owned();
        let passlog = std::env::temp_dir().join("comfywebm-pass").to_string_lossy().into_owned();
        let opts = plan::Opts { display: true, zscale, ..plan::Opts::new(&out, &passlog) };
        let mut info = plan::build(&spec, &info, &opts)?.info();
        if st.ffmpeg.is_some() {
            let e = &st.encoders;
            let (have, name) = match spec.codec {
                Codec::Vp9 => (e.vp9, "libvpx-vp9"),
                Codec::Vp8 => (e.vp8, "libvpx"),
                Codec::Av1 => (e.av1, "libsvtav1"),
                Codec::H264 => (e.h264, "libx264"),
                Codec::Hevc => (e.hevc, "libx265"),
            };
            if !have {
                info.warnings.push(format!("This ffmpeg has no {name} encoder."));
            }
            if spec.audio.is_some() {
                let (have, name) = match (spec.container, spec.codec) {
                    (Container::Mp4, _) => (e.aac, "aac"),
                    (_, Codec::Vp8) => (e.vorbis, "libvorbis"),
                    _ => (e.opus, "libopus"),
                };
                if !have {
                    info.warnings.push(format!("This ffmpeg has no {name} encoder."));
                }
            }
            if spec.subs.is_some() && !st.filters.subtitles {
                info.warnings.push("This ffmpeg has no subtitles filter, so subtitles cannot be burned in.".into());
            }
        }
        Ok(info)
    })
    .await
}

#[tauri::command]
fn queue_list(state: State<AppState>) -> Vec<Job> {
    state.queue.list()
}

#[tauri::command]
fn queue_add(state: State<AppState>, spec: EncodeSpec, start: bool) -> Job {
    state.queue.add(spec, start)
}

#[tauri::command]
fn queue_add_many(state: State<AppState>, specs: Vec<EncodeSpec>, start: bool) -> Vec<Job> {
    state.queue.add_many(specs, start)
}

#[tauri::command]
fn queue_start(state: State<AppState>) {
    state.queue.start()
}

#[tauri::command]
fn queue_cancel(state: State<AppState>, id: u64) -> Result<(), String> {
    state.queue.cancel(id)
}

#[tauri::command]
fn queue_remove(state: State<AppState>, id: u64) -> Result<(), String> {
    state.queue.remove(id)
}

#[tauri::command]
fn settings_get(state: State<AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
fn settings_set(state: State<AppState>, settings: Settings) -> Result<(), String> {
    state.save_settings(settings)
}

#[tauri::command]
async fn url_info(app: AppHandle, url: String) -> Result<UrlInfo, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ytdlp = state.ytdlp()?;
        ytdl::url_info(&ytdlp, &url, &state.settings().ytdlp_args)
    })
    .await
}

#[tauri::command]
async fn url_download(app: AppHandle, url: String, max_height: Option<u32>, section: Option<Segment>) -> Result<String, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ytdlp = state.ytdlp()?;
        let ffmpeg = state.ffmpeg()?;
        let cancel = Arc::new(Cancel::new());
        let downloads = app.state::<Downloads>();
        {
            let mut map = downloads.0.lock().unwrap();
            if map.contains_key(&url) {
                return Err("This link is already downloading.".into());
            }
            map.insert(url.clone(), cancel.clone());
        }
        let res = ytdl::download(
            &ytdlp,
            &ffmpeg,
            &url,
            &state.download_dir(),
            max_height,
            section,
            &state.settings().ytdlp_args,
            &cancel,
            |p| {
                // `url` is the exact string the frontend passed; it filters events by it
                let _ = app.emit(
                    "download-progress",
                    DownloadProgress { url: url.clone(), fraction: p.fraction, speed: p.speed, eta: p.eta, stage: p.stage.into() },
                );
            },
        );
        downloads.0.lock().unwrap().remove(&url);
        Ok(res.map_err(run_err)?.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
fn url_cancel(downloads: State<Downloads>, url: String) {
    if let Some(c) = downloads.0.lock().unwrap().get(&url) {
        c.cancel();
    }
}

#[tauri::command]
async fn export_frame(app: AppHandle, path: String, time: f64, crop: Option<Crop>) -> Result<String, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ffmpeg = state.ffmpeg()?;
        let info = state.media(&path)?;
        let out = naming::frame_path(&path, time, state.settings().output_dir.as_deref());
        run::export_frame(&ffmpeg, &info, time, crop, &out, state.zscale())?;
        Ok(out.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
async fn preview_sample(app: AppHandle, spec: EncodeSpec, at: f64) -> Result<String, String> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    blocking(move || {
        let state = app.state::<AppState>();
        let ffmpeg = state.ffmpeg()?;
        let info = state.media(&spec.input)?;
        let dir = state.cache_dir.join("samples");
        std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        // only the newest sample is kept
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let out = dir.join(format!("sample-{}-{n}.{}", std::process::id(), spec.container.ext()));
        run::sample(&ffmpeg, &spec, &info, at, &out, state.zscale(), &Cancel::new()).map_err(run_err)?;
        Ok(out.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
fn presets_export(path: String, presets: Vec<Preset>) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(&presets).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("Cannot write {path}: {e}"))
}

/// Reads a JSON array of presets (or `{ "presets": [...] }`, as in settings.json).
/// Entries that are not valid presets are skipped.
#[tauri::command]
fn presets_import(path: String) -> Result<Vec<Preset>, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| format!("{path} is not JSON: {e}"))?;
    let list = match v {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(mut o) => match o.remove("presets") {
            Some(serde_json::Value::Array(a)) => a,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    let presets: Vec<Preset> = list.into_iter().filter_map(|p| serde_json::from_value(p).ok()).collect();
    if presets.is_empty() {
        return Err(format!("{path} contains no presets."));
    }
    Ok(presets)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Downloads::default())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let cache_dir = app.path().app_cache_dir()?;
            app.manage(AppState::new(config_dir, cache_dir));
            queue::spawn_worker(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tools_status,
            initial_file,
            probe,
            make_proxy,
            plan,
            queue_list,
            queue_add,
            queue_add_many,
            queue_start,
            queue_cancel,
            queue_remove,
            settings_get,
            settings_set,
            url_info,
            url_download,
            url_cancel,
            export_frame,
            preview_sample,
            presets_export,
            presets_import,
        ])
        .run(tauri::generate_context!())
        .expect("error while running comfywebm");
}
