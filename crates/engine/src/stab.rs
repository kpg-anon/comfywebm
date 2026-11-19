//! Settings > Tools > Stabilization > Test: shakes a still picture by a known
//! amount, steadies it with the chosen method and measures how much of the
//! shake is gone. The measure is the mean difference between neighbouring
//! frames in the middle of the picture (0..255; a still picture scores about
//! 0). A broken stabilizer shows up as little or no reduction: ffmpeg 8.1.1
//! from gyan.dev with vid.stab before 1.4 reduced 9.4 to 3.8 with a picture
//! full of smeared blocks (FFmpeg ticket 22595, avoided by the `copy` in
//! plan.rs), where a working build reached 0.5 on the same clip. With the copy,
//! that old vid.stab took this test clip from 9.9 to 4.8 at smoothing 15; vid.stab
//! 1.4 (2026 builds) to 1.0.

use crate::plan::escape_filter_value;
use crate::spec::StabMethod;
use crate::tools::{command, output_within};
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StabTest {
    /// mean neighbour-frame difference of the shaken clip
    pub before: f64,
    /// the same after stabilizing
    pub after: f64,
    /// seconds the stabilizing took
    pub secs: f64,
}

const LIMIT: Duration = Duration::from_secs(120);

fn ffmpeg_ok(ffmpeg: &Path, args: &[&str], what: &str) -> Result<String, String> {
    let mut cmd = command(ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y"]).args(args);
    let out = output_within(cmd, LIMIT, what)?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{what} failed: {}", err.lines().last().unwrap_or("no message")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Mean difference between neighbouring frames in the middle half of the picture.
fn shake(ffmpeg: &Path, clip: &Path) -> Result<f64, String> {
    let clip = clip.to_string_lossy();
    let text = ffmpeg_ok(
        ffmpeg,
        &["-i", &clip, "-vf", "crop=iw/2:ih/2,format=gray,tblend=all_mode=difference,signalstats,metadata=print:key=lavfi.signalstats.YAVG:file=-", "-f", "null", "-"],
        "Measuring the shake",
    )?;
    let v: Vec<f64> = text.lines().filter_map(|l| l.split_once("YAVG=").and_then(|(_, n)| n.trim().parse().ok())).collect();
    if v.is_empty() {
        return Err("Measuring the shake gave no numbers.".into());
    }
    Ok(v.iter().sum::<f64>() / v.len() as f64)
}

/// Runs the test in `dir` (its files are removed afterwards).
pub fn self_test(ffmpeg: &Path, method: StabMethod, dir: &Path, gyroflow: Option<&Path>) -> Result<StabTest, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let pid = std::process::id();
    let shaken = dir.join(format!("stabtest-{pid}-in.mkv"));
    let steady = dir.join(format!("stabtest-{pid}-out.mkv"));
    let trf = dir.join(format!("stabtest-{pid}.trf"));
    let gyro_out = dir.join(format!("stabtest-{pid}-gyro.mov"));
    let res = (|| {
        // 3 s of one detailed picture, moved and turned like a hand-held camera
        // (up to about 50 px and 1.4 degrees); FFV1 is in every ffmpeg and lossless.
        // yuv420p like a video: the picture is RGB, and vid.stab before 1.4 crashed on packed RGB
        let shake_vf = "mandelbrot=s=1280x720:start_scale=0.8:end_scale=0.8:maxiter=200,trim=end_frame=1,loop=loop=89:size=1,setpts=N/30/TB,\
            rotate='0.025*sin(2*PI*t*2.1)':c=black,\
            crop=960:540:'160+35*sin(2*PI*t*1.3)+15*sin(2*PI*t*5.7+1)':'90+25*sin(2*PI*t*1.7+2)+10*sin(2*PI*t*6.3)',            format=yuv420p";
        let shaken_s = shaken.to_string_lossy().into_owned();
        let steady_s = steady.to_string_lossy().into_owned();
        // Gyroflow reads H.264 better than FFV1; near-lossless is enough for the test
        let codec: &[&str] = if method == StabMethod::Gyroflow { &["-c:v", "libx264", "-preset", "veryfast", "-crf", "8"] } else { &["-c:v", "ffv1"] };
        let mut make = vec!["-f", "lavfi", "-i", shake_vf, "-r", "30"];
        make.extend_from_slice(codec);
        make.push(&shaken_s);
        ffmpeg_ok(ffmpeg, &make, "Making the test clip")?;
        let before = shake(ffmpeg, &shaken)?;
        let t0 = std::time::Instant::now();
        match method {
            StabMethod::Deshake => {
                ffmpeg_ok(ffmpeg, &["-i", &shaken_s, "-vf", "deshake", "-c:v", "ffv1", &steady_s], "deshake")?;
            }
            StabMethod::Vidstab => {
                let t = escape_filter_value(&trf.to_string_lossy());
                ffmpeg_ok(ffmpeg, &["-i", &shaken_s, "-vf", &format!("vidstabdetect=shakiness=6:accuracy=15:result={t}"), "-f", "null", "-"], "vidstabdetect")?;
                // the same transform as an encode (plan.rs), with the copy that avoids ticket 22595
                let vf = format!("copy,vidstabtransform=input={t}:smoothing=30:optzoom=1:crop=black:interpol=bicubic");
                ffmpeg_ok(ffmpeg, &["-i", &shaken_s, "-vf", &vf, "-c:v", "ffv1", &steady_s], "vidstabtransform")?;
            }
            StabMethod::Gyroflow => {
                let g = gyroflow.ok_or("Gyroflow is not installed.")?;
                crate::gyro::run_gyroflow(g, &shaken, &gyro_out, false, &crate::run::Cancel::new(), |_| {}).map_err(|e| match e {
                    crate::run::RunError::Failed(m) => m,
                    crate::run::RunError::Canceled => "Canceled".into(),
                })?;
                let secs = t0.elapsed().as_secs_f64();
                return Ok(StabTest { before, after: shake(ffmpeg, &gyro_out)?, secs });
            }
        }
        let secs = t0.elapsed().as_secs_f64();
        Ok(StabTest { before, after: shake(ffmpeg, &steady)?, secs })
    })();
    for f in [&shaken, &steady, &trf, &gyro_out] {
        let _ = std::fs::remove_file(f);
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs ffmpeg on PATH (skipped without it); vid.stab only when ffmpeg has it.
    #[test]
    fn steadies_the_test_clip() {
        let t = crate::tools::find(None, None);
        let Some(ffmpeg) = t.ffmpeg.clone() else { return };
        let dir = std::env::temp_dir().join(format!("cwm-stabtest-{}", std::process::id()));
        let d = self_test(&ffmpeg, StabMethod::Deshake, &dir, None).unwrap();
        assert!(d.before > 3.0 && d.after < d.before, "{d:?}");
        if crate::tools::status(&t).filters.vidstab {
            let v = self_test(&ffmpeg, StabMethod::Vidstab, &dir, None).unwrap();
            // vid.stab before 1.4 removes about half of it, 1.4 about 90%
            assert!(v.after < v.before * 0.7, "{v:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
