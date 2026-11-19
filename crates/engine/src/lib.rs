//! comfywebm engine: finds ffmpeg and yt-dlp, probes files, plans and runs encodes.
//! No Tauri dependencies; the app shell lives in `src-tauri`.

pub mod config;
pub mod fonts;
pub mod naming;
pub mod plan;
pub mod probe;
pub mod proxy;
pub mod quality;
pub mod ramp;
pub mod run;
pub mod scenes;
pub mod stab;
pub mod spec;
pub mod strip;
pub mod track;
pub mod tools;
pub mod ytdl;
pub mod zoom;

pub use spec::*;
