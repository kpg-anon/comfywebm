//! Rotation keyframes: the picture's angle changes over the clip. Before the
//! first keyframe the angle is the Rotate setting; at each keyframe it becomes
//! that keyframe's angle, at once, or (`smooth`) turning from the previous
//! keyframe's angle over the time between them (eased at both ends, the
//! shorter way round). After the last it stays.
//!
//! A WebM keeps one frame size, so the frame has the picture's shape at the
//! first output frame (the nearest quarter turn of its angle). A picture at
//! another angle is fitted into it whole, with black around it, or (`fill`)
//! scaled to cover it, cut at the edges.
//!
//! The encode turns the picture (after crop) on a canvas of the frame's shape
//! with ffmpeg's `rotate`, then `zoompan` cuts the part the frame shows and
//! scales it to the output size. Both run after the ranges are joined, where
//! the final scale was, and take the angle from the source time of each frame.

use crate::spec::{Background, RotatePoint};

fn num(v: f64) -> String {
    crate::plan::fmt_num(v)
}

/// An angle in (-180, 180].
pub fn norm(a: f64) -> f64 {
    let r = a.rem_euclid(360.0);
    if r > 180.0 {
        r - 360.0
    } else {
        r
    }
}

/// Keyframes sorted by time, angles in (-180, 180], no two at the same time. Times
/// before 0 are kept: a job moved onto a part of the file (gyro.rs) still needs them.
pub fn clean(points: &[RotatePoint]) -> Vec<RotatePoint> {
    let mut p: Vec<RotatePoint> = points
        .iter()
        .filter(|p| p.t.is_finite() && p.angle.is_finite())
        .map(|p| RotatePoint { t: p.t, angle: norm(p.angle), smooth: p.smooth })
        .collect();
    p.sort_by(|a, b| a.t.total_cmp(&b.t));
    p.dedup_by(|b, a| (b.t - a.t).abs() < 1e-6);
    p
}

/// Whether the keyframes turn the picture away from the Rotate setting at all.
pub fn active(base: f64, points: &[RotatePoint]) -> bool {
    let b = norm(base);
    clean(points).iter().any(|p| (p.angle - b).abs() > 1e-6)
}

/// The keyframes as (time, angle, smooth), each angle the previous one plus the
/// shorter way to its own, so a smooth turn never goes the long way round.
/// Returns the base angle too.
fn keys(base: f64, points: &[RotatePoint]) -> (f64, Vec<(f64, f64, bool)>) {
    let b = norm(base);
    let mut prev = b;
    let k = clean(points)
        .iter()
        .map(|p| {
            prev += norm(p.angle - prev);
            (p.t, prev, p.smooth)
        })
        .collect();
    (b, k)
}

fn ease(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// The angle in degrees at source time `t` (not wrapped to +-180).
pub fn angle_at(base: f64, points: &[RotatePoint], t: f64) -> f64 {
    let (b, k) = keys(base, points);
    let Some(first) = k.first() else { return b };
    if t < first.0 {
        return b;
    }
    for i in 0..k.len() {
        let next = k.get(i + 1);
        match next {
            Some(n) if t < n.0 => {
                return if n.2 { k[i].1 + (n.1 - k[i].1) * ease((t - k[i].0) / (n.0 - k[i].0).max(1e-6)) } else { k[i].1 };
            }
            Some(_) => {}
            None => return k[i].1,
        }
    }
    b
}

/// The nearest quarter turn of an angle: 0, 90, 180 or 270.
pub fn quarter(a: f64) -> u32 {
    (((a / 90.0).round() as i64).rem_euclid(4) * 90) as u32
}

/// The source time a frame's angle is taken at, for a frame starting at `t`: its
/// end. Keyframes sit in the middle of a frame (where the player shows it), so a
/// keyframe on a frame turns that frame, with half a frame to spare either way.
pub fn frame_time(t: f64, fps: f64) -> f64 {
    if fps > 0.0 {
        t + 1.0 / fps
    } else {
        t
    }
}

/// The quarter turn that gives the frame its shape, for a first output frame starting at source time `t0`.
pub fn start_quarter(base: f64, points: &[RotatePoint], t0: f64, fps: f64) -> u32 {
    quarter(angle_at(base, points, frame_time(t0, fps)))
}

/// The width of the picture area the frame shows at angle `deg` (degrees), in
/// picture pixels, for a `pw` x `ph` picture in a frame `r` wide for 1 high:
/// all of the turned picture (fit), or the largest area inside it (fill).
fn window_w(pw: f64, ph: f64, r: f64, deg: f64, fill: bool) -> f64 {
    let a = deg.to_radians();
    let (c, s) = (a.cos().abs(), a.sin().abs());
    if fill {
        // the largest r:1 box, centred, whose corners stay inside the turned picture
        r * (pw / (r * c + s)).min(ph / (r * s + c))
    } else {
        (pw * c + ph * s).max(r * (pw * s + ph * c))
    }
}

/// The angles the canvas is sized for: the setting, every keyframe and steps through each smooth turn.
fn sample_angles(base: f64, points: &[RotatePoint]) -> Vec<f64> {
    let (b, k) = keys(base, points);
    let mut out = vec![b];
    let mut prev = b;
    for &(_, a, smooth) in &k {
        if smooth {
            out.extend((1..64).map(|i| prev + (a - prev) * i as f64 / 64.0));
        }
        out.push(a);
        prev = a;
    }
    out
}

/// The angle (degrees) at the source time held in ld(0), as an ffmpeg expression. Uses ld(1).
fn angle_expr(base: f64, points: &[RotatePoint]) -> String {
    let (b, k) = keys(base, points);
    // pieces by start time: the setting before the first keyframe, then one per keyframe
    let mut pieces: Vec<(f64, String)> = vec![(f64::NEG_INFINITY, num(b))];
    for (i, &(t, a, _)) in k.iter().enumerate() {
        let e = match k.get(i + 1) {
            Some(&(tn, an, true)) => {
                let span = (tn - t).max(1e-3);
                format!("st(1,(ld(0)-{})/{});{}+({})*ld(1)*ld(1)*(3-2*ld(1))", num(t), num(span), num(a), num(an - a))
            }
            _ => num(a),
        };
        pieces.push((t, e));
    }
    // chosen by halving, as zoom.rs does (ffmpeg's parser stops at about 100 levels)
    fn tree(lo: usize, hi: usize, p: &[(f64, String)]) -> String {
        if hi - lo == 1 {
            return p[lo].1.clone();
        }
        let mid = (lo + hi) / 2;
        format!("if(lt(ld(0),{}),{},{})", num(p[mid].0), tree(lo, mid, p), tree(mid, hi, p))
    }
    tree(0, pieces.len(), &pieces)
}

fn even(v: f64) -> u32 {
    (((v / 2.0).round() as u32) * 2).max(2)
}

/// What `filter` needs besides the keyframes.
pub struct Frame<'a> {
    /// the ranges in output order, (start, end) in source seconds
    pub segs: &'a [(f64, f64)],
    /// the picture after the crop (square pixels)
    pub pic: (u32, u32),
    /// the quarter turn that gives the frame its shape (`start_quarter`), when there is no canvas
    pub quarter: u32,
    /// the frame's shape (width over height) from a canvas; None = the picture's at `quarter`
    pub ratio: Option<f64>,
    /// the output size
    pub out: (u32, u32),
    pub fps: f64,
    /// the source has a variable frame rate: frames are put on `fps`'s grid first
    pub vfr: bool,
    pub mirror: bool,
    pub vflip: bool,
    /// cover the frame instead of showing the whole picture
    pub fill: bool,
    pub background: Background,
}

/// The filters that turn the picture by the keyframes and give the output size,
/// then flip it; None when the keyframes never leave the Rotate setting.
pub fn filter(base: f64, points: &[RotatePoint], f: &Frame) -> Option<String> {
    if !active(base, points) {
        return None;
    }
    let (ow, oh) = (f.out.0.max(2), f.out.1.max(2));
    let mut out = Vec::new();
    let fps = if f.fps > 0.0 { f.fps } else { 30.0 };
    if f.vfr {
        out.push(format!("fps={}", num(fps)));
    }
    if f.fill || f.background == Background::Black {
        out.push(turn(base, points, f, f.fill, (ow, oh), false));
    } else {
        // the picture with clear corners over a small copy that covers the frame, blurred
        let (bw, bh) = crate::canvas::blur_box(ow, oh);
        out.push(format!(
            "split[rtf][rtb];[rtb]{},{}[rtbg];[rtf]{}[rtfg];[rtbg][rtfg]overlay=0:0",
            turn(base, points, f, true, (bw, bh), false),
            crate::canvas::blur_chain(ow, oh, true),
            turn(base, points, f, false, (ow, oh), true)
        ));
    }
    if f.mirror {
        out.push("hflip".into());
    }
    if f.vflip {
        out.push("vflip".into());
    }
    Some(out.join(","))
}

/// The picture turned into an `ow` x `oh` frame: scaled down first when it has more
/// pixels than needed, turned on a frame-shaped canvas (`rotate`), then cut and
/// scaled to the frame (`zoompan`). `clear`: transparent where the picture is not.
fn turn(base: f64, points: &[RotatePoint], f: &Frame, fill: bool, (ow, oh): (u32, u32), clear: bool) -> String {
    let (pw, ph) = (f.pic.0.max(2) as f64, f.pic.1.max(2) as f64);
    let side = f.quarter % 180 == 90;
    // the frame's shape: the canvas's, or the picture's at the quarter turn
    let r = f.ratio.unwrap_or(if side { ph / pw } else { pw / ph });
    let angles = sample_angles(base, points);
    // Scaled down first when even the closest view has more pixels than the
    // output, so the canvas is no larger than it needs to be.
    let closest = angles.iter().map(|&a| window_w(pw, ph, r, a, fill)).fold(f64::INFINITY, f64::min);
    let k = (ow as f64 / closest).min(1.0);
    let odd = f.pic.0 % 2 == 1 || f.pic.1 % 2 == 1;
    let (qw, qh) = if k < 1.0 - 1e-6 || odd { (even(pw * k), even(ph * k)) } else { (f.pic.0, f.pic.1) };
    let (qwf, qhf) = (qw as f64, qh as f64);
    let r = f.ratio.unwrap_or(if side { qhf / qwf } else { qwf / qhf });
    let need = angles.iter().map(|&a| window_w(qwf, qhf, r, a, fill)).fold(0.0, f64::max);
    // The canvas: frame-shaped, and 4n pixels wider and taller than the picture
    // (n may be negative for fill), so the picture sits on whole pixels in the
    // middle, and on whole pixels of the half-size colour planes too. Of the
    // widths from `need` up, the first whose height makes the frame's view of
    // the picture at its quarter turn less than a pixel too tall: zoompan's
    // window is in whole pixels, and the picture then comes through unscaled.
    let step = |from: u32, to: f64| -> u32 { (from as f64 + 4.0 * ((to - from as f64) / 4.0 - 1e-9).ceil()).max(2.0) as u32 };
    let start_w = if side { qhf } else { qwf };
    let size = |n: u32| {
        let cw = step(qw, need) + 4 * n;
        (cw, step(qh, cw as f64 / r))
    };
    let (cw, ch) = (0..64).map(size).find(|&(cw, ch)| (ch as f64 - cw as f64 / r) * start_w / (cw as f64) < 0.99).unwrap_or(size(0));
    let fps = if f.fps > 0.0 { f.fps } else { 30.0 };
    let shift = num(1.0 / fps);
    let angle = angle_expr(base, points);
    let at = |var: &str| format!("st(0,{}+{shift})", crate::zoom::source_time_of(f.segs, var));
    // The zoom at the angle in ld(2) (radians), from the turned picture's
    // bounding box (zooms that make it the window's width, ld(5), and height,
    // ld(6)): fit shows all of it (the smaller), fill the largest area inside
    // it. When the two are within 0.6% the picture has the frame's shape: the
    // smaller if the window is then less than a pixel over (whole pixels drop
    // it), else the larger (a fraction of a pixel cut, no line of black).
    let (bw, bh) = (format!("({qw}*ld(3)+{qh}*ld(4))"), format!("({qw}*ld(4)+{qh}*ld(3))"));
    let other = if fill {
        format!("{ch}/min({qw}/(({cw}/{ch})*ld(3)+ld(4)),{qh}/(({cw}/{ch})*ld(4)+ld(3)))")
    } else {
        "ld(7)".to_string()
    };
    let zoom = format!(
        "st(5,{cw}/{bw});st(6,{ch}/{bh});st(7,min(ld(5),ld(6)));st(8,max(ld(5),ld(6)));if(lt(ld(8),1.006*ld(7)),if(lt(max({cw}/ld(7)-{bw},{ch}/ld(7)-{bh}),0.99),ld(7),ld(8)),{other})"
    );
    let mut out = Vec::new();
    if (qw, qh) != f.pic {
        out.push(format!("scale={qw}:{qh}:flags=lanczos,setsar=1"));
    }
    if clear {
        out.push("format=yuva420p".into());
    }
    out.push(format!("rotate=a='{};({angle})*PI/180':ow={cw}:oh={ch}:c={}", at("t"), if clear { "black@0" } else { "black" }));
    // a hair under the zoom, so zoompan's whole-pixel window is not a pixel short;
    // the window's corner on even pixels, for the colour planes
    out.push(format!(
        "zoompan=z='{};st(2,({angle})*PI/180);st(3,abs(cos(ld(2))));st(4,abs(sin(ld(2))));({zoom})*0.999999':x='2*floor((iw-iw/zoom)/4+0.5)':y='2*floor((ih-ih/zoom)/4+0.5)':d=1:s={ow}x{oh}:fps={},settb=AVTB,setpts=N/{}/TB",
        at("it"),
        num(fps),
        num(fps)
    ));
    out.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(t: f64, angle: f64, smooth: bool) -> RotatePoint {
        RotatePoint { t, angle, smooth }
    }

    #[test]
    fn angles_hold_jump_and_turn() {
        let p = [pt(2.0, 90.0, false), pt(4.0, 0.0, false), pt(5.0, 90.0, true)];
        assert_eq!(angle_at(0.0, &p, 1.0), 0.0, "the setting before the first keyframe");
        assert_eq!(angle_at(0.0, &p, 2.0), 90.0);
        assert_eq!(angle_at(0.0, &p, 3.9), 90.0, "held until the next");
        assert_eq!(angle_at(0.0, &p, 4.0), 0.0, "at once");
        assert!((angle_at(0.0, &p, 4.5) - 45.0).abs() < 1e-9, "half way through a smooth turn");
        assert!(angle_at(0.0, &p, 4.25) < 22.5, "eased");
        assert_eq!(angle_at(0.0, &p, 9.0), 90.0);
        // a smooth first keyframe has nothing to turn from: it is at once
        assert_eq!(angle_at(0.0, &[pt(1.0, 90.0, true)], 1.0), 90.0);
        // the shorter way round: 270 setting (-90) to 0 turns by +90, 170 to -170 by +20
        assert!((angle_at(270.0, &[pt(0.0, -90.0, false), pt(1.0, 0.0, true)], 0.5) + 45.0).abs() < 1e-9);
        assert!((angle_at(170.0, &[pt(0.0, 170.0, false), pt(1.0, -170.0, true)], 1.0) - 190.0).abs() < 1e-9);
        assert_eq!(quarter(190.0), 180);
        assert_eq!(quarter(-90.0), 270);
        assert_eq!(quarter(44.0), 0);
        assert!(!active(90.0, &[pt(1.0, 90.0, false), pt(2.0, -270.0, true)]));
        assert!(active(0.0, &[pt(1.0, 90.0, false)]));
    }

    #[test]
    fn expression_pieces() {
        // the expression, worked out here as ffmpeg would for a few times
        let p = [pt(1.0, 90.0, false), pt(3.0, -90.0, true), pt(6.0, 180.0, false)];
        let e = angle_expr(0.0, &p);
        assert!(e.starts_with("if(lt(ld(0),"), "{e}");
        // 90 to -90 is a half turn: clockwise (+180), so 270 at its end
        assert_eq!(angle_at(0.0, &p, 3.0), 270.0);
        assert!(e.contains("st(1,(ld(0)-1)/2);90+(180)*ld(1)*ld(1)*(3-2*ld(1))"), "{e}");
    }

    #[test]
    fn frame_and_canvas() {
        // a 1080x1920 phone video that turns sideways at 2 s: fit, output 540x960
        let segs = [(0.0, 5.0)];
        let fr = Frame { segs: &segs, pic: (1080, 1920), quarter: 0, ratio: None, out: (540, 960), fps: 30.0, vfr: false, mirror: false, vflip: false, fill: false, background: Background::Black };
        let f = filter(0.0, &[pt(2.0, 90.0, false)], &fr).unwrap();
        // scaled to the output's size first (the closest view is the whole picture at 0)
        assert!(f.starts_with("scale=540:960:flags=lanczos,setsar=1,rotate=a='st(0,t-0+0+0.033);"), "{f}");
        // canvas: 960 wide holds the sideways picture, 4k more than 540; 1708 tall (960/0.5625 = 1706.7, 4k more than 960)
        assert!(f.contains(":ow=960:oh=1708:c=black"), "{f}");
        assert!(f.ends_with(":d=1:s=540x960:fps=30,settb=AVTB,setpts=N/30/TB"), "{f}");
        // nothing to turn: no filter
        assert_eq!(filter(90.0, &[pt(2.0, 90.0, false)], &fr), None);
        // fill: the sideways part is shown 1.78x closer, so the picture is kept at 960x1706;
        // the canvas need not hold the whole turned picture
        let ff = Frame { fill: true, ..fr };
        let f = filter(0.0, &[pt(2.0, 90.0, false)], &ff).unwrap();
        assert!(f.starts_with("scale=960:1706:"), "{f}");
        assert!(f.contains(":ow=960:oh=1706:c=black"), "{f}");
        // flips after the turn
        let fl = Frame { mirror: true, vflip: true, vfr: true, fill: true, ..fr };
        let f = filter(0.0, &[pt(2.0, 90.0, false)], &fl).unwrap();
        assert!(f.starts_with("fps=30,") && f.ends_with(",hflip,vflip"), "{f}");
        // blurred background: a small filled turn behind a clear one
        let bl = Frame { background: Background::Blur, ..fr };
        let f = filter(0.0, &[pt(2.0, 90.0, false)], &bl).unwrap();
        assert!(f.starts_with("split[rtf][rtb];[rtb]"), "{f}");
        assert!(f.contains("s=90x160:"), "{f}");
        assert!(f.contains("format=yuva420p,rotate=") && f.contains(":c=black@0"), "{f}");
        assert!(f.ends_with("[rtbg][rtfg]overlay=0:0"), "{f}");
        // a square canvas: the frame's shape is the canvas's
        let sq = Frame { ratio: Some(1.0), out: (960, 960), ..fr };
        let f = filter(0.0, &[pt(2.0, 90.0, false)], &sq).unwrap();
        assert!(f.contains(":s=960x960:"), "{f}");
    }

    #[test]
    fn window_sizes() {
        let r = 1080.0 / 1920.0;
        assert!((window_w(1080.0, 1920.0, r, 0.0, false) - 1080.0).abs() < 1e-6);
        assert!((window_w(1080.0, 1920.0, r, 90.0, false) - 1920.0).abs() < 1e-6);
        assert!((window_w(1080.0, 1920.0, r, 0.0, true) - 1080.0).abs() < 1e-6);
        assert!((window_w(1080.0, 1920.0, r, 90.0, true) - 1080.0 * r).abs() < 1e-6);
        // a square frame at 45 degrees: fit shows the diagonal, fill the inscribed square
        assert!((window_w(100.0, 100.0, 1.0, 45.0, false) - 141.421356).abs() < 1e-4);
        assert!((window_w(100.0, 100.0, 1.0, 45.0, true) - 70.710678).abs() < 1e-4);
    }
}
