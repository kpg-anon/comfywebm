// What the ffmpeg install button does on this system (src-tauri install.rs):
// BtbN's GPL build on Windows and Linux, Homebrew's ffmpeg-full on macOS.
// The webview tells the system: WebView2 on Windows, WebKit on macOS and Linux.
const mac = typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);

export const ffmpegInstall = mac
  ? { label: "Install ffmpeg with Homebrew", note: "brew install ffmpeg-full (Homebrew's plain ffmpeg has no vid.stab); needs Homebrew" }
  : { label: "Install ffmpeg", note: "BtbN's GPL build from GitHub, with vid.stab 1.4, about 150 to 200 MB" };
