# comfywebm

A desktop app for cutting clips out of a video and encoding them to WebM or MP4,
either under a file size limit or at a constant quality. Open a file or a web
link, mark one or more ranges, optionally draw a crop, pick a preset, encode.

Built with Tauri 2 (Rust) and Svelte 5. Runs on Windows, Linux and macOS.
It uses your own ffmpeg and yt-dlp; neither is bundled.

## Features

- **Input**: local files (open, drag and drop, or the first command-line
  argument), and web links through yt-dlp: title, uploader, duration and
  thumbnail before downloading, a quality choice, and an optional "only download
  part" start and end. Dropping or opening several files adds one whole-file
  job each.
- **Ranges**: one or more ranges on the timeline, set with `I` / `O` at the
  playhead or by dragging handles. Several ranges are joined into one file, or
  sent as separate jobs with "Separate files". Loop playback of the selected range.
- **Crop**: drawn on the picture with 8 handles, moved by dragging or with the
  arrow keys, aspect lock (free, 1:1, 16:9, 9:16, 4:3, 21:9), numeric x/y/w/h.
- **Size limit mode**: the video bitrate comes from the limit, the clip length
  and the audio bitrate, with two passes. If the file comes out over the limit,
  it is encoded again at a lower bitrate; if it comes out well under, at a higher
  one. At most 3 attempts; the largest result that fits is kept. "Auto size"
  lowers the output resolution when the bitrate is too low for the picture
  size, and the sidebar shows bits per pixel with a good / fair / poor rating.
- **Quality mode**: CRF on the codec's own scale, with an optional bitrate cap.
- **Codecs**: WebM with VP9, VP8 or AV1 (Opus audio, Vorbis with VP8); MP4 with
  H.264, HEVC or AV1 (AAC audio).
- **Filters**: deinterlace, denoise, HDR to SDR tone mapping, rotate, mirror,
  speed, reverse, boomerang (forward then reverse), fade in and out, maximum
  size, frame rate cap.
- **Audio**: off, or one track with bitrate, volume in dB and loudness
  normalization.
- **Subtitles**: burn in a text subtitle track from the file, or an external
  `.srt` / `.ass` file. Timing follows the cut.
- **Sample**: encodes 3 seconds at the playhead with the current settings and
  plays it in a loop, with its size and bitrate.
- **Frame**: saves the frame at the playhead as a full-resolution PNG, with the
  crop applied.
- **Queue**: jobs run one at a time, with progress, pass and attempt, ETA, and
  for finished jobs the size, an over-limit mark, Play, Reveal and Copy path.
  The taskbar shows the progress of the running job.
- **Presets**: built-in platform presets (below) and your own: save, overwrite,
  rename, delete, import and export as JSON.
- "Show command" displays the ffmpeg command lines for the current settings.
- Output files never overwrite an existing file; a number is added instead.
  Metadata and chapters from the source are dropped.

## Requirements

- **ffmpeg** and **ffprobe**, with libvpx, libopus and libvorbis for WebM,
  libx264, libx265 and aac for MP4, libsvtav1 for AV1, libass for subtitles and
  zscale (libzimg) for tone mapping. The "full" Windows builds from
  [gyan.dev](https://www.gyan.dev/ffmpeg/builds/) and
  [BtbN](https://github.com/BtbN/FFmpeg-Builds/releases) have all of them.
  Download: <https://ffmpeg.org/download.html>.
- **yt-dlp**, only for web links: <https://github.com/yt-dlp/yt-dlp/releases>.

comfywebm looks for each program in this order: the path set in Settings (a
file or a folder), the folder of the comfywebm executable, then `PATH`. When
one is missing, the app says which and links to the download page.
Codecs that your ffmpeg lacks are disabled in the codec list.

Extra yt-dlp arguments, for example `--cookies-from-browser firefox`, can be
set in Settings. yt-dlp runs with `--ignore-config`, so your own yt-dlp config
file does not apply.

## Build from source

You need Rust (stable), Node.js 20 or newer, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your
platform (WebView2 on Windows, WebKitGTK on Linux).

```sh
npm install
npm run tauri dev                         # development build with hot reload
npx tauri build                           # release build and installers
npx tauri build --debug --no-bundle       # debug executable only, in target/debug
```

On Windows, `npx tauri build` writes `target/release/comfywebm.exe` and the
NSIS installer `target/release/bundle/nsis/comfywebm_1.0.0_x64-setup.exe`.
`src-tauri/tauri.windows.conf.json` limits the Windows bundle to NSIS, so no
WiX download is needed; Linux and macOS use the targets in `tauri.conf.json`.

With the `x86_64-pc-windows-gnu` toolchain, the executable loads
`WebView2Loader.dll` at run time. The build copies it to `target/debug` or
`target/release`, and the NSIS installer includes it. Keep it in the same
folder as `comfywebm.exe` when you move the executable by hand.

Checks:

```sh
npm run check                             # Svelte and TypeScript
cargo test -p comfywebm-engine            # engine unit tests
bash scripts/smoke.sh                     # short real encodes of generated clips
```

`scripts/smoke.sh` generates its test clips with ffmpeg in a temporary folder
and prints one PASS or FAIL line per case (VP8, AV1, HEVC two-pass, H.264 CRF,
reverse, boomerang, audio filters, rotate, subtitles, deinterlace, speed, tone
mapping, cancel).

## Keys

Keys do nothing while a text field has focus.

| key | action |
| --- | --- |
| Space | play / pause |
| Left / Right | seek 1 s back / forward |
| Shift + Left / Right | seek 5 s |
| `,` / `.` | one frame back / forward (the mouse wheel over the player does the same) |
| `I` / `O` | set the start / end of the range under or nearest the playhead |
| Shift + `I` / `O` | jump to the start / end of the selected range |
| `N` | add a range at the playhead |
| Delete | remove the selected range (when there is more than one) |
| `L` | loop the selected range |
| `C` | crop mode on / off |
| Esc | leave crop mode |
| Arrows (crop mode) | move the crop 1 px; with Shift 10 px |
| Alt + arrows (crop mode) | resize the crop |
| `P` | sample: encode 3 s at the playhead |
| `S` | save the current frame as PNG |
| Ctrl + O | open a file |
| Ctrl + U | open a web link |
| Ctrl + Enter | encode |

Pasting a link while no text field has focus opens it in the web link dialog.

## Built-in presets

Limits checked on 2026-11-15. The 4chan numbers are from
`https://a.4cdn.org/boards.json`. Discord's are from its "File Attachments FAQ"
(the free limit is 20 MB since August 2026; whether Discord counts 10^6 or 2^20
bytes per MB was not confirmed, so the presets use 10^6). X's are from the media
best practices page on `docs.x.com`. 4chan's 2048x2048 picture limit and whether
4chan accepts MP4 were not confirmed; the 4chan presets stay WebM.

MiB limits are binary: 1 MiB = 1,048,576 bytes.

| preset | container, codec | limit | audio | picture | longest clip |
| --- | --- | --- | --- | --- | --- |
| 4chan (4 MiB, no sound) | WebM, VP9 | 4,194,304 bytes | none | at most 2048x2048, auto size | 120 s |
| 4chan /b/ /bant/ (2 MiB, no sound) | WebM, VP9 | 2,097,152 bytes | none | at most 2048x2048, auto size | 120 s |
| 4chan /gif/ (4 MiB, sound) | WebM, VP9 | 4,194,304 bytes | Opus 96 kbps | at most 2048x2048, auto size | 300 s |
| 4chan /r/ /wsr/ (4 MiB, sound) | WebM, VP9 | 4,194,304 bytes | Opus 96 kbps | at most 2048x2048, auto size | 120 s |
| 4chan /wsg/ (6 MiB, sound) | WebM, VP9 | 6,291,456 bytes | Opus 96 kbps | at most 2048x2048, auto size | 400 s |
| Discord (20 MB) | MP4, H.264 | 20,000,000 bytes | AAC 128 kbps | auto size | none |
| Discord Nitro Basic (50 MB) | MP4, H.264 | 50,000,000 bytes | AAC 128 kbps | auto size | none |
| X / Twitter | MP4, H.264 | CRF 20 | AAC 128 kbps | at most 1280x1280, 60 fps | 1200 s |
| Quality WebM | WebM, VP9 | CRF 30 | Opus 128 kbps | source | none |
| Quality MP4 | MP4, H.264 | CRF 20 | AAC 160 kbps | source | none |

The longest clip is advisory: the app warns when the output is longer.

## Files

- Settings: `settings.json` in the app config folder (`%APPDATA%\com.comfywebm.app`
  on Windows, `~/.config/com.comfywebm.app` on Linux,
  `~/Library/Application Support/com.comfywebm.app` on macOS).
- Downloads, preview copies and samples: the app cache folder
  (`%LOCALAPPDATA%\com.comfywebm.app` on Windows). The download folder can be
  changed in Settings.
- Output goes beside the source file unless an output folder is set. Output
  names follow the name template in Settings; the tokens are `{name}`,
  `{start}`, `{end}`, `{codec}`, `{height}` and `{date}`.

## Credits

comfywebm takes ideas from these projects.

- [WebMConverter](https://github.com/nixxquality/WebMConverter): filter options
  in a GUI, showing the ffmpeg command, presets, crop keys.
- [webm.py](https://github.com/Kagami/webm.py): bitrate from the size limit,
  two-pass VP9 with Opus, burned-in subtitles, encoder settings.
- [mpv-webm](https://github.com/ekisu/mpv-webm): marking start and end while
  watching, drawing the crop on the picture, target file size.

## License

MIT. See [LICENSE](LICENSE).
