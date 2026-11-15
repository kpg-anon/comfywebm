//! comfywebm engine: finds ffmpeg and yt-dlp, probes files, plans and runs encodes.
//! No Tauri dependencies; the app shell lives in `src-tauri`.

pub mod naming;
pub mod plan;
pub mod probe;
pub mod proxy;
pub mod run;
pub mod spec;
pub mod tools;
pub mod ytdl;

pub use spec::*;
