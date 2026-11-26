//! comfywebm engine: finds ffmpeg and yt-dlp, probes files, plans and runs encodes.
//! No Tauri dependencies; the app shell lives in `src-tauri`.

pub mod autocolor;
pub mod canvas;
pub mod wb;
pub mod config;
pub mod gyro;
pub mod fonts;
pub mod naming;
pub mod plan;
pub mod probe;
pub mod proxy;
pub mod pytrack;
pub mod quality;
pub mod ramp;
pub mod rife;
pub mod rotation;
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
