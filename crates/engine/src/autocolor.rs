//! Auto color: white balance, levels and exposure measured on a few frames of the
//! clip, given back as adjustment values (Advanced > Adjustments > Auto) that can be
//! changed afterwards.
//!
//! White balance: the light's colour is estimated with one of the methods in wb.rs
//! (Settings > Tools > Auto color; Combined by default, the best of them measured on
//! real clips with known casts, examples/wb_bench.rs). On clips that needed nothing the
//! methods are 4 to 5 degrees off neutral on average, so a cast within `WB_DEADBAND` is
//! left alone and only the part beyond it is taken out.
//!
//! Levels and exposure: a contrast stretch from the 0.5% darkest and brightest pixels
//! (as auto levels in photo editors) only on a flat picture, and a gamma that pulls a
//! middle light level that is clearly too bright (or very dark) part of the way back.

use crate::spec::{Adjust, MediaInfo};
use crate::tools::{command, output_within};
use crate::wb::{self, Frame, WbMethod};
use serde::Serialize;
use std::path::Path;

/// Width the frames are measured at.
const SAMPLE_W: u32 = 256;
/// How many frames are measured, spread over the ranges.
const SAMPLES: usize = 12;

/// Temperature and tint as log gains: temperature 1 = red e^0.25 over, blue e^0.25 under
/// (red/blue e^0.5 = 1.65x); tint 1 = green e^-0.3 against red and blue e^0.15.
const TEMP_LOG: f64 = 0.25;
const TINT_LOG: f64 = 0.15;

/// Red, green and blue gains for a temperature and tint (both -1..1), scaled so the
/// light level (Rec. 709 weights) stays the same. Used by the encode
/// (colorchannelmixer) and the preview (zoom.ts has the same numbers).
pub fn wb_gains(temperature: f64, tint: f64) -> [f64; 3] {
    let (t, n) = (temperature.clamp(-1.0, 1.0), tint.clamp(-1.0, 1.0));
    let r = (TEMP_LOG * t + TINT_LOG * n).exp();
    let g = (-2.0 * TINT_LOG * n).exp();
    let b = (-TEMP_LOG * t + TINT_LOG * n).exp();
    let k = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    [r / k, g / k, b / k]
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoColor {
    /// temperature, tint, brightness, contrast and gamma set; saturation, hue and curve neutral
    pub adjust: Adjust,
    /// what was found and changed, one sentence each
    pub notes: Vec<String>,
    pub frames: usize,
}

/// Degrees: a cast within this of neutral is left alone (the methods' typical error on
/// clips that needed nothing, wb_bench), and this much is left of a larger one.
pub const WB_DEADBAND: f64 = 3.5;

/// Temperature and tint (the encode's sRGB-domain gains, wb_gains) that divide out a light
/// of colour `e` (linear RGB). A gain g on linear light is about g^(1/2.2) on sRGB values.
pub fn temp_tint_for(e: [f64; 3]) -> (f64, f64) {
    let k = e.map(|v| (1.0 / v.max(1e-6)).powf(1.0 / 2.2).ln());
    let t = (k[0] - k[2]) / (2.0 * TEMP_LOG);
    let n = (k[0] + k[2] - 2.0 * k[1]) / (6.0 * TINT_LOG);
    (t, n)
}

/// Frames (linear RGB, wb.rs), measured with white balance method `method`.
pub fn analyze(frames: &[Frame], method: WbMethod) -> AutoColor {
    let mut a = Adjust::default();
    let mut notes = Vec::new();
    let luma = |p: &[f64; 3]| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
    if frames.iter().all(|f| f.px.is_empty()) {
        return AutoColor { adjust: a, notes: vec!["No frames could be read.".into()], frames: 0 };
    }

    // --- white balance ---
    let e = wb::estimate(method, frames);
    let off = wb::angle(e, [1.0, 1.0, 1.0]);
    if off > WB_DEADBAND {
        // the light, moved toward neutral by the dead band: only the part beyond it is corrected
        let w = [1.0 / 3f64.sqrt(); 3];
        let s = (off - WB_DEADBAND) / off;
        let part = [0, 1, 2].map(|c| (w[c] + (e[c] - w[c]) * s).max(1e-6));
        let (t, n) = temp_tint_for(part);
        a.temperature = round2(t.clamp(-0.8, 0.8));
        a.tint = round2(n.clamp(-0.8, 0.8));
        let kind = if a.temperature < -0.02 {
            "warm or yellow"
        } else if a.temperature > 0.02 {
            "cool or blue"
        } else if a.tint > 0.0 {
            "greenish"
        } else {
            "magenta"
        };
        notes.push(format!(
            "The light was {kind}, {off:.1} degrees from neutral ({}); temperature {:+.2}, tint {:+.2}.",
            method.name(),
            a.temperature,
            a.tint
        ));
    } else {
        notes.push(format!("White balance within {WB_DEADBAND} degrees of neutral ({off:.1}, {}); left as it is.", method.name()));
    }

    // --- levels and exposure, on the picture as balanced (sRGB values) ---
    let g = wb_gains(a.temperature, a.tint);
    let mut ys: Vec<f64> = frames
        .iter()
        .flat_map(|f| f.px.iter())
        .map(|p| {
            let q = p.map(wb::to_srgb);
            luma(&[(q[0] * g[0]).min(1.0), (q[1] * g[1]).min(1.0), (q[2] * g[2]).min(1.0)])
        })
        .collect();
    ys.sort_by(f64::total_cmp);
    let at = |q: f64| ys[((ys.len() - 1) as f64 * q).round() as usize];
    let (lo, med, hi) = (at(0.005), at(0.5), at(0.995));
    let clipped = ys.iter().filter(|y| **y >= 0.98).count() as f64 / ys.len() as f64;
    let (mut c, mut b) = (1.0, 0.0);
    // only a flat picture: most clips already reach near black and white
    if hi - lo < 0.85 && hi > lo {
        // the darkest and brightest half percent to near black and white, around the
        // middle of that span (eq: (v - 0.5) * c + 0.5 + b)
        c = (0.97 / (hi - lo)).clamp(1.0, 1.35);
        b = (-((lo + hi) / 2.0 - 0.5) * c).clamp(-0.12, 0.12);
        if c > 1.005 || b.abs() > 0.005 {
            notes.push(format!("The picture was flat (light levels {:.0}% to {:.0}%); contrast {:.2}, brightness {:+.2}.", lo * 100.0, hi * 100.0, c, b));
        }
    }
    a.contrast = round2(c);
    a.brightness = round2(b);
    let m = ((med - 0.5) * a.contrast + 0.5 + a.brightness).clamp(0.02, 0.98);
    // darken a clearly too-bright picture; brighten only a very dark one (a stage or a night
    // scene is meant to be dark)
    let target = if m > 0.65 || (clipped > 0.08 && m > 0.5) {
        Some(0.5)
    } else if m < 0.15 {
        Some(0.22)
    } else {
        None
    };
    if let Some(t) = target {
        // part of the way, so a bright or dark clip keeps some of its look
        let t = m + (t - m) * 0.6;
        let gm = (m.ln() / t.ln()).clamp(0.7, 1.2);
        if (gm - 1.0).abs() >= 0.03 {
            a.gamma = round2(gm);
            notes.push(if gm < 1.0 {
                format!(
                    "The picture was bright (middle light level {:.0}%{}); gamma {:.2}.",
                    m * 100.0,
                    if clipped > 0.05 { format!(", {:.0}% blown out", clipped * 100.0) } else { String::new() },
                    a.gamma
                )
            } else {
                format!("The picture was very dark (middle light level {:.0}%); gamma {:.2}.", m * 100.0, a.gamma)
            });
        }
    }
    AutoColor { adjust: a, notes, frames: frames.len() }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Times to measure: `SAMPLES` spread over the ranges by their length (the whole file without ranges).
pub fn sample_times(segs: &[(f64, f64)], duration: f64) -> Vec<f64> {
    let segs: Vec<(f64, f64)> = if segs.is_empty() { vec![(0.0, duration)] } else { segs.to_vec() };
    let total: f64 = segs.iter().map(|(s, e)| (e - s).max(0.0)).sum();
    if total <= 0.0 {
        return vec![segs[0].0.max(0.0)];
    }
    (0..SAMPLES)
        .map(|i| {
            let mut at = total * (i as f64 + 0.5) / SAMPLES as f64;
            for (s, e) in &segs {
                let len = (e - s).max(0.0);
                if at <= len {
                    return s + at;
                }
                at -= len;
            }
            segs.last().unwrap().1
        })
        .collect()
}

/// Measures frames of `info` at the ranges `segs` (source seconds) and works out the adjustments.
pub fn auto_color(ffmpeg: &Path, info: &MediaInfo, segs: &[(f64, f64)], method: WbMethod) -> Result<AutoColor, String> {
    let frames = read_frames(ffmpeg, info, &sample_times(segs, info.duration))?;
    Ok(analyze(&frames, method))
}

/// Frames of `info` at `times` (source seconds), `SAMPLE_W` wide, in linear RGB. Read at
/// the same time; a frame that cannot be read is left out.
pub fn read_frames(ffmpeg: &Path, info: &MediaInfo, times: &[f64]) -> Result<Vec<Frame>, String> {
    if info.audio_only {
        return Err("The file has no video stream.".into());
    }
    let frames: Vec<Result<Frame, String>> = std::thread::scope(|sc| {
        let hs: Vec<_> = times
            .iter()
            .map(|&t| {
                sc.spawn(move || {
                    let mut cmd = command(ffmpeg);
                    cmd.args(["-hide_banner", "-nostdin", "-v", "error", "-ss", &crate::plan::fmt_num(t), "-i", &info.path]);
                    cmd.args(["-map", &format!("0:{}", info.video.stream_index), "-frames:v", "1", "-an", "-sn", "-dn"]);
                    cmd.args(["-vf", &format!("scale={SAMPLE_W}:-2,format=rgb24"), "-f", "rawvideo", "-"]);
                    let out = output_within(cmd, std::time::Duration::from_secs(60), "ffmpeg (auto color)")?;
                    Ok(crate::wb::frame_from_rgb24(SAMPLE_W as usize, &out.stdout))
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("auto color failed".into()))).collect()
    });
    let frames: Vec<Frame> = frames.into_iter().filter_map(Result::ok).filter(|f| !f.px.is_empty()).collect();
    if frames.is_empty() {
        return Err("No frames could be read for Auto.".into());
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a frame of colour blocks and grey steps (encoded values), under a light `light`
    /// applied in linear light
    fn frame(light: [f64; 3], lo: f64, hi: f64) -> Frame {
        let (w, h) = (64, 48);
        let colours = [[0.6, 0.2, 0.1], [0.1, 0.5, 0.2], [0.2, 0.3, 0.7], [0.7, 0.6, 0.2]];
        let px = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let base = if (x / 16 + y / 16) % 2 == 0 {
                    let v = lo + (hi - lo) * x as f64 / (w - 1) as f64;
                    [v, v, v].map(wb::to_linear)
                } else {
                    colours[(x / 16 + y / 16 * 4) % 4].map(|v| wb::to_linear(lo + (hi - lo) * v))
                };
                [0, 1, 2].map(|c| (base[c] * light[c]).min(1.0))
            })
            .collect();
        Frame { w, h, px }
    }

    #[test]
    fn gains_keep_the_light_level() {
        for (t, n) in [(0.0, 0.0), (0.5, 0.0), (-0.6, 0.3), (1.0, -1.0)] {
            let g = wb_gains(t, n);
            assert!((0.2126 * g[0] + 0.7152 * g[1] + 0.0722 * g[2] - 1.0).abs() < 1e-9);
        }
        let w = wb_gains(0.5, 0.0);
        assert!(w[0] > w[1] && w[1] > w[2], "warmer: more red, less blue {w:?}");
        assert_eq!(wb_gains(0.0, 0.0), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn temperature_and_tint_divide_out_the_light() {
        // a warm light: cooler; a green one: toward magenta
        let (t, n) = temp_tint_for([1.0, 0.85, 0.6]);
        assert!(t < -0.3, "{t}");
        let (_, n2) = temp_tint_for([0.9, 1.0, 0.9]);
        assert!(n2 > 0.1 && n.abs() < 0.3, "{n2} {n}");
    }

    #[test]
    fn a_yellow_cast_is_cooled() {
        // against the same frame without the cast, so what each method assumes of this
        // made-up scene cancels out
        for m in WbMethod::ALL {
            let y = analyze(&[frame([1.0, 0.9, 0.6], 0.05, 0.95)], m);
            let n = analyze(&[frame([1.0, 1.0, 1.0], 0.05, 0.95)], m);
            assert!(y.adjust.temperature < n.adjust.temperature - 0.12, "{}: {:?} vs {:?}", m.name(), y.adjust, n.adjust);
        }
    }

    #[test]
    fn full_range_is_not_stretched_and_a_flat_one_is() {
        let full = analyze(&[frame([1.0; 3], 0.0, 1.0)], WbMethod::Combined);
        assert_eq!((full.adjust.contrast, full.adjust.brightness), (1.0, 0.0));
        let flat = analyze(&[frame([1.0; 3], 0.3, 0.7)], WbMethod::Combined);
        assert!(flat.adjust.contrast > 1.3, "{:?}", flat.adjust);
    }

    #[test]
    fn samples_spread_over_the_ranges() {
        let t = sample_times(&[(10.0, 12.0), (50.0, 52.0)], 100.0);
        assert_eq!(t.len(), SAMPLES);
        assert!(t.iter().all(|x| (10.0..=12.0).contains(x) || (50.0..=52.0).contains(x)));
        assert_eq!(t.iter().filter(|x| **x < 20.0).count(), SAMPLES / 2);
    }
}
