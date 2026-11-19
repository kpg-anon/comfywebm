//! Runs one job from an EncodeSpec JSON file. Used by scripts/smoke.sh.
//!
//! cargo run -p comfywebm-engine --example spec -- <spec.json> <output> [--cancel-after SECS]
//!
//! Prints `result: <JobResult JSON>` on success. With --cancel-after it cancels
//! the job after SECS seconds, then prints `canceled` and one `leftover: <path>`
//! line for each `.part` file or pass log the job did not delete.

use comfywebm_engine::{probe, run, tools, EncodeSpec};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut pos = Vec::new();
    let mut cancel_after = None;
    while let Some(a) = args.next() {
        if a == "--cancel-after" {
            cancel_after = Some(args.next().and_then(|s| s.parse::<f64>().ok()).ok_or("--cancel-after needs seconds")?);
        } else {
            pos.push(a);
        }
    }
    let [spec_path, output] = <[String; 2]>::try_from(pos).map_err(|_| "usage: spec <spec.json> <output> [--cancel-after SECS]")?;
    let text = std::fs::read_to_string(&spec_path).map_err(|e| format!("{spec_path}: {e}"))?;
    let spec: EncodeSpec = serde_json::from_str(&text).map_err(|e| format!("{spec_path}: {e}"))?;

    let t = tools::find(None, None);
    let st = tools::status(&t);
    let ffmpeg = t.ffmpeg.ok_or("ffmpeg not found")?;
    let ffprobe = t.ffprobe.ok_or("ffprobe not found")?;
    let info = probe::probe(&ffprobe, &spec.input)?;
    // the other files ranges come from (joins, splits), probed as the app does
    let mut sources = std::collections::HashMap::new();
    for p in spec.segment_sources.iter().flatten() {
        if p != &spec.input && !sources.contains_key(p) {
            sources.insert(p.clone(), probe::probe(&ffprobe, p)?);
        }
    }

    // CWM_PRINT_PLAN=1: print each ffmpeg pass (as the planner makes it) before running
    if std::env::var_os("CWM_PRINT_PLAN").is_some() {
        let opts = comfywebm_engine::plan::Opts { zscale: st.filters.zscale, sources: Some(&sources), ..comfywebm_engine::plan::Opts::new(&output, "passlog") };
        let p = comfywebm_engine::plan::build(&spec, &info, &opts)?;
        for (i, pass) in p.passes.iter().enumerate() {
            println!("pass {}: {}", i + 1, pass.join(" "));
        }
    }

    let cancel = Arc::new(run::Cancel::new());
    if let Some(secs) = cancel_after {
        let c = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs_f64(secs));
            c.cancel();
        });
    }
    let out = Path::new(&output);
    // CWM_GYROFLOW=<Gyroflow program>: Stabilize with Gyroflow can run
    let gyro = std::env::var_os("CWM_GYROFLOW").map(|g| comfywebm_engine::gyro::Gyro { gyroflow: g.into(), ffprobe: ffprobe.clone() });
    match run::run_job(&ffmpeg, &spec, &info, out, st.filters.zscale, Some(&sources), gyro.as_ref(), &cancel, |_| {}) {
        Ok(o) => {
            println!("output: {}", o.output.display());
            println!("result: {}", serde_json::to_string(&o.result).unwrap());
            Ok(())
        }
        Err(run::RunError::Canceled) => {
            println!("canceled");
            for p in leftovers(out) {
                println!("leftover: {}", p.display());
            }
            Ok(())
        }
        Err(run::RunError::Failed(e)) => Err(e),
    }
}

/// `.part` files beside the output, and pass logs of this process in the temp dir.
fn leftovers(out: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let stem = out.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = out.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with(&stem) && n.ends_with(".part") {
                found.push(e.path());
            }
        }
    }
    let prefix = format!("comfywebm-{}-", std::process::id());
    if let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with(&prefix) {
                found.push(e.path());
            }
        }
    }
    found
}
