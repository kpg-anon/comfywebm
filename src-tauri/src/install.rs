//! Installs the external tools from inside the app: ffmpeg (BtbN's GPL
//! builds on Windows and Linux, Homebrew's ffmpeg-full on macOS), yt-dlp and
//! Gyroflow. Nothing is bundled with comfywebm: each is downloaded from its
//! project's latest GitHub release when the user presses Install, unpacked
//! into `<tools dir>/<tool>/current` and checked, and the setting that points
//! at the tool is set to it. Unpacking uses the system's tar (Windows 10 and
//! later ship one that reads zip files), so no archive code is added.

use crate::state::AppState;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Where installed tools live: `<app local data>/tools`, or `data/tools` beside a portable exe.
pub struct ToolsDir(pub PathBuf);

/// Cancel flags of running installs, by tool name.
#[derive(Default)]
pub struct Installs(Mutex<HashMap<String, Arc<AtomicBool>>>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Ffmpeg,
    Ytdlp,
    Gyroflow,
}

impl Tool {
    fn name(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "ffmpeg",
            Tool::Ytdlp => "yt-dlp",
            Tool::Gyroflow => "gyroflow",
        }
    }
}

/// Sent as the `install-progress` event.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub tool: Tool,
    /// "find", "download", "unpack", "check" or "brew"
    pub phase: &'static str,
    pub done: u64,
    pub total: Option<u64>,
    /// a line of Homebrew's output, or the file being fetched
    pub text: Option<String>,
}

/// What an install did.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub tool: Tool,
    /// the folder or file now set in Settings
    pub path: String,
    /// the release that was installed (the file name)
    pub release: String,
}

struct Asset {
    name: String,
    url: String,
    size: u64,
}

const UA: &str = concat!("comfywebm/", env!("CARGO_PKG_VERSION"));

fn client() -> Result<reqwest::Client, String> {
    // reqwest is built without a crypto provider (as for the updater); ring is in the build
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder().user_agent(UA).connect_timeout(Duration::from_secs(20)).build().map_err(|e| e.to_string())
}

/// The newest file in `repo`'s latest release that `pick` accepts; with several, the
/// one `pick` ranks highest.
async fn latest_asset(c: &reqwest::Client, repo: &str, pick: impl Fn(&str) -> Option<Vec<u32>>) -> Result<Asset, String> {
    #[derive(Deserialize)]
    struct Rel {
        assets: Vec<A>,
    }
    #[derive(Deserialize)]
    struct A {
        name: String,
        browser_download_url: String,
        size: u64,
    }
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp = c.get(&url).header("Accept", "application/vnd.github+json").send().await.map_err(|e| format!("Cannot reach GitHub: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub answered {} for {repo}'s latest release.", resp.status()));
    }
    let rel: Rel = resp.json().await.map_err(|e| format!("GitHub's answer for {repo} could not be read: {e}"))?;
    rel.assets
        .into_iter()
        .filter_map(|a| pick(&a.name).map(|rank| (rank, a)))
        .max_by(|x, y| x.0.cmp(&y.0))
        .map(|(_, a)| Asset { name: a.name, url: a.browser_download_url, size: a.size })
        .ok_or_else(|| format!("{repo}'s latest release has no file for this system."))
}

fn emit(app: &AppHandle, p: InstallProgress) {
    let _ = app.emit("install-progress", p);
}

/// Downloads `a` to `dest`, reporting progress; stops when `cancel` is set.
async fn download(app: &AppHandle, tool: Tool, c: &reqwest::Client, a: &Asset, dest: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let resp = c.get(&a.url).send().await.map_err(|e| format!("Download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Download failed: the server answered {}.", resp.status()));
    }
    let total = resp.content_length().or(Some(a.size)).filter(|n| *n > 0);
    let mut file = std::fs::File::create(dest).map_err(|e| format!("Cannot write {}: {e}", dest.display()))?;
    let mut stream = resp.bytes_stream();
    let mut done = 0u64;
    let mut last = Instant::now();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::SeqCst) {
            return Err("Canceled".into());
        }
        let chunk = chunk.map_err(|e| format!("Download failed: {e}"))?;
        file.write_all(&chunk).map_err(|e| format!("Cannot write {}: {e}", dest.display()))?;
        done += chunk.len() as u64;
        if last.elapsed() > Duration::from_millis(200) {
            last = Instant::now();
            emit(app, InstallProgress { tool, phase: "download", done, total, text: Some(a.name.clone()) });
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    if total.is_some_and(|t| done != t) {
        return Err(format!("Download incomplete: {done} of {} bytes.", total.unwrap_or(0)));
    }
    Ok(())
}

/// The system's tar: on Windows the one in System32 (a tar from Git or MSYS on PATH cannot read zip files).
fn tar() -> PathBuf {
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        root.join("System32").join("tar.exe")
    } else {
        PathBuf::from("tar")
    }
}

fn run_ok(mut cmd: std::process::Command, what: &str) -> Result<String, String> {
    let out = comfywebm_engine::tools::output_within(
        {
            cmd.stdin(std::process::Stdio::null());
            cmd
        },
        Duration::from_secs(600),
        what,
    )?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{what} failed: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn unpack(archive: &Path, into: &Path) -> Result<(), String> {
    std::fs::create_dir_all(into).map_err(|e| format!("Cannot create {}: {e}", into.display()))?;
    let mut cmd = comfywebm_engine::tools::command(&tar());
    cmd.arg("-xf").arg(archive).arg("-C").arg(into);
    run_ok(cmd, "Unpacking").map(|_| ())
}

/// macOS: copies the .app out of a disk image.
#[cfg(target_os = "macos")]
fn unpack_dmg(dmg: &Path, into: &Path) -> Result<(), String> {
    let mount = into.with_extension("mount");
    let _ = std::fs::create_dir_all(&mount);
    let mut cmd = std::process::Command::new("hdiutil");
    cmd.args(["attach", "-nobrowse", "-readonly", "-mountpoint"]).arg(&mount).arg(dmg);
    run_ok(cmd, "Opening the disk image")?;
    let res = (|| {
        std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
        let app = std::fs::read_dir(&mount)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "app"))
            .ok_or("The disk image holds no app.")?;
        let mut cp = std::process::Command::new("cp");
        cp.arg("-R").arg(&app).arg(into);
        run_ok(cp, "Copying the app").map(|_| ())
    })();
    let mut det = std::process::Command::new("hdiutil");
    det.args(["detach", "-quiet"]).arg(&mount);
    let _ = run_ok(det, "Closing the disk image");
    let _ = std::fs::remove_dir(&mount);
    res
}

/// The first file under `dir` (depth-first, at most 6 levels) whose name is one of `names`.
fn find_file(dir: &Path, names: &[&str], depth: u32) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut dirs = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            dirs.push(p);
        } else if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| names.iter().any(|w| n.eq_ignore_ascii_case(w))) {
            return Some(p);
        }
    }
    if depth == 0 {
        return None;
    }
    dirs.into_iter().find_map(|d| find_file(&d, names, depth - 1))
}

#[cfg(unix)]
fn make_executable(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(m) = std::fs::metadata(p) {
        let mut perm = m.permissions();
        perm.set_mode(perm.mode() | 0o755);
        let _ = std::fs::set_permissions(p, perm);
    }
}
#[cfg(not(unix))]
fn make_executable(_: &Path) {}

/// The release file for `tool` on this system.
async fn pick_asset(c: &reqwest::Client, tool: Tool) -> Result<Asset, String> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    match tool {
        Tool::Ffmpeg => {
            let (plat, ext) = match (os, arch) {
                ("windows", "x86_64") => ("win64", ".zip"),
                ("linux", "x86_64") => ("linux64", ".tar.xz"),
                ("linux", "aarch64") => ("linuxarm64", ".tar.xz"),
                _ => return Err(format!("No ffmpeg download for {os} {arch}.")),
            };
            latest_asset(c, "BtbN/FFmpeg-Builds", |n| comfywebm_engine::tools::btbn_rank(n, plat, ext)).await
        }
        Tool::Ytdlp => {
            let want = match (os, arch) {
                ("windows", _) => "yt-dlp.exe",
                ("macos", _) => "yt-dlp_macos",
                ("linux", "aarch64") => "yt-dlp_linux_aarch64",
                ("linux", _) => "yt-dlp_linux",
                _ => return Err(format!("No yt-dlp download for {os}.")),
            };
            latest_asset(c, "yt-dlp/yt-dlp", |n| (n == want).then(Vec::new)).await
        }
        Tool::Gyroflow => {
            let want = match (os, arch) {
                ("windows", "aarch64") => "Gyroflow-windows-arm64.zip",
                ("windows", _) => "Gyroflow-windows64.zip",
                ("macos", _) => "Gyroflow-mac-universal.dmg",
                ("linux", "x86_64") => "Gyroflow-linux64.tar.gz",
                _ => return Err(format!("No Gyroflow download for {os} {arch}.")),
            };
            latest_asset(c, "gyroflow/gyroflow", |n| (n == want).then(Vec::new)).await
        }
    }
}

/// macOS: `brew install ffmpeg-full` (Homebrew's plain ffmpeg has no vid.stab), its
/// output sent as progress. Returns the formula's bin folder.
#[cfg(target_os = "macos")]
fn brew_ffmpeg(app: &AppHandle, cancel: &AtomicBool) -> Result<PathBuf, String> {
    use std::io::BufRead;
    let brew = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .ok_or("Homebrew (brew.sh) was not found, and it is how ffmpeg with vid.stab is installed on macOS.")?;
    let mut child = std::process::Command::new(&brew)
        .args(["install", "ffmpeg-full"])
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot run brew: {e}"))?;
    let err = child.stderr.take().map(|e| std::thread::spawn(move || std::io::read_to_string(e).unwrap_or_default()));
    if let Some(out) = child.stdout.take() {
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                return Err("Canceled".into());
            }
            emit(app, InstallProgress { tool: Tool::Ffmpeg, phase: "brew", done: 0, total: None, text: Some(line) });
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        let e = err.and_then(|h| h.join().ok()).unwrap_or_default();
        return Err(format!("brew install ffmpeg-full failed: {}", e.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message")));
    }
    let mut cmd = std::process::Command::new(&brew);
    cmd.args(["--prefix", "ffmpeg-full"]);
    let prefix = run_ok(cmd, "brew --prefix")?;
    Ok(PathBuf::from(prefix.trim()).join("bin"))
}

async fn install_inner(app: &AppHandle, tool: Tool, cancel: Arc<AtomicBool>) -> Result<Installed, String> {
    let root = app.state::<ToolsDir>().0.join(tool.name());
    std::fs::create_dir_all(&root).map_err(|e| format!("Cannot create {}: {e}", root.display()))?;
    emit(app, InstallProgress { tool, phase: "find", done: 0, total: None, text: None });

    #[cfg(target_os = "macos")]
    if tool == Tool::Ffmpeg {
        let a = app.clone();
        let c = cancel.clone();
        let bin = tauri::async_runtime::spawn_blocking(move || brew_ffmpeg(&a, &c)).await.map_err(|e| e.to_string())??;
        return finish(app, tool, &bin, "Homebrew ffmpeg-full".into());
    }

    let c = client()?;
    let asset = pick_asset(&c, tool).await?;
    let file = root.join(format!("download-{}", asset.name));
    let res = download(app, tool, &c, &asset, &file, &cancel).await;
    if let Err(e) = res {
        let _ = std::fs::remove_file(&file);
        return Err(e);
    }
    // unpack next to the copy in use, then swap, so a failure leaves that copy working
    let fresh = root.join("new");
    let _ = std::fs::remove_dir_all(&fresh);
    emit(app, InstallProgress { tool, phase: "unpack", done: 0, total: None, text: Some(asset.name.clone()) });
    let (f2, n2) = (file.clone(), fresh.clone());
    let name = asset.name.clone();
    let unpacked = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        if tool == Tool::Ytdlp {
            std::fs::create_dir_all(&n2).map_err(|e| e.to_string())?;
            let exe = n2.join(if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" });
            std::fs::rename(&f2, &exe).map_err(|e| e.to_string())?;
            make_executable(&exe);
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        if name.ends_with(".dmg") {
            return unpack_dmg(&f2, &n2);
        }
        let _ = &name;
        unpack(&f2, &n2)
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&file);
    if let Err(e) = unpacked {
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(e);
    }
    let current = root.join("current");
    if current.exists() {
        // a tool in use (an encode running) cannot be removed on Windows: keep it and say so
        std::fs::remove_dir_all(&current).map_err(|e| {
            let _ = std::fs::remove_dir_all(&fresh);
            format!("The copy installed before is in use (an encode running?), so it was not replaced: {e}")
        })?;
    }
    std::fs::rename(&fresh, &current).map_err(|e| format!("Cannot move the new copy into place: {e}"))?;

    let target = match tool {
        Tool::Ffmpeg => {
            let exe = find_file(&current, &[if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }], 6).ok_or("The download holds no ffmpeg.")?;
            exe.parent().map(Path::to_path_buf).ok_or("ffmpeg has no folder")?
        }
        Tool::Ytdlp => current.join(if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" }),
        Tool::Gyroflow => {
            let names: &[&str] = if cfg!(windows) { &["Gyroflow.exe"] } else { &["gyroflow", "Gyroflow"] };
            let exe = find_file(&current, names, 6).ok_or("The download holds no Gyroflow program.")?;
            make_executable(&exe);
            exe
        }
    };
    if tool == Tool::Ffmpeg {
        for n in ["ffmpeg", "ffprobe"] {
            make_executable(&target.join(n));
        }
    }
    finish(app, tool, &target, asset.name)
}

/// Checks the tool runs, then points its setting at it.
fn finish(app: &AppHandle, tool: Tool, target: &Path, release: String) -> Result<Installed, String> {
    emit(app, InstallProgress { tool, phase: "check", done: 0, total: None, text: None });
    let exe = match tool {
        Tool::Ffmpeg => target.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }),
        _ => target.to_path_buf(),
    };
    let arg = match tool {
        Tool::Ffmpeg => "-version",
        Tool::Ytdlp => "--version",
        Tool::Gyroflow => "--version",
    };
    let mut cmd = comfywebm_engine::tools::command(&exe);
    cmd.arg(arg);
    run_ok(cmd, &format!("{} {arg}", tool.name()))?;
    let state = app.state::<AppState>();
    let mut s = state.settings();
    let path = target.to_string_lossy().into_owned();
    match tool {
        Tool::Ffmpeg => s.ffmpeg_path = Some(path.clone()),
        Tool::Ytdlp => s.ytdlp_path = Some(path.clone()),
        Tool::Gyroflow => s.gyroflow_path = Some(path.clone()),
    }
    state.save_settings(s)?;
    state.forget_tools();
    Ok(Installed { tool, path, release })
}

#[tauri::command]
pub async fn tool_install(app: AppHandle, tool: Tool) -> Result<Installed, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let installs = app.state::<Installs>();
        let mut m = installs.0.lock().unwrap();
        if m.contains_key(tool.name()) {
            return Err(format!("{} is being installed already.", tool.name()));
        }
        m.insert(tool.name().into(), cancel.clone());
    }
    let res = install_inner(&app, tool, cancel).await;
    app.state::<Installs>().0.lock().unwrap().remove(tool.name());
    res
}

#[tauri::command]
pub fn tool_install_cancel(app: AppHandle, tool: Tool) {
    if let Some(c) = app.state::<Installs>().0.lock().unwrap().get(tool.name()) {
        c.store(true, Ordering::SeqCst);
    }
}

/// The copy of `tool` the app installed, if any (its folder under the tools dir).
#[tauri::command]
pub fn tool_installed(app: AppHandle, tool: Tool) -> Option<String> {
    let p = app.state::<ToolsDir>().0.join(tool.name()).join("current");
    p.is_dir().then(|| p.to_string_lossy().into_owned())
}

/// Deletes the copy the app installed and clears the setting when it pointed there.
#[tauri::command]
pub fn tool_remove(app: AppHandle, tool: Tool) -> Result<(), String> {
    let root = app.state::<ToolsDir>().0.join(tool.name());
    let state = app.state::<AppState>();
    let mut s = state.settings();
    let inside = |p: &Option<String>| p.as_deref().is_some_and(|p| Path::new(p).starts_with(&root));
    let changed = match tool {
        Tool::Ffmpeg if inside(&s.ffmpeg_path) => {
            s.ffmpeg_path = None;
            true
        }
        Tool::Ytdlp if inside(&s.ytdlp_path) => {
            s.ytdlp_path = None;
            true
        }
        Tool::Gyroflow if inside(&s.gyroflow_path) => {
            s.gyroflow_path = None;
            true
        }
        _ => false,
    };
    if changed {
        state.save_settings(s)?;
        state.forget_tools();
    }
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| format!("Cannot delete {} (in use?): {e}", root.display()))?;
    }
    Ok(())
}
