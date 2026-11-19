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
        .map(|p| ZoomPoint { t: p.t.max(0.0), zoom: p.zoom.clamp(MIN_ZOOM, MAX_ZOOM), x: p.x.clamp(0.0, 1.0), y: p.y.clamp(0.0, 1.0) })
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
    let mut off = 0.0;
    let mut parts = Vec::new();
    for (i, (s, e)) in segs.iter().enumerate() {
        let len = e - s;
        let here = format!("it-{}+{}", num(off), num(*s));
        if i + 1 < segs.len() {
            parts.push((num(off + len), here));
        } else {
            parts.push((String::new(), here));
        }
        off += len;
    }
    // if(lt(it,end1), it-off1+s1, if(lt(it,end2), ..., last))
    let mut expr = parts.last().map(|p| p.1.clone()).unwrap_or_else(|| "it".into());
    for (end, here) in parts.iter().rev().skip(1) {
        expr = format!("if(lt(it,{end}),{here},{expr})");
    }
    expr
}

/// One value (zoom, x or y) over the points, of the source time kept in ld(0).
fn value(p: &[ZoomPoint], f: impl Fn(&ZoomPoint) -> f64) -> String {
    if p.len() == 1 {
        return num(f(&p[0]));
    }
    let m = slopes(p, &f);
    let mut expr = num(f(&p[p.len() - 1]));
    for (i, w) in p.windows(2).enumerate().rev() {
        let (a, b) = (&w[0], &w[1]);
        let span = (b.t - a.t).max(1e-6);
        let (c1, c2, c3) = cubic(f(a), f(b), m[i] * span, m[i + 1] * span);
        // u is kept in ld(1)
        let seg = format!("st(1,(ld(0)-{})/{});{}+ld(1)*(({})+ld(1)*(({})+ld(1)*({})))", num(a.t), num(span), num(f(a)), num(c1), num(c2), num(c3));
        expr = format!("if(lte(ld(0),{}),{seg},{expr})", num(b.t));
    }
    format!("if(lte(ld(0),{}),{},{expr})", num(p[0].t), num(f(&p[0])))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(t: f64, zoom: f64, x: f64, y: f64) -> ZoomPoint {
        ZoomPoint { t, zoom, x, y }
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
