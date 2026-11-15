mod queue;
mod state;
mod winstate;

use comfywebm_engine::config;
use comfywebm_engine::run::{self, Cancel, RunError};
use comfywebm_engine::{
    naming, plan, proxy, strip, ytdl, Codec, Container, Crop, DownloadProgress, EncodeSpec, Job, MediaInfo, PlanInfo, Preset,
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

/// Running timeline picture jobs by kind ("thumbs", "wave"); a new request of
/// a kind cancels the previous one (the user opened another file).
#[derive(Default)]
struct Strips(Mutex<HashMap<&'static str, Arc<Cancel>>>);

fn strip_job<T>(app: &AppHandle, kind: &'static str, f: impl FnOnce(&Cancel) -> Result<T, RunError>) -> Result<T, String> {
    let strips = app.state::<Strips>();
    let cancel = Arc::new(Cancel::new());
    if let Some(old) = strips.0.lock().unwrap().insert(kind, cancel.clone()) {
        old.cancel();
    }
    let res = f(&cancel);
    let mut map = strips.0.lock().unwrap();
    if map.get(kind).is_some_and(|c| Arc::ptr_eq(c, &cancel)) {
        map.remove(kind);
    }
    res.map_err(run_err)
}

/// Filmstrip JPEG for the timeline (cached).
#[tauri::command]
async fn timeline_thumbs(app: AppHandle, path: String) -> Result<String, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ffmpeg = state.ffmpeg()?;
        let info = state.media(&path)?;
        let out = strip_job(&app, "thumbs", |c| strip::thumbs(&ffmpeg, &info, &state.cache_dir, c))?;
        Ok(out.to_string_lossy().into_owned())
    })
    .await
}

/// Waveform PNG of one audio track for the timeline (cached).
#[tauri::command]
async fn timeline_wave(app: AppHandle, path: String, track: u32) -> Result<String, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ffmpeg = state.ffmpeg()?;
        let info = state.media(&path)?;
        let out = strip_job(&app, "wave", |c| strip::waveform(&ffmpeg, &info, track, &state.cache_dir, c))?;
        Ok(out.to_string_lossy().into_owned())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheInfo {
    dir: String,
    /// preview proxies, samples and timeline pictures
    bytes: u64,
    files: u64,
    /// the default download folder for "Download and edit" (inside the cache dir)
    download_bytes: u64,
}

fn dir_size(dir: &std::path::Path, depth: u32) -> (u64, u64) {
    let mut t = (0, 0);
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            match e.file_type() {
                Ok(ft) if ft.is_dir() && depth < 4 => {
                    let (b, n) = dir_size(&e.path(), depth + 1);
                    t = (t.0 + b, t.1 + n);
                }
                Ok(ft) if ft.is_file() => {
                    t = (t.0 + e.metadata().map(|m| m.len()).unwrap_or(0), t.1 + 1);
                }
                _ => {}
            }
        }
    }
    t
}

#[tauri::command]
async fn cache_info(app: AppHandle) -> Result<CacheInfo, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let dir = &state.cache_dir;
        let (all, files) = dir_size(dir, 0);
        let dl = dir.join("downloads");
        let (dl_bytes, dl_files) = dir_size(&dl, 0);
        Ok(CacheInfo { dir: dir.to_string_lossy().into_owned(), bytes: all - dl_bytes, files: files - dl_files, download_bytes: dl_bytes })
    })
    .await
}

/// Deletes proxies, samples and timeline pictures; with `downloads`, also the
/// default download folder in the cache dir. Files in use (Windows) are skipped.
#[tauri::command]
async fn cache_clear(app: AppHandle, downloads: bool) -> Result<(), String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let dir = &state.cache_dir;
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if name == "downloads" {
                    if downloads {
                        let _ = std::fs::remove_dir_all(&p);
                    }
                } else if name == "samples" || name == "strips" {
                    let _ = std::fs::remove_dir_all(&p);
                } else if name.starts_with("proxy-") {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
        Ok(())
    })
    .await
}

/// The running sample encode (a new one replaces it) and the sample files
/// this process wrote, newest last.
#[derive(Default)]
struct Samples {
    running: Mutex<Option<Arc<Cancel>>>,
    files: Mutex<Vec<std::path::PathBuf>>,
}

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

#[tauri::command]
async fn tools_rescan(app: AppHandle) -> Result<ToolsStatus, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        state.forget_tools();
        Ok(state.tools().1)
    })
    .await
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
        let out = naming::output_path(&spec, &info, height, state.output_dir().as_deref(), &settings.name_template);
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
                Codec::Gif => (e.gif, "gif"),
                Codec::Webp => (e.webp, "libwebp_anim"),
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
fn queue_add(app: AppHandle, state: State<AppState>, spec: EncodeSpec, start: bool) -> Job {
    let j = state.queue.add(spec, start);
    queue::emit_state(&app, &state.queue);
    j
}

#[tauri::command]
fn queue_add_many(app: AppHandle, state: State<AppState>, specs: Vec<EncodeSpec>, start: bool) -> Vec<Job> {
    let j = state.queue.add_many(specs, start);
    queue::emit_state(&app, &state.queue);
    j
}

#[tauri::command]
fn queue_start(app: AppHandle, state: State<AppState>) {
    state.queue.start();
    queue::emit_state(&app, &state.queue);
}

/// Stop after the running job.
#[tauri::command]
fn queue_pause(app: AppHandle, state: State<AppState>) {
    state.queue.pause();
    queue::emit_state(&app, &state.queue);
}

#[tauri::command]
fn queue_state(state: State<AppState>) -> queue::QueueState {
    state.queue.state()
}

/// A finished, failed or canceled job goes back to pending at the end of the list.
#[tauri::command]
fn queue_retry(state: State<AppState>, id: u64) -> Result<Job, String> {
    state.queue.retry(id)
}

/// Moves a job to position `to`; returns the new list.
#[tauri::command]
fn queue_move(state: State<AppState>, id: u64, to: usize) -> Result<Vec<Job>, String> {
    state.queue.move_to(id, to)
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
        ytdl::url_info(&ytdlp, &url, &state.ytdlp_extra())
    })
    .await
}

#[tauri::command]
/// `keep`: "Download only". The file goes to the keep folder (Settings, else
/// the user's Downloads folder) in its own container where one fits, instead
/// of an MKV in the download folder.
async fn url_download(app: AppHandle, url: String, max_height: Option<u32>, section: Option<Segment>, keep: Option<bool>) -> Result<String, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ytdlp = state.ytdlp()?;
        let ffmpeg = state.ffmpeg()?;
        let keep = keep.unwrap_or(false);
        let dir = if keep {
            match state.settings().keep_dir.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()) {
                Some(d) => std::path::PathBuf::from(d),
                None => app.path().download_dir().map_err(|e| format!("Cannot find the Downloads folder: {e}. Set one in Settings."))?,
            }
        } else {
            state.download_dir()
        };
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
            &dir,
            max_height,
            section,
            if keep { "mp4/webm/mkv" } else { "mkv" },
            &state.ytdlp_extra(),
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
        let out = naming::frame_path(&path, time, state.output_dir().as_deref());
        let out = run::export_frame(&ffmpeg, &info, time, crop, &out, state.zscale())?;
        Ok(out.to_string_lossy().into_owned())
    })
    .await
}

/// The frame at `time` as the encode sees it (all filters and the output size), as a PNG.
#[tauri::command]
async fn export_frame_processed(app: AppHandle, spec: EncodeSpec, time: f64) -> Result<String, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ffmpeg = state.ffmpeg()?;
        let info = state.media(&spec.input)?;
        let out = naming::frame_path(&spec.input, time, state.output_dir().as_deref());
        let out = run::export_frame_processed(&ffmpeg, &spec, &info, time, &out, state.zscale())?;
        Ok(out.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
async fn preview_sample(app: AppHandle, spec: EncodeSpec, at: f64) -> Result<String, String> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    blocking(move || {
        let state = app.state::<AppState>();
        let samples = app.state::<Samples>();
        let ffmpeg = state.ffmpeg()?;
        let info = state.media(&spec.input)?;
        let dir = state.cache_dir.join("samples");
        std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        // a newer sample replaces one still encoding
        let cancel = Arc::new(Cancel::new());
        if let Some(old) = samples.running.lock().unwrap().replace(cancel.clone()) {
            old.cancel();
        }
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let out = dir.join(format!("sample-{}-{n}.{}", std::process::id(), spec.container.ext()));
        let res = run::sample(&ffmpeg, &spec, &info, at, &out, state.zscale(), &cancel);
        {
            let mut r = samples.running.lock().unwrap();
            if r.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cancel)) {
                *r = None;
            }
        }
        res.map_err(run_err)?;
        // keep this process's two newest samples (one may still be playing); others are deleted
        let mut files = samples.files.lock().unwrap();
        files.push(out.clone());
        while files.len() > 2 {
            let _ = std::fs::remove_file(files.remove(0));
        }
        Ok(out.to_string_lossy().into_owned())
    })
    .await
}

/// Stops the sample encode, if one runs.
#[tauri::command]
fn sample_cancel(samples: State<Samples>) {
    if let Some(c) = samples.running.lock().unwrap().take() {
        c.cancel();
    }
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

/// config.toml lists and every preset file.
#[tauri::command]
fn config_get(state: State<AppState>) -> config::ConfigInfo {
    state.config.load()
}

/// Writes a preset; `file` replaces that preset file, None makes a new one in saved/.
#[tauri::command]
fn preset_save(state: State<AppState>, preset: Preset, file: Option<String>) -> Result<String, String> {
    state.config.save(&preset, file.as_deref())
}

#[tauri::command]
fn preset_delete(state: State<AppState>, file: String) -> Result<(), String> {
    state.config.delete(&file)
}

/// Writes the platform presets again (files with other names are kept).
#[tauri::command]
fn config_restore_platform(state: State<AppState>) {
    state.config.write_platform();
}

/// The output folder new jobs use (Desktop, the custom folder), or null for beside the source.
#[tauri::command]
fn output_dir(state: State<AppState>) -> Option<String> {
    state.output_dir()
}

const MEDIA_EXT: &[&str] = &[
    "mp4", "mkv", "webm", "mov", "avi", "m4v", "ts", "m2ts", "mts", "flv", "wmv", "mpg", "mpeg", "ogv", "3gp", "gif",
];

/// Video files in a folder (and its subfolders when `recursive`), sorted by path.
#[tauri::command]
fn list_media(dir: String, recursive: bool) -> Result<Vec<String>, String> {
    fn walk(dir: &std::path::Path, recursive: bool, depth: u32, out: &mut Vec<String>) -> Result<(), String> {
        let rd = std::fs::read_dir(dir).map_err(|e| format!("Cannot read {}: {e}", dir.display()))?;
        for e in rd.flatten() {
            let p = e.path();
            let ft = e.file_type().map_err(|e| e.to_string())?;
            if ft.is_dir() {
                if recursive && depth < 8 {
                    walk(&p, recursive, depth + 1, out)?;
                }
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| MEDIA_EXT.contains(&x.to_ascii_lowercase().as_str())) {
                out.push(p.to_string_lossy().into_owned());
            }
            if out.len() > 5000 {
                return Err("More than 5000 files; choose a smaller folder.".into());
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(std::path::Path::new(&dir), recursive, 0, &mut out)?;
    out.sort_by_key(|p| p.to_lowercase());
    Ok(out)
}

/// Theme export: the save dialog runs here, so the page never names a path.
/// Returns the path written, or None when the dialog was canceled.
#[tauri::command]
async fn theme_export(app: AppHandle, text: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    if text.len() > 256 * 1024 {
        return Err("The theme is too large.".into());
    }
    let picked = app
        .dialog()
        .file()
        .set_title("Export theme")
        .add_filter("comfywebm theme", &["json"])
        .set_file_name("comfywebm-theme.json")
        .blocking_save_file();
    let Some(fp) = picked else { return Ok(None) };
    let path = fp.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Theme import: the open dialog runs here. Returns the file's text (at most
/// 256 KB), or None when the dialog was canceled.
#[tauri::command]
async fn theme_import(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().set_title("Import theme").add_filter("comfywebm theme", &["json"]).blocking_pick_file();
    let Some(fp) = picked else { return Ok(None) };
    let path = fp.into_path().map_err(|e| e.to_string())?;
    let len = std::fs::metadata(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?.len();
    if len > 256 * 1024 {
        return Err(format!("{} is larger than 256 KB; it is not a theme file.", path.display()));
    }
    std::fs::read_to_string(&path).map(Some).map_err(|e| format!("Cannot read {}: {e}", path.display()))
}

/// Preview proxies and timeline pictures above this total are deleted at
/// start, oldest first. Downloads and samples are not counted.
const CACHE_LIMIT: u64 = 4 << 30;

fn trim_cache(dir: &std::path::Path) {
    let mut files: Vec<(std::time::SystemTime, u64, std::path::PathBuf)> = Vec::new();
    let mut collect = |d: &std::path::Path, f: &dyn Fn(&str) -> bool| {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let (true, Ok(m)) = (f(&name), e.metadata()) {
                    if m.is_file() {
                        files.push((m.modified().or_else(|_| m.accessed()).unwrap_or(std::time::UNIX_EPOCH), m.len(), e.path()));
                    }
                }
            }
        }
    };
    collect(dir, &|n| n.starts_with("proxy-"));
    collect(&dir.join("strips"), &|_| true);
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= CACHE_LIMIT {
        return;
    }
    files.sort_by_key(|f| f.0);
    for (_, len, p) in files {
        if total <= CACHE_LIMIT {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Downloads::default())
        .manage(Samples::default())
        .manage(Strips::default())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let cache_dir = app.path().app_cache_dir()?;
            if let Some(win) = app.get_webview_window("main") {
                winstate::install(&win, config_dir.join("window.json"));
            }
            let desktop = app.path().desktop_dir().ok();
            trim_cache(&cache_dir);
            app.manage(AppState::new(config_dir, cache_dir, desktop));
            queue::spawn_worker(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tools_status,
            tools_rescan,
            initial_file,
            probe,
            make_proxy,
            plan,
            queue_list,
            queue_add,
            queue_add_many,
            queue_start,
            queue_pause,
            queue_state,
            queue_retry,
            queue_move,
            sample_cancel,
            export_frame_processed,
            timeline_thumbs,
            timeline_wave,
            cache_info,
            cache_clear,
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
            config_get,
            preset_save,
            preset_delete,
            config_restore_platform,
            output_dir,
            list_media,
            theme_export,
            theme_import,
        ])
        .build(tauri::generate_context!())
        .expect("error while building comfywebm")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // no ffmpeg or yt-dlp may outlive the app
                if let Some(state) = app.try_state::<AppState>() {
                    state.queue.shutdown();
                }
                for c in app.state::<Downloads>().0.lock().unwrap().values() {
                    c.cancel();
                }
                for c in app.state::<Strips>().0.lock().unwrap().values() {
                    c.cancel();
                }
                let samples = app.state::<Samples>();
                if let Some(c) = samples.running.lock().unwrap().take() {
                    c.cancel();
                }
                for f in samples.files.lock().unwrap().drain(..) {
                    let _ = std::fs::remove_file(f);
                }
            }
        });
}
