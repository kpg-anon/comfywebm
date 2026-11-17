//! Speed ramps: the speed changes over the clip, set by points on the
//! timeline. Each point is a moment of the source file and a speed; between
//! two points the speed changes in a straight line, before the first and
//! after the last it stays at that point's speed.
//!
//! The ranges are joined first (joined time: the ranges one after another,
//! in output order), so the points are mapped onto joined time per range.
//! Video: `setpts` with the exact output time of every frame (the integral of
//! 1/speed). Sound: `atempo` has no smooth tempo change, so ramped parts are
//! cut into short pieces of one tempo each and joined again; the result is
//! padded or trimmed to the exact length, so sound and picture end together.

use crate::spec::SpeedPoint;

/// The speed range of the ramp (two `atempo` steps would allow more; the
/// timeline shows this range).
pub const MIN_SPEED: f64 = 0.25;
pub const MAX_SPEED: f64 = 4.0;
/// Length of one sound piece where the speed changes.
const AUDIO_STEP: f64 = 0.25;

/// A part of joined time where the speed is `a + b * (t - t0)`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Piece {
    t0: f64,
    len: f64,
    a: f64,
    b: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ramp {
    pieces: Vec<Piece>,
}

/// The points sorted, with a usable time and speed clamped to the range; None when none is left.
pub fn clean(points: &[SpeedPoint]) -> Option<Vec<SpeedPoint>> {
    let mut p: Vec<SpeedPoint> = points
        .iter()
        .filter(|p| p.t.is_finite() && p.speed.is_finite() && p.speed > 0.0)
        .map(|p| SpeedPoint { t: p.t.max(0.0), speed: p.speed.clamp(MIN_SPEED, MAX_SPEED) })
        .collect();
    p.sort_by(|a, b| a.t.total_cmp(&b.t));
    p.dedup_by(|a, b| (a.t - b.t).abs() < 1e-6);
    (!p.is_empty()).then_some(p)
}

/// The speed at file time `t`.
pub fn speed_at(points: &[SpeedPoint], t: f64) -> f64 {
    let (first, last) = (points[0], points[points.len() - 1]);
    if t <= first.t {
        return first.speed;
    }
    if t >= last.t {
        return last.speed;
    }
    for w in points.windows(2) {
        let (p, q) = (w[0], w[1]);
        if t <= q.t {
            let f = if q.t > p.t { (t - p.t) / (q.t - p.t) } else { 1.0 };
            return p.speed + (q.speed - p.speed) * f;
        }
    }
    last.speed
}

/// Output seconds for `x` seconds of a piece starting at speed `a`, changing by `b` per second.
fn out_len(a: f64, b: f64, x: f64) -> f64 {
    if b.abs() < 1e-9 {
        x / a
    } else {
        ((a + b * x) / a).ln() / b
    }
}

impl Ramp {
    /// The ramp over `segs` (file time ranges, in output order). None without points.
    pub fn new(points: &[SpeedPoint], segs: &[(f64, f64)]) -> Option<Ramp> {
        let points = clean(points)?;
        let mut pieces = Vec::new();
        let mut offset = 0.0;
        for &(s, e) in segs {
            // the range's ends and the points inside it
            let mut cuts = vec![s];
            cuts.extend(points.iter().map(|p| p.t).filter(|&t| t > s + 1e-6 && t < e - 1e-6));
            cuts.push(e);
            for w in cuts.windows(2) {
                let (x0, x1) = (w[0], w[1]);
                if x1 - x0 < 1e-9 {
                    continue;
                }
                let a = speed_at(&points, x0);
                let b = (speed_at(&points, x1) - a) / (x1 - x0);
                pieces.push(Piece { t0: offset + x0 - s, len: x1 - x0, a, b });
            }
            offset += e - s;
        }
        Some(Ramp { pieces })
    }

    /// Length of the output.
    pub fn duration(&self) -> f64 {
        self.pieces.iter().map(|p| out_len(p.a, p.b, p.len)).sum()
    }

    /// Output time of joined time `t`.
    pub fn out_at(&self, t: f64) -> f64 {
        self.pieces.iter().map(|p| out_len(p.a, p.b, (t - p.t0).clamp(0.0, p.len))).sum()
    }

    pub fn min_speed(&self) -> f64 {
        self.pieces.iter().map(|p| p.a.min(p.a + p.b * p.len)).fold(f64::INFINITY, f64::min)
    }

    pub fn max_speed(&self) -> f64 {
        self.pieces.iter().map(|p| p.a.max(p.a + p.b * p.len)).fold(0.0, f64::max)
    }

    /// `setpts=...`: every frame at its output time. The expression is a sum of
    /// one term per piece (clip keeps T within the piece); quoted, as it has commas.
    pub fn setpts(&self) -> String {
        let terms: Vec<String> = self
            .pieces
            .iter()
            .map(|p| {
                let x = format!("clip(T-{},0,{})", num(p.t0), num(p.len));
                if p.b.abs() < 1e-9 {
                    format!("{x}/{}", num(p.a))
                } else {
                    format!("log(({}+{}*{x})/{})/{}", num(p.a), num(p.b), num(p.a), num(p.b))
                }
            })
            .collect();
        format!("setpts='({})/TB'", terms.join("+"))
    }

    /// The sound: a filter graph from one unlabeled input to one unlabeled output
    /// (usable as `-af` or after `[label]` in a filter_complex). Parts of one speed are
    /// one piece; where the speed changes, pieces of AUDIO_STEP seconds at the speed
    /// in their middle. `tag` keeps the labels unique.
    pub fn audio(&self, tag: &str) -> String {
        // (start, end, tempo) in joined time
        let mut parts: Vec<(f64, f64, f64)> = Vec::new();
        for p in &self.pieces {
            if p.b.abs() < 1e-9 {
                parts.push((p.t0, p.t0 + p.len, p.a));
                continue;
            }
            let n = (p.len / AUDIO_STEP).ceil().max(1.0) as usize;
            let step = p.len / n as f64;
            for k in 0..n {
                let x0 = k as f64 * step;
                let mid = x0 + step / 2.0;
                parts.push((p.t0 + x0, p.t0 + x0 + step, p.a + p.b * mid));
            }
        }
        // neighbours of the same tempo (constant parts across a range join) become one
        let mut merged: Vec<(f64, f64, f64)> = Vec::new();
        for q in parts {
            match merged.last_mut() {
                Some(m) if (m.2 - q.2).abs() < 1e-9 && (m.1 - q.0).abs() < 1e-9 => m.1 = q.1,
                _ => merged.push(q),
            }
        }
        let n = merged.len();
        let total = num(self.duration());
        let mut g = String::new();
        if n == 1 {
            g.push_str(&tempo_chain(merged[0].2).join(","));
        } else {
            g.push_str(&format!("asplit={n}"));
            for k in 0..n {
                g.push_str(&format!("[{tag}s{k}]"));
            }
            g.push(';');
            for (k, (s, e, tempo)) in merged.iter().enumerate() {
                let mut c = vec![format!("atrim={}:{}", num(*s), num(*e)), "asetpts=PTS-STARTPTS".to_string()];
                c.extend(tempo_chain(*tempo));
                g.push_str(&format!("[{tag}s{k}]{}[{tag}p{k}];", c.join(",")));
            }
            for k in 0..n {
                g.push_str(&format!("[{tag}p{k}]"));
            }
            g.push_str(&format!("concat=n={n}:v=0:a=1"));
        }
        if g.is_empty() {
            g.push_str("anull");
        }
        // the pieces add or lose a few ms each: end exactly where the picture ends
        g.push_str(&format!(",apad,atrim=duration={total}"));
        g
    }
}

/// `atempo` for one tempo (0.5..100 per filter; slower is two filters).
fn tempo_chain(t: f64) -> Vec<String> {
    if (t - 1.0).abs() < 1e-9 {
        return vec![];
    }
    if t < 0.5 {
        let r = t.sqrt();
        return vec![format!("atempo={}", num(r)), format!("atempo={}", num(r))];
    }
    vec![format!("atempo={}", num(t))]
}

/// A number for a filter: at most 6 decimals, no trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(t: f64, speed: f64) -> SpeedPoint {
        SpeedPoint { t, speed }
    }

    /// The same numbers as tests/unit/ramp.test.ts.
    #[test]
    fn ramp_lengths() {
        // 0.5x at 0 s to 2x at 4 s, then 2x: ln(4)/0.375 + 2/2
        let r = Ramp::new(&[pt(4.0, 2.0), pt(0.0, 0.5)], &[(0.0, 6.0)]).unwrap();
        assert!((r.duration() - (4f64.ln() / 0.375 + 1.0)).abs() < 1e-9, "{}", r.duration());
        assert!((r.out_at(4.0) - 4f64.ln() / 0.375).abs() < 1e-9);
        assert_eq!((r.min_speed(), r.max_speed()), (0.5, 2.0));
        // two ranges in output order 10-12 then 2-4 (file time), with points at 3 s (1x) and 11 s (3x):
        // range 10-12 is 3x throughout (after the last point from 11 on, ramping 1->3 between 3 and 11)
        let r = Ramp::new(&[pt(3.0, 1.0), pt(11.0, 3.0)], &[(10.0, 12.0), (2.0, 4.0)]).unwrap();
        // 10-11: 2.75->3 over 1 s; 11-12: 3; 2-3: 1; 3-4: 1->1.25
        let want = (3.0f64 / 2.75).ln() / 0.25 + 1.0 / 3.0 + 1.0 + (1.25f64).ln() / 0.25;
        assert!((r.duration() - want).abs() < 1e-9, "{} vs {want}", r.duration());
        // one point: a constant speed
        let r = Ramp::new(&[pt(5.0, 2.0)], &[(0.0, 4.0)]).unwrap();
        assert!((r.duration() - 2.0).abs() < 1e-9);
        assert!(Ramp::new(&[], &[(0.0, 4.0)]).is_none());
        // speeds outside the range are clamped
        let r = Ramp::new(&[pt(0.0, 0.01)], &[(0.0, 1.0)]).unwrap();
        assert!((r.duration() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn ramp_filters() {
        let r = Ramp::new(&[pt(0.0, 0.5), pt(4.0, 2.0)], &[(0.0, 6.0)]).unwrap();
        assert_eq!(r.setpts(), "setpts='(log((0.5+0.375*clip(T-0,0,4))/0.5)/0.375+clip(T-4,0,2)/2)/TB'");
        let a = r.audio("r");
        // 4 s of ramp in 16 pieces of 0.25 s, then one 2x piece
        assert!(a.starts_with("asplit=17[rs0]"), "{a}");
        assert!(a.contains("[rs0]atrim=0:0.25,asetpts=PTS-STARTPTS,atempo=0.546875[rp0];"), "{a}");
        assert!(a.contains("[rs16]atrim=4:6,asetpts=PTS-STARTPTS,atempo=2[rp16];"), "{a}");
        assert!(a.ends_with("concat=n=17:v=0:a=1,apad,atrim=duration=4.696785"), "{a}");
        // a constant ramp is one filter; below 0.5x two
        let r = Ramp::new(&[pt(0.0, 0.25)], &[(0.0, 1.0)]).unwrap();
        assert_eq!(r.audio("r"), "atempo=0.5,atempo=0.5,apad,atrim=duration=4");
    }
}
