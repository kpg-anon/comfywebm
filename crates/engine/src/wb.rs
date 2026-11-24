//! White balance: established ways to estimate the colour of the light from the
//! picture (colour constancy), on linear RGB as the methods are defined. Each gives the
//! light's colour as an RGB triple; dividing the picture by it makes the light neutral.
//!
//! - Grey World (Buchsbaum 1980): the scene averages to grey.
//! - Shades of Grey (Finlayson & Trezzi 2004): the Minkowski p-norm (p = 6) of the
//!   pixels is grey; between Grey World (p = 1) and White Patch (p = infinity).
//! - Grey-Edge (van de Weijer, Gevers & Gijsenij 2007): the p-norm (p = 6) of the
//!   edges (first derivative after a Gaussian, sigma 1) is grey. ffmpeg's `greyedge`.
//! - White Patch (Land 1977, robust): the brightest pixels (the 99th percentile of each
//!   channel) are white.
//! - Grey Pixels (Yang, Gao & Li, CVPR 2015): pixels whose local contrast is the same
//!   in the three channels (in log space) are grey; the light is their mean.
//!
//! - Combined: the mean of Grey-Edge, White Patch and Grey Pixels; methods that fail on
//!   different scenes partly cancel out.
//!
//! Pixels with a channel at or near full scale are left out of every estimate: a
//! clipped channel no longer shows the light's colour.

use serde::{Deserialize, Serialize};

/// A frame in linear RGB (0..1), row by row.
#[derive(Debug, Clone)]
pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f64; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WbMethod {
    GreyWorld,
    ShadesOfGrey,
    GreyEdge,
    WhitePatch,
    GreyPixels,
    /// the mean of Grey-Edge, White Patch and Grey Pixels (each as a direction)
    Combined,
}

impl WbMethod {
    pub const ALL: [WbMethod; 6] = [WbMethod::GreyWorld, WbMethod::ShadesOfGrey, WbMethod::GreyEdge, WbMethod::WhitePatch, WbMethod::GreyPixels, WbMethod::Combined];
    pub fn name(self) -> &'static str {
        match self {
            WbMethod::GreyWorld => "Grey World",
            WbMethod::ShadesOfGrey => "Shades of Grey",
            WbMethod::GreyEdge => "Grey-Edge",
            WbMethod::WhitePatch => "White Patch",
            WbMethod::GreyPixels => "Grey Pixels",
            WbMethod::Combined => "Combined",
        }
    }
}

/// sRGB transfer, encoded to linear and back.
pub fn to_linear(v: f64) -> f64 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}
pub fn to_srgb(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// A frame from 8-bit sRGB bytes (rgb24).
pub fn frame_from_rgb24(w: usize, bytes: &[u8]) -> Frame {
    let lut: Vec<f64> = (0..256).map(|i| to_linear(i as f64 / 255.0)).collect();
    let px: Vec<[f64; 3]> = bytes.chunks_exact(3).map(|p| [lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize]]).collect();
    let h = px.len().checked_div(w).unwrap_or(0);
    Frame { w, h, px: px[..w * h].to_vec() }
}

/// Linear level above which a channel counts as clipped (sRGB 250 of 255).
const CLIP: f64 = 0.955;
/// Linear level below which a pixel is too dark to tell its colour (sRGB about 10 of 255).
const DARK: f64 = 0.003;

fn usable(p: &[f64; 3]) -> bool {
    p.iter().all(|v| *v < CLIP) && p.iter().any(|v| *v > DARK)
}

fn norm(e: [f64; 3]) -> [f64; 3] {
    let s = (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt();
    if s > 0.0 && s.is_finite() { e.map(|v| v / s) } else { [1.0 / 3f64.sqrt(); 3] }
}

/// Angle in degrees between two colours (the usual error measure for these methods).
pub fn angle(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (a, b) = (norm(a), norm(b));
    (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0).acos().to_degrees()
}

fn minkowski(frames: &[Frame], p: f64) -> [f64; 3] {
    let mut s = [0.0f64; 3];
    for f in frames {
        for q in f.px.iter().filter(|q| usable(q)) {
            for c in 0..3 {
                s[c] += q[c].powf(p);
            }
        }
    }
    norm(s.map(|v| v.powf(1.0 / p)))
}

/// Gaussian blur of one channel (separable, sigma in pixels).
fn blur(f: &Frame, c: usize, sigma: f64) -> Vec<f64> {
    let r = (sigma * 3.0).ceil() as isize;
    let k: Vec<f64> = (-r..=r).map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp()).collect();
    let ks: f64 = k.iter().sum();
    let (w, h) = (f.w as isize, f.h as isize);
    let at = |v: &Vec<f64>, x: isize, y: isize| v[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize];
    let src: Vec<f64> = f.px.iter().map(|p| p[c]).collect();
    let mut tmp = vec![0.0; src.len()];
    for y in 0..h {
        for x in 0..w {
            tmp[(y * w + x) as usize] = (-r..=r).map(|i| k[(i + r) as usize] * at(&src, x + i, y)).sum::<f64>() / ks;
        }
    }
    let mut out = vec![0.0; src.len()];
    for y in 0..h {
        for x in 0..w {
            out[(y * w + x) as usize] = (-r..=r).map(|i| k[(i + r) as usize] * at(&tmp, x, y + i)).sum::<f64>() / ks;
        }
    }
    out
}

/// Gradient magnitude of a channel (central differences), borders left at 0.
fn grad(v: &[f64], w: usize, h: usize) -> Vec<f64> {
    let mut g = vec![0.0; v.len()];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            let dx = (v[i + 1] - v[i - 1]) / 2.0;
            let dy = (v[i + w] - v[i - w]) / 2.0;
            g[i] = (dx * dx + dy * dy).sqrt();
        }
    }
    g
}

/// pixels next to a clipped or dark one (their edges are not the light's colour)
fn edge_mask(f: &Frame) -> Vec<bool> {
    let (w, h) = (f.w, f.h);
    let bad: Vec<bool> = f.px.iter().map(|p| !usable(p)).collect();
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            if x == 0 || y == 0 || x + 1 >= w || y + 1 >= h {
                return false;
            }
            !(bad[i] || bad[i - 1] || bad[i + 1] || bad[i - w] || bad[i + w])
        })
        .collect()
}

fn grey_edge(frames: &[Frame], p: f64, sigma: f64) -> [f64; 3] {
    let mut s = [0.0f64; 3];
    for f in frames.iter().filter(|f| f.w >= 3 && f.h >= 3) {
        let ok = edge_mask(f);
        for (c, sc) in s.iter_mut().enumerate() {
            let g = grad(&blur(f, c, sigma), f.w, f.h);
            *sc += g.iter().zip(&ok).filter(|(_, ok)| **ok).map(|(v, _)| v.powf(p)).sum::<f64>();
        }
    }
    norm(s.map(|v| v.powf(1.0 / p)))
}

fn white_patch(frames: &[Frame]) -> [f64; 3] {
    let mut e = [0.0f64; 3];
    for c in 0..3 {
        let mut v: Vec<f64> = frames.iter().flat_map(|f| f.px.iter()).filter(|q| usable(q)).map(|q| q[c]).collect();
        if v.is_empty() {
            return norm([1.0; 3]);
        }
        v.sort_by(f64::total_cmp);
        e[c] = v[((v.len() - 1) as f64 * 0.99) as usize];
    }
    norm(e)
}

/// Grey Pixels: the grey index of a pixel is how much its local contrast differs between
/// the channels (relative standard deviation of the gradient of log RGB); the 0.1% of
/// pixels with the lowest index (and some contrast) are averaged.
fn grey_pixels(frames: &[Frame]) -> [f64; 3] {
    let mut cands: Vec<(f64, [f64; 3])> = Vec::new();
    for f in frames.iter().filter(|f| f.w >= 3 && f.h >= 3) {
        let ok = edge_mask(f);
        let logs: Vec<Frame> = vec![Frame { w: f.w, h: f.h, px: f.px.iter().map(|p| p.map(|v| (v.max(1e-4)).ln())).collect() }];
        let d: Vec<Vec<f64>> = (0..3).map(|c| grad(&blur(&logs[0], c, 1.0), f.w, f.h)).collect();
        for i in 0..f.px.len() {
            if !ok[i] {
                continue;
            }
            let v = [d[0][i], d[1][i], d[2][i]];
            let m = (v[0] + v[1] + v[2]) / 3.0;
            // flat areas say nothing (their contrast is noise)
            if m < 0.02 {
                continue;
            }
            let sd = ((v.iter().map(|x| (x - m).powi(2)).sum::<f64>()) / 3.0).sqrt();
            cands.push((sd / m, f.px[i]));
        }
    }
    if cands.is_empty() {
        return grey_world_frames(frames);
    }
    cands.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n = ((cands.len() as f64 * 0.001).ceil() as usize).max(cands.len().min(50));
    let mut s = [0.0f64; 3];
    for (_, p) in &cands[..n] {
        // each pixel counts by its direction, not its brightness
        let q = norm(*p);
        for c in 0..3 {
            s[c] += q[c];
        }
    }
    norm(s)
}

fn grey_world_frames(frames: &[Frame]) -> [f64; 3] {
    minkowski(frames, 1.0)
}

/// The light's colour (linear RGB, unit length) by `method`.
pub fn estimate(method: WbMethod, frames: &[Frame]) -> [f64; 3] {
    match method {
        WbMethod::GreyWorld => minkowski(frames, 1.0),
        WbMethod::ShadesOfGrey => minkowski(frames, 6.0),
        WbMethod::GreyEdge => grey_edge(frames, 6.0, 1.0),
        WbMethod::WhitePatch => white_patch(frames),
        WbMethod::GreyPixels => grey_pixels(frames),
        WbMethod::Combined => {
            let e = [grey_edge(frames, 6.0, 1.0), white_patch(frames), grey_pixels(frames)];
            norm([0, 1, 2].map(|c| e.iter().map(|v| v[c]).sum::<f64>()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a frame with colour blocks and edges under a light of colour `light`
    fn scene(light: [f64; 3]) -> Frame {
        let (w, h) = (64, 48);
        let colours = [[0.6, 0.2, 0.1], [0.1, 0.5, 0.2], [0.2, 0.3, 0.7], [0.5, 0.5, 0.5], [0.8, 0.8, 0.8], [0.3, 0.3, 0.3], [0.7, 0.6, 0.2], [0.15, 0.15, 0.15]];
        let px = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let c = colours[(x / 16 + (y / 16) * 4) % colours.len()];
                [c[0] * light[0], c[1] * light[1], c[2] * light[2]].map(|v| v.min(1.0))
            })
            .collect();
        Frame { w, h, px }
    }

    #[test]
    fn every_method_finds_a_warm_light() {
        let light = [1.0, 0.8, 0.55];
        for m in WbMethod::ALL {
            let neutral = estimate(m, &[scene([1.0, 1.0, 1.0])]);
            let warm = estimate(m, &[scene(light)]);
            // the change between the two is the light's colour (what each method assumes of
            // the scene itself cancels out)
            let found = [warm[0] / neutral[0], warm[1] / neutral[1], warm[2] / neutral[2]];
            assert!(angle(found, light) < 1.5, "{}: {:.2} degrees", m.name(), angle(found, light));
        }
    }

    #[test]
    fn transfer_round_trips() {
        for i in 0..=255 {
            let v = i as f64 / 255.0;
            assert!((to_srgb(to_linear(v)) - v).abs() < 1e-9);
        }
    }
}
