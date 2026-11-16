//! Locating ffmpeg, ffprobe and yt-dlp, asking them what they support, and
//! starting and stopping child processes.

use crate::spec::{Encoders, Filters, ToolsStatus};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub ytdlp: Option<PathBuf>,
}

/// A `Command` for a child process. On Windows it gets CREATE_NO_WINDOW so no
/// console window flashes up from the GUI app. On Unix it starts its own
/// process group, so `kill_tree` can stop it and everything it started.
pub fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.stdin(Stdio::null());
    cmd
}

/// Runs `cmd` like `Command::output`, but kills the process tree when it has
/// not finished after `limit` (a probe of a file on a stalled network share
/// would otherwise block forever).
pub fn output_within(mut cmd: Command, limit: std::time::Duration, what: &str) -> Result<std::process::Output, String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot run {what}: {e}"))?;
    let mut so = child.stdout.take().expect("piped stdout");
    let mut se = child.stderr.take().expect("piped stderr");
    let t_out = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = so.read_to_end(&mut b);
        b
    });
    let t_err = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = se.read_to_end(&mut b);
        b
    });
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() < limit => std::thread::sleep(std::time::Duration::from_millis(25)),
            Ok(None) => {
                kill_tree(&mut child);
                let _ = child.wait();
                return Err(format!("{what} did not finish within {} s.", limit.as_secs()));
            }
            Err(e) => return Err(format!("Waiting for {what} failed: {e}")),
        }
    };
    Ok(std::process::Output { status, stdout: t_out.join().unwrap_or_default(), stderr: t_err.join().unwrap_or_default() })
}

/// Kills `child` and the processes it started. yt-dlp starts ffmpeg, and a
/// scoop shim (`scoop\shims\ffmpeg.exe`) starts the real program, so killing
/// only the direct child can leave the worker running.
pub fn kill_tree(child: &mut Child) {
    let pid = child.id().to_string();
    #[cfg(windows)]
    {
        let _ = command(Path::new("taskkill"))
            .args(["/PID", &pid, "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        // the child leads its own process group (see `command`)
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{pid}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

fn exe_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// Search order: the settings override (a folder, or a file whose folder is
/// also searched for the other tool), then the folder of the running exe,
/// then PATH.
pub fn find(ffmpeg_override: Option<&str>, ytdlp_override: Option<&str>) -> Tools {
    Tools {
        ffmpeg: find_one("ffmpeg", ffmpeg_override),
        ffprobe: find_one("ffprobe", ffmpeg_override),
        ytdlp: find_one("yt-dlp", ytdlp_override),
    }
}

fn find_one(name: &str, override_path: Option<&str>) -> Option<PathBuf> {
    let file = exe_name(name);
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(o) = override_path.map(str::trim).filter(|s| !s.is_empty()) {
        let p = PathBuf::from(o);
        if p.is_file() {
            // A file that is the tool itself is used directly.
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if stem.eq_ignore_ascii_case(name) || name == "yt-dlp" {
                return Some(p);
            }
            if let Some(parent) = p.parent() {
                dirs.push(parent.to_path_buf());
            }
        } else {
            dirs.push(p.join("bin"));
            dirs.push(p);
        }
        // An override that does not contain the tool falls through to the
        // other locations, so a folder with only ffmpeg still finds ffprobe on PATH.
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        dirs.push(dir);
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.into_iter().map(|d| d.join(&file)).find(|c| c.is_file())
}

fn stdout_of(program: &Path, args: &[&str]) -> Option<String> {
    // a tool that hangs (a broken build, a stalled network drive) must not block start-up
    let mut cmd = command(program);
    cmd.args(args);
    let out = output_within(cmd, std::time::Duration::from_secs(20), "tool check").ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs `ffmpeg -version`, `-encoders`, `-filters` and `yt-dlp --version`.
pub fn status(tools: &Tools) -> ToolsStatus {
    let s = |p: &Option<PathBuf>| p.as_ref().map(|p| p.to_string_lossy().into_owned());
    let mut st = ToolsStatus {
        ffmpeg: s(&tools.ffmpeg),
        ffprobe: s(&tools.ffprobe),
        ytdlp: s(&tools.ytdlp),
        ..Default::default()
    };
    if let Some(y) = &tools.ytdlp {
        st.ytdlp_version = stdout_of(y, &["--version"]).and_then(|t| t.lines().next().map(|l| l.trim().to_string()));
    }
    let Some(ffmpeg) = &tools.ffmpeg else {
        return st;
    };
    if let Some(text) = stdout_of(ffmpeg, &["-hide_banner", "-version"]) {
        // first line: "ffmpeg version 8.1.1-full_build-www.gyan.dev Copyright ..."
        st.version = text.lines().next().and_then(|l| l.split_whitespace().nth(2)).map(str::to_string);
    }
    if let Some(text) = stdout_of(ffmpeg, &["-hide_banner", "-encoders"]) {
        st.encoders = parse_encoders(&text);
    }
    if let Some(text) = stdout_of(ffmpeg, &["-hide_banner", "-filters"]) {
        st.filters = parse_filters(&text);
    }
    st
}

/// Encoder lines look like " V....D libvpx-vp9           libvpx VP9 (codec vp9)".
pub fn parse_encoders(text: &str) -> Encoders {
    let mut e = Encoders::default();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(flags), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if flags.len() != 6 {
            continue;
        }
        match name {
            "libvpx" => e.vp8 = true,
            "libvpx-vp9" => e.vp9 = true,
            "libsvtav1" => e.av1 = true,
            "libx264" => e.h264 = true,
            "libx265" => e.hevc = true,
            "libopus" => e.opus = true,
            "libvorbis" => e.vorbis = true,
            "aac" => e.aac = true,
            "gif" => e.gif = true,
            "libwebp_anim" => e.webp = true,
            "libmp3lame" => e.mp3 = true,
            _ => {}
        }
    }
    e
}

/// Filter lines look like " .S zscale            V->V       Apply resizing, ...".
pub fn parse_filters(text: &str) -> Filters {
    let mut f = Filters::default();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(flags), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !(2..=3).contains(&flags.len()) || !flags.chars().all(|c| ".TSC".contains(c)) {
            continue;
        }
        match name {
            "zscale" => f.zscale = true,
            "subtitles" => f.subtitles = true,
            _ => {}
        }
    }
    f
}
