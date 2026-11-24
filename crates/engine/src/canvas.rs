//! The canvas: an output frame of another shape or size than the picture
//! (spec `canvas`). The picture is fitted into it whole, with black or a
//! blurred copy of itself around it, or (`fill`) scaled to cover it and cut
//! at the edges. Without rotation keyframes this is a scale and a pad or crop
//! where the final scale was; with them, rotation.rs turns the picture into a
//! frame of the canvas's shape.

use crate::plan::fmt_num;
use crate::spec::{Background, Canvas};

fn even(v: f64) -> u32 {
    (((v / 2.0).round() as u32) * 2).max(2)
}

/// The frame at full size for a `pw` x `ph` picture: the canvas's own size
/// (`exact`), or the smallest frame of its shape that holds the picture.
pub fn frame_size(c: &Canvas, pw: u32, ph: u32) -> (u32, u32) {
    if c.exact {
        return (c.w.max(2) / 2 * 2, c.h.max(2) / 2 * 2);
    }
    let r = c.w.max(1) as f64 / c.h.max(1) as f64;
    let (pw, ph) = (pw.max(2) as f64, ph.max(2) as f64);
    if pw / ph > r {
        (even(pw), even(pw / r))
    } else {
        (even(ph * r), even(ph))
    }
}

/// The blurred copy behind a picture: the input (any size) scaled to cover a
/// small box (`blur_box`), blurred, a little darker, and scaled up to the `ow` x `oh`
/// frame. `scaled`: the input already is the small box.
pub fn blur_chain(ow: u32, oh: u32, scaled: bool) -> String {
    let (bw, bh) = (even(ow as f64 / 6.0).max(16), even(oh as f64 / 6.0).max(16));
    let fit = if scaled { String::new() } else { format!("scale={bw}:{bh}:force_original_aspect_ratio=increase,crop={bw}:{bh},") };
    format!(
        "{fit}boxblur=luma_radius='min(w,h)/10':luma_power=2:chroma_radius='min(cw,ch)/10':chroma_power=2,eq=brightness=-0.08,scale={ow}:{oh}:flags=bicubic,setsar=1"
    )
}

/// The small box `blur_chain` works in, for a frame `ow` x `oh`.
pub fn blur_box(ow: u32, oh: u32) -> (u32, u32) {
    (even(ow as f64 / 6.0).max(16), even(oh as f64 / 6.0).max(16))
}

/// A `pw` x `ph` picture put into an `ow` x `oh` frame (the output size), or
/// None when it has the frame's shape (the usual scale does).
pub fn filter(pw: u32, ph: u32, ow: u32, oh: u32, fill: bool, bg: Background) -> Option<String> {
    let (pwf, phf) = (pw.max(2) as f64, ph.max(2) as f64);
    let (owf, ohf) = (ow as f64, oh as f64);
    // the same shape within half a pixel at the output size: an ordinary scale
    let k_fit = (owf / pwf).min(ohf / phf);
    if (pwf * k_fit - owf).abs() < 1.0 && (phf * k_fit - ohf).abs() < 1.0 {
        return None;
    }
    if fill {
        return Some(format!("scale={ow}:{oh}:force_original_aspect_ratio=increase:flags=lanczos,crop={ow}:{oh},setsar=1"));
    }
    let (sw, sh) = (even(pwf * k_fit).min(ow), even(phf * k_fit).min(oh));
    // the picture's corner on even pixels, for the colour planes
    let (x, y) = ((ow - sw) / 4 * 2, (oh - sh) / 4 * 2);
    Some(match bg {
        Background::Black => format!("scale={sw}:{sh}:flags=lanczos,setsar=1,pad={ow}:{oh}:{x}:{y}:black"),
        Background::Blur => format!(
            "split[cvf][cvb];[cvb]{}[cvbg];[cvf]scale={sw}:{sh}:flags=lanczos,setsar=1[cvfg];[cvbg][cvfg]overlay={x}:{y}",
            blur_chain(ow, oh, false)
        ),
    })
}

/// "9:16" and the like, for messages.
pub fn label(c: &Canvas) -> String {
    if c.exact {
        format!("{}x{}", c.w, c.h)
    } else {
        format!("{}:{}", fmt_num(c.w as f64), fmt_num(c.h as f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_sizes() {
        let shape = |w, h| Canvas { w, h, exact: false };
        // a landscape picture in a 9:16 frame: as wide as the picture
        assert_eq!(frame_size(&shape(9, 16), 3840, 2160), (3840, 6826));
        // a portrait picture in 16:9: as tall
        assert_eq!(frame_size(&shape(16, 9), 1080, 1920), (3414, 1920));
        assert_eq!(frame_size(&shape(1, 1), 1080, 1920), (1920, 1920));
        assert_eq!(frame_size(&Canvas { w: 1081, h: 1919, exact: true }, 10, 10), (1080, 1918));
    }

    #[test]
    fn filters() {
        // the same shape: nothing
        assert_eq!(filter(1080, 1920, 540, 960, false, Background::Black), None);
        // landscape into portrait, black bars above and below
        assert_eq!(filter(1920, 1080, 540, 960, false, Background::Black).unwrap(), "scale=540:304:flags=lanczos,setsar=1,pad=540:960:0:328:black");
        assert_eq!(
            filter(1920, 1080, 540, 960, true, Background::Black).unwrap(),
            "scale=540:960:force_original_aspect_ratio=increase:flags=lanczos,crop=540:960,setsar=1"
        );
        let b = filter(1920, 1080, 540, 960, false, Background::Blur).unwrap();
        assert!(b.starts_with("split[cvf][cvb];[cvb]scale=90:160:force_original_aspect_ratio=increase,crop=90:160,boxblur="), "{b}");
        assert!(b.ends_with("[cvbg][cvfg]overlay=0:328"), "{b}");
    }
}
