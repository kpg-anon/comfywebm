//! Installs the external tools from inside the app: ffmpeg (BtbN's GPL
//! builds on Windows and Linux, Homebrew's ffmpeg-full on macOS), yt-dlp and
//! Gyroflow. Nothing is bundled with comfywebm: each is downloaded from its
//! project's latest GitHub release when the user presses Install, unpacked
//! into `<tools dir>/<tool>/current` and checked, and the setting that points
//! at the tool is set to it. Unpacking uses the system's tar (Windows 10 and
//! later ship one that reads zip files), so no archive code is added.
//!
//! RIFE (engine rife.rs) is rife-ncnn-vulkan from TNTwise's GitHub releases. The
//! release file holds every RIFE model (about 830 MB); only the program and the
//! model the engine uses (rife.rs MODEL) are unpacked, about 20 MB.
//!
//! The Python trackers (VitTrack, SAM 2; engine pytrack.rs) get their own
//! environment each, `<tools dir>/<tool>/current/env`, made by uv (Astral's
//! single-file Python installer, fetched into `<tools dir>/uv`), with a Python
//! uv downloads into `<tools dir>/py` and its cache in `<tools dir>/uvc`. No
//! Python on the system is used or changed.

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
    Vittrack,
    Sam2,
    Rife,
}

impl Tool {
    fn name(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "ffmpeg",
            Tool::Ytdlp => "yt-dlp",
            Tool::Gyroflow => "gyroflow",
            Tool::Vittrack => "vittrack",
            Tool::Sam2 => "sam2",
            Tool::Rife => "rife",
        }
    }
    fn is_python(self) -> bool {
        matches!(self, Tool::Vittrack | Tool::Sam2)
    }
}

/// Sent as the `install-progress` event.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub tool: Tool,
    /// "find", "download", "unpack", "check", "brew" or "pip" (uv's output in `text`)
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
    unpack_only(archive, into, &[])
}

/// Unpacks the files of `archive` whose paths match one of `members` (tar wildcards; all when empty).
fn unpack_only(archive: &Path, into: &Path, members: &[&str]) -> Result<(), String> {
    std::fs::create_dir_all(into).map_err(|e| format!("Cannot create {}: {e}", into.display()))?;
    let mut cmd = comfywebm_engine::tools::command(&tar());
    cmd.arg("-xf").arg(archive).arg("-C").arg(into).args(members);
    run_ok(cmd, "Unpacking").map(|_| ())
}

/// What RIFE's release file is unpacked to: the program (and on Windows the OpenMP library
/// beside it), its licence, and the model the engine uses.
fn rife_members() -> Vec<String> {
    let mut v = vec![format!("*/{}/*", comfywebm_engine::rife::MODEL), "*/LICENSE".to_string()];
    if cfg!(windows) {
        v.extend(["*/rife-ncnn-vulkan.exe".into(), "*/vcomp140.dll".into()]);
    } else {
        v.push("*/rife-ncnn-vulkan".into());
    }
    v
}

fn rife_exe_name() -> &'static str {
    if cfg!(windows) {
        "rife-ncnn-vulkan.exe"
    } else {
        "rife-ncnn-vulkan"
    }
}

/// The installed RIFE (`<tools>/rife/current`), with ffprobe for reading its files; None when
/// it is not installed or ffprobe is not found.
pub fn rife_tool(tools: &Path, ffprobe: Option<PathBuf>) -> Option<comfywebm_engine::rife::Rife> {
    let exe = find_file(&tools.join("rife").join("current"), &[rife_exe_name()], 4)?;
    let model = exe.parent()?.join(comfywebm_engine::rife::MODEL);
    if !model.join("flownet.bin").is_file() {
        return None;
    }
    Some(comfywebm_engine::rife::Rife { exe, model, ffprobe: ffprobe? })
}

/// A 24-bit BMP of one colour.
fn plain_bmp(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut b = Vec::with_capacity(size as usize);
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&size.to_le_bytes());
    b.extend_from_slice(&[0, 0, 0, 0]);
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&(w as i32).to_le_bytes());
    b.extend_from_slice(&(h as i32).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&[0u8; 24]);
    for _ in 0..h {
        for _ in 0..w {
            b.extend_from_slice(&[rgb[2], rgb[1], rgb[0]]);
        }
        b.extend(std::iter::repeat_n(0u8, (row - w * 3) as usize));
    }
    b
}

/// Makes one in-between picture of two test pictures: the program runs and a graphics
/// card takes the model. Returns the graphics cards it lists.
fn rife_check(exe: &Path) -> Result<String, String> {
    let model = exe.parent().map(|d| d.join(comfywebm_engine::rife::MODEL)).ok_or("RIFE has no folder")?;
    let dir = std::env::temp_dir().join(format!("comfywebm-rife-check-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let (a, b, out) = (dir.join("a.bmp"), dir.join("b.bmp"), dir.join("c.png"));
    let res = (|| {
        std::fs::write(&a, plain_bmp(64, 64, [200, 40, 40])).map_err(|e| e.to_string())?;
        std::fs::write(&b, plain_bmp(64, 64, [40, 40, 200])).map_err(|e| e.to_string())?;
        let mut cmd = comfywebm_engine::tools::command(exe);
        cmd.arg("-0").arg(&a).arg("-1").arg(&b).arg("-o").arg(&out).arg("-m").arg(&model);
        if let Some(d) = exe.parent() {
            cmd.current_dir(d);
        }
        cmd.stdin(std::process::Stdio::null());
        let o = comfywebm_engine::tools::output_within(cmd, Duration::from_secs(120), "RIFE")?;
        let text = String::from_utf8_lossy(&o.stderr).into_owned();
        if !o.status.success() || !out.is_file() {
            let why = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message");
            return Err(format!("RIFE did not run on this computer: {why}. It needs a graphics card with Vulkan (an up-to-date graphics driver)."));
        }
        // "[1 NVIDIA GeForce RTX 4070 Laptop GPU]  queueC=..." several lines per card
        let mut cards: Vec<String> = Vec::new();
        for l in text.lines() {
            let Some(inner) = l.strip_prefix('[').and_then(|r| r.split_once(']')).map(|(n, _)| n) else { continue };
            let name = inner.split_once(' ').map_or(inner, |(_, x)| x).to_string();
            if !cards.contains(&name) {
                cards.push(name);
            }
        }
        Ok(cards.join(", "))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    res
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
        Tool::Rife => {
            let want = match (os, arch) {
                ("windows", "x86_64") => "windows.zip",
                ("linux", "x86_64") => "ubuntu.zip",
                ("macos", _) => "macos.zip",
                _ => return Err(format!("No RIFE download for {os} {arch}.")),
            };
            latest_asset(c, "TNTwise/rife-ncnn-vulkan", |n| (n == want).then(Vec::new)).await
        }
        Tool::Vittrack | Tool::Sam2 => Err("The trackers are installed with uv, not from a release file.".into()),
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
    if tool.is_python() {
        let r = python_install(app, tool, cancel).await;
        // nothing installed before: no empty folder left behind
        if r.is_err() && !root.join("current").exists() {
            let _ = std::fs::remove_dir_all(&root);
        }
        return r;
    }

    #[cfg(target_os = "macos")]
    if tool == Tool::Ffmpeg {
        let a = app.clone();
        let c = cancel.clone();
        let bin = tauri::async_runtime::spawn_blocking(move || brew_ffmpeg(&a, &c)).await.map_err(|e| e.to_string())??;
        return finish(app, tool, &bin, "Homebrew ffmpeg-full".into());
    }

    let c = client()?;
    let asset = pick_asset(&c, tool).await?;
    // the release file and its unpacked part, with room to spare
    if let Some(free) = free_bytes(&root) {
        let need = asset.size + (300 << 20);
        if free < need {
            let gb = |b: u64| format!("{:.1} GB", b as f64 / (1u64 << 30) as f64);
            return Err(format!("This needs about {} free on the drive of {}, and it has {}.", gb(need), root.display(), gb(free)));
        }
    }
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
        if tool == Tool::Rife {
            let m = rife_members();
            return unpack_only(&f2, &n2, &m.iter().map(String::as_str).collect::<Vec<_>>());
        }
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
        Tool::Rife => {
            let exe = find_file(&current, &[rife_exe_name()], 4).ok_or("The download holds no RIFE program.")?;
            make_executable(&exe);
            exe
        }
        Tool::Vittrack | Tool::Sam2 => return Err("The trackers are installed with uv.".into()),
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
    if tool == Tool::Rife {
        // found by its folder, as the trackers are; no setting points at it
        let cards = rife_check(target)?;
        // the release file is named after the system only ("windows.zip"), so it is not shown
        let _ = release;
        let label = format!("RIFE {} (rife-ncnn-vulkan)", comfywebm_engine::rife::MODEL.trim_start_matches("rife-v"));
        let release = if cards.is_empty() { label } else { format!("{label}; graphics: {cards}") };
        return Ok(Installed { tool, path: target.to_string_lossy().into_owned(), release });
    }
    let exe = match tool {
        Tool::Ffmpeg => target.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }),
        _ => target.to_path_buf(),
    };
    let arg = match tool {
        Tool::Ffmpeg => "-version",
        _ => "--version",
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
        Tool::Vittrack | Tool::Sam2 | Tool::Rife => {}
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
    // the last Python tracker gone: uv, its Python and its cache too
    let tools = &app.state::<ToolsDir>().0;
    if tool.is_python() && [Tool::Vittrack, Tool::Sam2].iter().all(|t| !tools.join(t.name()).exists()) {
        for d in ["uv", "py", "uvc"] {
            let _ = std::fs::remove_dir_all(tools.join(d));
        }
    }
    Ok(())
}

// ---------- the Python trackers ----------

/// The environment's python in `dir` (a venv).
fn venv_python(dir: &Path) -> PathBuf {
    if cfg!(windows) {
        dir.join("Scripts").join("python.exe")
    } else {
        dir.join("bin").join("python")
    }
}

/// The installed Python tracker `name` ("vittrack" or "sam2"), if its folder is complete.
pub fn py_tracker(tools: &Path, name: &str) -> Option<comfywebm_engine::pytrack::PyTracker> {
    use comfywebm_engine::pytrack::{Method, PyTracker};
    let method = match name {
        "vittrack" => Method::Vittrack,
        "sam2" => Method::Sam2,
        _ => return None,
    };
    let dir = tools.join(name).join("current");
    let python = venv_python(&dir.join("env"));
    let model = match method {
        Method::Sam2 => std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "pt"))?,
        _ => dir.join("vittrack.onnx"),
    };
    (python.is_file() && model.is_file()).then(|| PyTracker { method, python, script: dir.join("track.py"), model })
}

const YUNET_MODEL: &str = "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx";

/// Track face: OpenCV's YuNet face finder, run in the VitTrack environment (it has OpenCV).
/// The model (230 KB) is fetched on first use when an older install lacks it. None when
/// VitTrack is not installed.
pub async fn face_tracker(app: &AppHandle) -> Result<Option<comfywebm_engine::pytrack::PyTracker>, String> {
    let tools = app.state::<ToolsDir>().0.clone();
    let Some(mut t) = py_tracker(&tools, "vittrack") else { return Ok(None) };
    let model = tools.join("vittrack").join("current").join("yunet.onnx");
    if !model.is_file() {
        let c = client()?;
        let a = Asset { name: "yunet.onnx".into(), url: YUNET_MODEL.into(), size: 0 };
        let part = model.with_extension("part");
        download(app, Tool::Vittrack, &c, &a, &part, &AtomicBool::new(false)).await?;
        std::fs::rename(&part, &model).map_err(|e| e.to_string())?;
    }
    t.method = comfywebm_engine::pytrack::Method::Face;
    t.model = model;
    Ok(Some(t))
}

fn uv_asset() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("uv-x86_64-pc-windows-msvc.zip"),
        ("windows", "aarch64") => Ok("uv-aarch64-pc-windows-msvc.zip"),
        ("macos", "aarch64") => Ok("uv-aarch64-apple-darwin.tar.gz"),
        ("macos", "x86_64") => Ok("uv-x86_64-apple-darwin.tar.gz"),
        ("linux", "x86_64") => Ok("uv-x86_64-unknown-linux-gnu.tar.gz"),
        ("linux", "aarch64") => Ok("uv-aarch64-unknown-linux-gnu.tar.gz"),
        (os, arch) => Err(format!("No uv download for {os} {arch}.")),
    }
}

/// uv in `<tools>/uv/current`, downloaded first when it is not there.
async fn ensure_uv(app: &AppHandle, tool: Tool, c: &reqwest::Client, tools: &Path, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let exe = if cfg!(windows) { "uv.exe" } else { "uv" };
    let dir = tools.join("uv");
    if let Some(p) = find_file(&dir.join("current"), &[exe], 3) {
        return Ok(p);
    }
    let want = uv_asset()?;
    let a = latest_asset(c, "astral-sh/uv", |n| (n == want).then(Vec::new)).await?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("download-{}", a.name));
    if let Err(e) = download(app, tool, c, &a, &file, cancel).await {
        let _ = std::fs::remove_file(&file);
        return Err(e);
    }
    let fresh = dir.join("new");
    let _ = std::fs::remove_dir_all(&fresh);
    let res = unpack(&file, &fresh);
    let _ = std::fs::remove_file(&file);
    res?;
    let _ = std::fs::remove_dir_all(dir.join("current"));
    std::fs::rename(&fresh, dir.join("current")).map_err(|e| e.to_string())?;
    let p = find_file(&dir.join("current"), &[exe], 3).ok_or("The uv download holds no uv program.")?;
    make_executable(&p);
    Ok(p)
}

/// Runs `cmd`, sending each line it writes as "pip" progress; kills it when `cancel` is set.
fn run_streaming(app: &AppHandle, tool: Tool, mut cmd: std::process::Command, what: &str, cancel: &AtomicBool) -> Result<Vec<String>, String> {
    use std::process::Stdio;
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Cannot run {what}: {e}"))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut readers = Vec::new();
    let pipes: [Option<Box<dyn std::io::Read + Send>>; 2] =
        [child.stdout.take().map(|o| Box::new(o) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|e| Box::new(e) as Box<dyn std::io::Read + Send>)];
    for s in pipes.into_iter().flatten() {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            comfywebm_engine::run::each_line(s, |l| {
                if !l.is_empty() {
                    let _ = tx.send(l.to_string());
                }
            })
        }));
    }
    drop(tx);
    let mut lines = Vec::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(l) => {
                emit(app, InstallProgress { tool, phase: "pip", done: 0, total: None, text: Some(l.clone()) });
                lines.push(l);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Canceled".into());
        }
    }
    for r in readers {
        let _ = r.join();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        let why = lines.iter().rev().find(|l| l.contains("error")).or(lines.last()).cloned().unwrap_or_else(|| "no message".into());
        return Err(format!("{what} failed: {why}"));
    }
    Ok(lines)
}

/// Free bytes on the drive that holds `p`.
#[cfg(windows)]
fn free_bytes(p: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
    }
    let w: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: a NUL-terminated wide string and three valid out pointers
    let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then_some(avail)
}
#[cfg(not(windows))]
fn free_bytes(p: &Path) -> Option<u64> {
    let out = std::process::Command::new("df").arg("-Pk").arg(p).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().nth(1)?.split_whitespace().nth(3)?.parse::<u64>().ok().map(|k| k * 1024)
}

/// An NVIDIA card with a driver (nvidia-smi answers): uv then installs PyTorch built for CUDA.
fn has_nvidia() -> bool {
    let mut cmd = comfywebm_engine::tools::command(Path::new("nvidia-smi"));
    cmd.arg("-L");
    comfywebm_engine::tools::output_within(cmd, Duration::from_secs(10), "nvidia-smi").is_ok_and(|o| o.status.success())
}

const VITTRACK_MODEL: &str = "https://github.com/opencv/opencv_zoo/raw/main/models/object_tracking_vittrack/object_tracking_vittrack_2023sep.onnx";
const SAM2_MODELS: &str = "https://dl.fbaipublicfiles.com/segment_anything_2/092824/";

/// Disk space a Python tracker takes, with room to spare (PyTorch for CUDA alone is about 5 GB).
fn space_needed(tool: Tool, nvidia: bool) -> u64 {
    match tool {
        Tool::Sam2 if nvidia => 7 << 30,
        Tool::Sam2 => 2 << 30,
        _ => 600 << 20,
    }
}

async fn python_install(app: &AppHandle, tool: Tool, cancel: Arc<AtomicBool>) -> Result<Installed, String> {
    let tools = app.state::<ToolsDir>().0.clone();
    let root = tools.join(tool.name());
    let nvidia = tool == Tool::Sam2 && tauri::async_runtime::spawn_blocking(has_nvidia).await.unwrap_or(false);
    let need = space_needed(tool, nvidia);
    if let Some(free) = free_bytes(&tools) {
        if free < need {
            let gb = |b: u64| format!("{:.1} GB", b as f64 / (1u64 << 30) as f64);
            return Err(format!("This needs about {} free on the drive of {}, and it has {}.", gb(need), tools.display(), gb(free)));
        }
    }
    let c = client()?;
    let uv = ensure_uv(app, tool, &c, &tools, &cancel).await?;
    let fresh = root.join("new");
    let _ = std::fs::remove_dir_all(&fresh);
    std::fs::create_dir_all(&fresh).map_err(|e| e.to_string())?;
    let fail = |e: String| {
        let _ = std::fs::remove_dir_all(root.join("new"));
        e
    };
    let env = fresh.join("env");
    let python = venv_python(&env);
    let mut pip: Vec<String> = vec!["pip".into(), "install".into(), "--python".into(), python.to_string_lossy().into_owned()];
    match tool {
        Tool::Vittrack => pip.extend(["numpy".into(), "opencv-python>=4.9".into()]),
        _ => pip.extend(["--torch-backend=auto".into(), "torch".into(), "torchvision".into(), "sam2>=1.1".into()]),
    }
    let steps: Vec<(Vec<String>, &str)> =
        vec![(vec!["venv".into(), "--python".into(), "3.12".into(), env.to_string_lossy().into_owned()], "Making the Python environment"), (pip, "Installing the Python packages")];
    for (args, what) in steps {
        let mut cmd = comfywebm_engine::tools::command(&uv);
        cmd.args(&args)
            // a short cache path: one package (antlr4, for SAM 2) is built from source,
            // and Windows refuses paths over 260 characters
            .env("UV_CACHE_DIR", tools.join("uvc"))
            .env("UV_PYTHON_INSTALL_DIR", tools.join("py"))
            .env("UV_PYTHON_PREFERENCE", "only-managed")
            .env("UV_NO_CONFIG", "1")
            .env_remove("VIRTUAL_ENV")
            .env_remove("PYTHONPATH")
            .env_remove("PYTHONHOME");
        let (a, w, cl) = (app.clone(), what.to_string(), cancel.clone());
        tauri::async_runtime::spawn_blocking(move || run_streaming(&a, tool, cmd, &w, &cl)).await.map_err(|e| e.to_string())?.map_err(fail)?;
    }
    emit(app, InstallProgress { tool, phase: "check", done: 0, total: None, text: None });
    // SAM 2.1 small with a GPU; tiny without one (still about 7 s a frame on a laptop CPU)
    let device = if tool == Tool::Sam2 {
        let mut cmd = comfywebm_engine::tools::command(&python);
        cmd.args(["-I", "-c", "import torch; print('cuda' if torch.cuda.is_available() else 'mps' if torch.backends.mps.is_available() else 'cpu', torch.__version__)"]);
        run_ok(cmd, "Checking PyTorch").map_err(fail)?.trim().to_string()
    } else {
        String::new()
    };
    let (url, file, label) = match tool {
        Tool::Vittrack => (VITTRACK_MODEL.to_string(), "vittrack.onnx".to_string(), "VitTrack (OpenCV)".to_string()),
        _ => {
            let (dev, ver) = device.split_once(' ').unwrap_or((device.as_str(), ""));
            let size = if dev == "cpu" { "tiny" } else { "small" };
            let f = format!("sam2.1_hiera_{size}.pt");
            let on = match dev {
                "cuda" => "the NVIDIA graphics card",
                "mps" => "the Apple graphics chip",
                _ => "the processor (no supported graphics card)",
            };
            (format!("{SAM2_MODELS}{f}"), f, format!("SAM 2.1 {size} with PyTorch {ver}, running on {on}"))
        }
    };
    let asset = Asset { name: file.clone(), url, size: 0 };
    download(app, tool, &c, &asset, &fresh.join(&file), &cancel).await.map_err(fail)?;
    if tool == Tool::Vittrack {
        // the face finder for Track face uses the same OpenCV
        let a = Asset { name: "yunet.onnx".into(), url: YUNET_MODEL.into(), size: 0 };
        download(app, tool, &c, &a, &fresh.join("yunet.onnx"), &cancel).await.map_err(fail)?;
    }
    std::fs::write(fresh.join("track.py"), comfywebm_engine::pytrack::SCRIPT).map_err(|e| fail(e.to_string()))?;
    // the packages load
    let mut cmd = comfywebm_engine::tools::command(&python);
    cmd.args(["-I", "-c", if tool == Tool::Vittrack { "import cv2; cv2.TrackerVit_Params()" } else { "import sam2.build_sam" }]);
    run_ok(cmd, "Loading the tracker").map_err(fail)?;
    let current = root.join("current");
    if current.exists() {
        std::fs::remove_dir_all(&current).map_err(|e| fail(format!("The copy installed before is in use (tracking?), so it was not replaced: {e}")))?;
    }
    std::fs::rename(&fresh, &current).map_err(|e| format!("Cannot move the new copy into place: {e}"))?;
    Ok(Installed { tool, path: current.to_string_lossy().into_owned(), release: label })
}

