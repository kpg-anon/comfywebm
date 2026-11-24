//! Zoom: points over source time, each a zoom (1x = the whole picture) and
//! the centre of what shows (0..1 across and down the picture after crop,
//! rotation and flips). Zoom and centre follow a smooth curve through the
//! points (cubic Hermite, slopes as in `slopes`), so a move starts and stops
//! gently at the first and last point and does not stop at the ones between;
//! before the first and after the last they stay. The encode uses ffmpeg's zoompan, one output
//! frame per input frame, where the final scale was (the caption and the
//! watermark are drawn after it, so they do not zoom). ffmpeg's crop cannot
//! change size per frame, and a scale with eval=frame ahead of it left crop
//! with the first frame's size (the window stuck to the top-left corner).

use crate::spec::ZoomPoint;

pub const MIN_ZOOM: f64 = 1.0;
pub const MAX_ZOOM: f64 = 4.0;

/// Points sorted by time, zoom within MIN..MAX, centre within 0..1, no two at the same time.
pub fn clean(points: &[ZoomPoint]) -> Vec<ZoomPoint> {
    let mut p: Vec<ZoomPoint> = points
        .iter()
        .filter(|p| p.t.is_finite() && p.zoom.is_finite() && p.x.is_finite() && p.y.is_finite())
        .map(|p| ZoomPoint {
            t: p.t.max(0.0),
            zoom: p.zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            x: p.x.clamp(0.0, 1.0),
            y: p.y.clamp(0.0, 1.0),
            rot: if p.rot.is_finite() { p.rot.clamp(-90.0, 90.0) } else { 0.0 },
        })
        .collect();
    p.sort_by(|a, b| a.t.total_cmp(&b.t));
    p.dedup_by(|b, a| (b.t - a.t).abs() < 1e-6);
    p
}

/// The slope of one value at each point: 0 at the first and last and where
/// the value turns back (a held point holds, nothing overshoots), else the
/// mean of the slopes on each side, limited as in Fritsch-Carlson. The
/// preview's zoom.ts does the same.
fn slopes(p: &[ZoomPoint], f: impl Fn(&ZoomPoint) -> f64) -> Vec<f64> {
    let n = p.len();
    let d: Vec<f64> = p.windows(2).map(|w| (f(&w[1]) - f(&w[0])) / (w[1].t - w[0].t).max(1e-6)).collect();
    (0..n)
        .map(|i| {
            if i == 0 || i + 1 == n {
                return 0.0;
            }
            let (a, b) = (d[i - 1], d[i]);
            if a * b <= 0.0 {
                return 0.0;
            }
            let m = (a + b) / 2.0;
            m.signum() * m.abs().min(3.0 * a.abs().min(b.abs()))
        })
        .collect()
}

/// The cubic between two points as `a + u*(c1 + u*(c2 + u*c3))`, u = 0..1.
fn cubic(a: f64, b: f64, ma: f64, mb: f64) -> (f64, f64, f64) {
    (ma, 3.0 * (b - a) - 2.0 * ma - mb, 2.0 * (a - b) + ma + mb)
}

/// Zoom and centre at source time `t` (for tests and checks; the encode uses the expressions).
pub fn at(points: &[ZoomPoint], t: f64) -> (f64, f64, f64) {
    let p = clean(points);
    let Some(first) = p.first() else { return (1.0, 0.5, 0.5) };
    if t <= first.t {
        return (first.zoom, first.x, first.y);
    }
    let (mz, mx, my) = (slopes(&p, |q| q.zoom), slopes(&p, |q| q.x), slopes(&p, |q| q.y));
    for (i, w) in p.windows(2).enumerate() {
        if t <= w[1].t {
            let h = (w[1].t - w[0].t).max(1e-6);
            let u = (t - w[0].t) / h;
            let v = |a: f64, b: f64, m: &[f64]| {
                let (c1, c2, c3) = cubic(a, b, m[i] * h, m[i + 1] * h);
                a + u * (c1 + u * (c2 + u * c3))
            };
            return (v(w[0].zoom, w[1].zoom, &mz), v(w[0].x, w[1].x, &mx), v(w[0].y, w[1].y, &my));
        }
    }
    let last = p[p.len() - 1];
    (last.zoom, last.x, last.y)
}

fn num(v: f64) -> String {
    crate::plan::fmt_num(v)
}

/// The source time from zoompan's `it` (time in the joined output, before
/// speed changes): `segs` are the ranges in output order as (start, end).
fn source_time(segs: &[(f64, f64)]) -> String {
    source_time_of(segs, "it")
}

/// The same from another filter's time variable (crop and rotate call it `t`).
pub(crate) fn source_time_of(segs: &[(f64, f64)], it: &str) -> String {
    let mut off = 0.0;
    let mut parts = Vec::new();
    for (i, (s, e)) in segs.iter().enumerate() {
        let len = e - s;
        let here = format!("{it}-{}+{}", num(off), num(*s));
        if i + 1 < segs.len() {
            parts.push((num(off + len), here));
        } else {
            parts.push((String::new(), here));
        }
        off += len;
    }
    // if(lt(it,end1), it-off1+s1, if(lt(it,end2), ..., last))
    let mut expr = parts.last().map(|p| p.1.clone()).unwrap_or_else(|| it.into());
    for (end, here) in parts.iter().rev().skip(1) {
        expr = format!("if(lt({it},{end}),{here},{expr})");
    }
    expr
}

/// One value (zoom, x or y) over the points, of the source time kept in ld(0).
fn value(p: &[ZoomPoint], f: impl Fn(&ZoomPoint) -> f64) -> String {
    if p.len() == 1 {
        return num(f(&p[0]));
    }
    let m = slopes(p, &f);
    // the curve between point i and i + 1; u is kept in ld(1)
    let seg = |i: usize| {
        let (a, b) = (&p[i], &p[i + 1]);
        let span = (b.t - a.t).max(1e-6);
        let (c1, c2, c3) = cubic(f(a), f(b), m[i] * span, m[i + 1] * span);
        format!("st(1,(ld(0)-{})/{});{}+ld(1)*(({})+ld(1)*(({})+ld(1)*({})))", num(a.t), num(span), num(f(a)), num(c1), num(c2), num(c3))
    };
    // The curves a..b, chosen by halving: ffmpeg's expression parser stops at about
    // 100 levels of nesting ("Missing ')'"), and one if() per point inside the
    // last failed at 162 points (Track face over 18 s). Halving needs log2(n) levels.
    fn tree(a: usize, b: usize, p: &[ZoomPoint], seg: &dyn Fn(usize) -> String) -> String {
        if b - a == 1 {
            return seg(a);
        }
        let mid = (a + b) / 2;
        format!("if(lte(ld(0),{}),{},{})", num(p[mid].t), tree(a, mid, p, seg), tree(mid, b, p, seg))
    }
    let n = p.len();
    format!("if(lte(ld(0),{}),{},if(lte(ld(0),{}),{},{}))", num(p[0].t), num(f(&p[0])), num(p[n - 1].t), tree(0, n - 1, p, &seg), num(f(&p[n - 1])))
}

/// The zoompan filter for `points` at an output of `w` x `h` and `fps`, or
/// None without points (or when every point is the whole picture).
pub fn filter(points: &[ZoomPoint], segs: &[(f64, f64)], w: u32, h: u32, fps: f64) -> Option<String> {
    let p = clean(points);
    if p.is_empty() || p.iter().all(|q| q.zoom <= MIN_ZOOM + 1e-9) {
        return None;
    }
    let t = source_time(segs);
    let z = value(&p, |q| q.zoom);
    let cx = value(&p, |q| q.x);
    let cy = value(&p, |q| q.y);
    let fps = num(if fps > 0.0 { fps } else { 30.0 });
    // The window's corner from its centre, kept inside the picture. zoompan
    // numbers its frames in 1/fps but keeps the input's time base (1/15360 for
    // an MP4): a speed change after it spread 5 s over 5120 s (512 = 15360 / 30),
    // so its frames get their times again.
    Some(format!(
        "zoompan=z='st(0,{t});{z}':x='st(0,{t});clip(({cx})*iw-iw/zoom/2,0,iw-iw/zoom)':y='st(0,{t});clip(({cy})*ih-ih/zoom/2,0,ih-ih/zoom)':d=1:s={w}x{h}:fps={fps},settb=AVTB,setpts=N/{fps}/TB"
    ))
}

/// Follow's view in pixels: `fw` x `fh` (its size at 1x) over the zoom, even, at least 2.
pub fn window_size(fw: f64, fh: f64, zoom: f64) -> (u32, u32) {
    let z = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let even = |v: f64| (((v / z / 2.0).round() as u32) * 2).max(2);
    (even(fw), even(fh))
}

/// The zoom Follow's widest view has: the smallest of the points'.
pub fn widest(points: &[ZoomPoint]) -> Option<f64> {
    clean(points).iter().map(|p| p.zoom).reduce(f64::min)
}

/// Follow's view (`window_size`) moved over a `pw` x `ph` picture and turned:
/// a box large enough to hold the widest view (the smallest zoom) at the
/// path's largest turn is cut out around the path's centre (kept inside the
/// picture), turned by the path's angle around its middle, and the widest view
/// cut from that middle. ffmpeg's crop keeps one size for the whole clip, so
/// when the points' zooms differ, zoompan then zooms in from the middle of the
/// widest view as far as each moment's zoom asks, at the output's `out` size.
/// With one zoom the view is scaled to `out` when that differs (Auto
/// resolution, a size limit). None without points.
pub fn window_filter(points: &[ZoomPoint], segs: &[(f64, f64)], fw: f64, fh: f64, pw: u32, ph: u32, out: (u32, u32), fps: f64) -> Option<String> {
    let p = clean(points);
    let zmin = widest(&p)?;
    let zmax = p.iter().map(|q| q.zoom).fold(zmin, f64::max);
    let (w, h) = window_size(fw, fh, zmin);
    let (w, h) = (w.min(pw.max(2)), h.min(ph.max(2)));
    let turn = p.iter().map(|q| q.rot.abs()).fold(0.0, f64::max).min(90.0).to_radians();
    let (c, s) = (turn.cos(), turn.sin());
    let even = |v: f64| (((v / 2.0).ceil() as u32) * 2).max(2);
    let (bw, bh) = if turn > 1e-6 { (even(w as f64 * c + h as f64 * s).min(pw), even(w as f64 * s + h as f64 * c).min(ph)) } else { (w, h) };
    let t = source_time_of(segs, "t");
    let cx = value(&p, |q| q.x);
    let cy = value(&p, |q| q.y);
    let mut f = format!("crop=w={bw}:h={bh}:x='st(0,{t});clip(({cx})*iw-{bw}/2,0,iw-{bw})':y='st(0,{t});clip(({cy})*ih-{bh}/2,0,ih-{bh})':exact=1");
    if turn > 1e-6 {
        let a = value(&p, |q| q.rot);
        f.push_str(&format!(",rotate=a='st(0,{t});({a})*PI/180':ow={bw}:oh={bh}:c=black,crop={w}:{h}"));
    }
    if zmax > zmin + 1e-6 {
        // The zoom over the widest view's. The window is centred on the path's centre: its
        // offset from the cut box's middle (the box was kept inside the picture), turned as
        // the picture was, from the widest view's middle; kept inside the widest view, its
        // corner on even pixels for the colour planes. The frames get their times again
        // (zoompan counts its own, see `filter`). ld(1) is used inside the curves.
        let it = source_time(segs);
        let z = value(&p, |q| q.zoom);
        let a = if turn > 1e-6 { value(&p, |q| q.rot) } else { "0".into() };
        let fps = num(if fps > 0.0 { fps } else { 30.0 });
        let off = format!(
            "st(0,{it});st(6,({cx})*{pw});st(7,({cy})*{ph});st(8,({a})*PI/180);st(3,ld(6)-clip(ld(6)-{bw}/2,0,{pw}-{bw})-{bw}/2);st(4,ld(7)-clip(ld(7)-{bh}/2,0,{ph}-{bh})-{bh}/2)"
        );
        f.push_str(&format!(
            ",zoompan=z='st(0,{it});({z})/{}':x='{off};2*floor(clip(iw/2+ld(3)*cos(ld(8))-ld(4)*sin(ld(8))-iw/zoom/2,0,iw-iw/zoom)/2+0.5)':y='{off};2*floor(clip(ih/2+ld(3)*sin(ld(8))+ld(4)*cos(ld(8))-ih/zoom/2,0,ih-ih/zoom)/2+0.5)':d=1:s={}x{}:fps={fps},settb=AVTB,setpts=N/{fps}/TB",
            num(zmin),
            out.0,
            out.1
        ));
    } else if out != (w, h) {
        f.push_str(&format!(",scale={}:{}:flags=lanczos", out.0, out.1));
    }
    Some(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(t: f64, zoom: f64, x: f64, y: f64) -> ZoomPoint {
        ZoomPoint { t, zoom, x, y, rot: 0.0 }
    }

    #[test]
    fn many_points_stay_shallow() {
        // 400 points: the deepest nesting of brackets stays far below ffmpeg's ~100 levels
        let p: Vec<ZoomPoint> = (0..400).map(|i| pt(i as f64 * 0.05, 2.0, 0.5 + 0.3 * (i as f64 * 0.1).sin(), 0.5)).collect();
        let e = value(&p, |q| q.x);
        let (mut d, mut most) = (0i32, 0i32);
        for c in e.chars() {
            match c {
                '(' => {
                    d += 1;
                    most = most.max(d);
                }
                ')' => d -= 1,
                _ => {}
            }
        }
        assert_eq!(d, 0);
        assert!(most < 40, "nesting {most}");
    }

    #[test]
    fn window_filter_moves_a_fixed_view_and_turns_it() {
        assert_eq!(window_size(1080.0, 1080.0, 2.0), (540, 540));
        assert_eq!(window_filter(&[], &[(0.0, 5.0)], 1080.0, 1080.0, 1080, 1920, (540, 540), 30.0), None);
        // no turn: one crop of the view's size
        let f = window_filter(&[pt(1.0, 2.0, 0.5, 0.3)], &[(0.0, 5.0)], 1080.0, 1080.0, 1080, 1920, (540, 540), 30.0).unwrap();
        assert_eq!(f, "crop=w=540:h=540:x='st(0,t-0+0);clip((0.5)*iw-540/2,0,iw-540)':y='st(0,t-0+0);clip((0.3)*ih-540/2,0,ih-540)':exact=1");
        // a 45 degree turn: a box sqrt(2) larger, turned, then the view from its middle
        let mut a = pt(1.0, 2.0, 0.5, 0.3);
        a.rot = 45.0;
        let f = window_filter(&[pt(0.0, 2.0, 0.5, 0.3), a], &[(0.0, 5.0)], 1080.0, 1080.0, 1080, 1920, (540, 540), 30.0).unwrap();
        assert!(f.starts_with("crop=w=764:h=764:"), "{f}");
        assert!(f.contains(",rotate=a='st(0,t-0+0);"), "{f}");
        assert!(f.ends_with(":ow=764:oh=764:c=black,crop=540:540"), "{f}");
        // a smaller output: the view is scaled to it
        let f = window_filter(&[pt(1.0, 2.0, 0.5, 0.3)], &[(0.0, 5.0)], 1080.0, 1080.0, 1080, 1920, (360, 360), 30.0).unwrap();
        assert!(f.ends_with(":exact=1,scale=360:360:flags=lanczos"), "{f}");
        // zooms that differ: the widest view (1.5x: 720x720) is cut, then zoompan zooms in from its middle
        let f = window_filter(&[pt(0.0, 1.5, 0.5, 0.5), pt(2.0, 3.0, 0.5, 0.5)], &[(0.0, 5.0)], 1080.0, 1080.0, 1080, 1920, (540, 540), 30.0).unwrap();
        assert!(f.starts_with("crop=w=720:h=720:"), "{f}");
        assert!(f.contains(",zoompan=z='st(0,it-0+0);("), "{f}");
        assert!(f.contains(")/1.5':x='st(0,it-0+0);st(6,"), "{f}");
        assert!(f.ends_with(":d=1:s=540x540:fps=30,settb=AVTB,setpts=N/30/TB"), "{f}");
    }

    #[test]
    fn eases_between_points_and_holds_outside() {
        let p = [pt(2.0, 1.0, 0.5, 0.5), pt(4.0, 3.0, 0.2, 0.8)];
        assert_eq!(at(&p, 0.0), (1.0, 0.5, 0.5));
        assert_eq!(at(&p, 9.0), (3.0, 0.2, 0.8));
        let (z, x, _) = at(&p, 3.0);
        assert!((z - 2.0).abs() < 1e-9 && (x - 0.35).abs() < 1e-9, "{z} {x}");
        // eased: a quarter of the way in time is less than a quarter of the way in zoom
        assert!(at(&p, 2.5).0 < 1.5);
        // a point between two others is passed through without stopping, and a turning point holds
        let p = [pt(0.0, 2.0, 0.0, 0.5), pt(1.0, 2.0, 0.5, 0.5), pt(2.0, 2.0, 1.0, 0.5)];
        assert!((at(&p, 1.0).1 - 0.5).abs() < 1e-9);
        assert!(at(&p, 1.1).1 > 0.55, "moving at the middle point: {}", at(&p, 1.1).1);
        let p = [pt(0.0, 2.0, 0.0, 0.5), pt(1.0, 2.0, 0.8, 0.5), pt(2.0, 2.0, 0.2, 0.5)];
        for k in 0..=20 {
            assert!(at(&p, k as f64 / 10.0).1 <= 0.8 + 1e-9, "no overshoot");
        }
        // out-of-range values are clamped, unsorted points sorted
        assert_eq!(clean(&[pt(5.0, 9.0, 2.0, -1.0), pt(1.0, 0.5, 0.5, 0.5)]), vec![pt(1.0, 1.0, 0.5, 0.5), pt(5.0, 4.0, 1.0, 0.0)]);
    }

    #[test]
    fn source_time_across_ranges() {
        assert_eq!(source_time(&[(10.0, 14.0)]), "it-0+10");
        assert_eq!(source_time(&[(10.0, 14.0), (2.0, 3.5)]), "if(lt(it,4),it-0+10,it-4+2)");
        assert_eq!(source_time(&[(10.0, 14.0), (2.0, 3.0), (20.0, 21.0)]), "if(lt(it,4),it-0+10,if(lt(it,5),it-4+2,it-5+20))");
    }

    #[test]
    fn filter_text() {
        assert_eq!(filter(&[], &[(0.0, 5.0)], 640, 360, 30.0), None);
        assert_eq!(filter(&[pt(1.0, 1.0, 0.3, 0.3)], &[(0.0, 5.0)], 640, 360, 30.0), None, "only the whole picture");
        let f = filter(&[pt(1.0, 2.0, 0.25, 0.5)], &[(0.0, 5.0)], 640, 360, 29.97).unwrap();
        assert_eq!(
            f,
            "zoompan=z='st(0,it-0+0);2':x='st(0,it-0+0);clip((0.25)*iw-iw/zoom/2,0,iw-iw/zoom)':y='st(0,it-0+0);clip((0.5)*ih-ih/zoom/2,0,ih-ih/zoom)':d=1:s=640x360:fps=29.97,settb=AVTB,setpts=N/29.97/TB"
        );
        let f = filter(&[pt(1.0, 1.0, 0.5, 0.5), pt(3.0, 2.0, 0.5, 0.5)], &[(0.0, 5.0)], 640, 360, 30.0).unwrap();
        assert!(f.contains("z='st(0,it-0+0);if(lte(ld(0),1),1,if(lte(ld(0),3),st(1,(ld(0)-1)/2);1+ld(1)*((0)+ld(1)*((3)+ld(1)*(-2))),2))'"), "{f}");
    }
}
