# comfywebm: usage reference

Everything the README leaves out: each feature in detail, the keys, the
built-in presets, what ffmpeg needs, the files the app keeps, privacy, and
building it yourself. For an overview and the downloads, see
[README.md](README.md).

- [Features](#features)
- [Keys](#keys)
- [Built-in presets](#built-in-presets)
- [Requirements](#requirements)
- [Files](#files)
- [Privacy](#privacy)
- [Questions](#questions)
- [Building from source](#building-from-source)
- [Credits](#credits)

## Features

- **Input**: local files (open, or the first command-line argument; drag and
  drop onto the preview area), and web links through yt-dlp: title, uploader,
  duration and thumbnail before downloading, a quality choice (Best available
  unless another is picked), and an optional "only download part" start and
  end. Dropping or opening several files adds one whole-file job each.
- **Splits**: a tab for combining two or more videos or pictures (PNG,
  JPEG, WebP, BMP, TIFF) into one output. Add them (button or drop), set each
  video's start and end, and reorder them. How to combine: side by side (the
  default), stacked, a grid (as square as possible; an empty place gets the
  gap color), or one after another. A picture beside a video stays on screen
  for the result's whole length; one after another it shows for a length you
  set (5 seconds to start with). The final shape (Auto: the shape that wastes
  the least space; 16:9, 9:16, 1:1, 4:5); for a split screen the shape of
  each part, which sets the final shape (two 9:16 parts side by side make
  9:8). Size: Auto (as large as the size limit allows) or Small, Medium,
  Large, each shown in pixels for the shape and kept within the preset's
  largest size. Fill each space: the whole picture with bars, fill it and cut
  the edges, or the picture over a blurred copy. Split screen also has: sizes
  of the parts (fit each clip, equal, first clip larger, or by hand), sound
  from one clip (the first clip that has sound to start with), all mixed, or
  none, the length of the shortest or the longest video (the others hold
  their last frame, or with "Longest, loop" start again from their start
  until the longest one ends, their sound too), and a gap with its color. The frame rate is the highest
  of the videos (at most 60), within the sidebar's cap.
  - **Crop and zoom** each clip: Crop on its row (or double-click its part in
    the preview) shows the whole picture with a crop rectangle, the aspect
    buttons of the editor plus Part (the shape of the space the clip fills;
    with Auto shapes the space follows the crop, so Part is free), a custom
    ratio, x/y/w/h, and a zoom slider. In the preview, drag a part to move
    its picture and use the wheel to zoom at the pointer.
  - **By hand**: drag the line between two parts (side by side or stacked) to
    change their sizes.
  - **Match**: the Match button on a row makes that clip as long as the other
    one (with three or more clips, a list of them); a video keeps its start
    when the file is long enough. The ends of a video's range snap to the
    other clips' lengths while dragged.
  - **Timeline**: each video's row has a track of the whole file with a
    filmstrip: drag an end to trim, drag the range to move it, click in it to
    show that moment. A seek bar under the preview covers the whole result
    (one after another, with a block per clip). Play (or Space) plays the
    result; Loop (on to start with) plays it again from the start at the end.
  - The preview shows the parts where the engine puts them, with their crops,
    and plays them together. Right-click a part or a row for crop, match,
    move and remove. The sidebar's settings (preset, size limit, format,
    codec, caption, watermark, adjustments, speed) apply to the result; the
    sidebar's crop, rotation, flips, subtitles and cut-only do not.
- **Batch**: a tab for encoding many files with the same settings. Add files,
  a folder (optionally with subfolders) or drop them. Each file is planned with
  the current settings and checked: a picture too poor for the size limit,
  longer than the preset allows, scaled down, a different orientation or size
  than the rest, variable frame rate, interlaced or HDR, no audio track. Most
  checks offer a fix to apply with one click (cap at 30 fps, trim, leave out the
  sound, crop in the editor). Each file can have its own trim and crop (set in
  the editor, then "Save to batch"), frame rate cap and sound switch. "Join
  into one file" joins the included files in list order into one output at
  the first file's picture size and frame rate (other sizes get black bars;
  a file without sound gets silence).
- **Download only**: the Open URL dialog can also just save the original file
  and keep it, without a look-up first. It goes to your Downloads folder (or the
  folder set in Settings) in its own container (MP4, WebM, else MKV).
- **Ranges**: one or more ranges on the timeline, set with `I` / `O` at the
  playhead or by dragging handles; drag a range itself to move it with its
  length kept (a click on it seeks). Several ranges are joined into one file, or
  sent as separate jobs with "Separate files". Play plays the selected range:
  from its start when the playhead is outside it, stopping at its end (with
  Loop, `L`, again from its start). Click past the end while playing to keep
  watching there.
- **Timeline**: thumbnails and the audio waveform behind the ranges (made in
  the background, cached; switch off in Settings). Ctrl+wheel zooms (the wheel
  pans while zoomed), a button zooms to the selected range, with a time ruler,
  an overview of the whole file while zoomed, and the time under the pointer.
  Range edges snap to the playhead, the other ranges and the file's ends (hold
  Alt to place freely). "Find cuts" marks where the picture cuts to a new shot;
  edges snap to those too, and `[` / `]` jump between them. It reads the video
  once (the button shows how far) and finds cuts the way PySceneDetect's
  adaptive detector does: a frame whose change stands out from the frames
  around it, so camera moves and light shows are not cuts; a flash or strobe
  (the picture comes back within 0.4 s) is not one either; a fade through
  black is one cut in its middle; shots are at least 0.5 s. Fewer cuts /
  Normal / More cuts beside the button changes the result at once. Undo and redo (Ctrl+Z, Ctrl+Shift+Z) cover ranges,
  their order, the crop, rotation, flips and adjustments. Reopening a file brings back its ranges and crop.
- **Edit strip**: with two or more ranges, a strip under the timeline shows the
  joined output in order. Drag a block to change the order, click it to jump to
  it, and "Play edit" plays the ranges one after another.
- **Crop**: drawn on the picture with 8 handles, moved by dragging or with the
  arrow keys, numeric x/y/w/h. Aspect lock: free, 1:1, 16:9, 9:16, 4:3, 4:5,
  a "More" list (3:4, 2:3, 3:2, 5:4, 2:1, 1.85:1, 2.39:1, 21:9, 32:9, 9:21, 1:2)
  and a custom ratio such as `5:3` or `1.66`. "Find bars" in the Video group crops black
  bars automatically.
- **Output line**: above Encode (in Splits also beside its own Encode): the
  picture size, frame rate, sound or no sound, length and file size. In size
  limit mode the size is "up to" the bitrate's size (a simple picture comes out
  smaller); in quality mode it is known only after encoding, or "up to" the
  bitrate cap. The unit follows the size limit: a whole number of decimal MB
  (Discord's 20 MB = 20,000,000 bytes) shows MB, anything else MiB (4chan's
  4 MiB). Hover the line for the bitrates, bits per pixel and file name.
- **Size limit mode**: the video bitrate comes from the limit, the clip length
  and the audio bitrate, with two passes. A file over the limit, or under 98% of
  it, is encoded again at a bitrate aimed at 99% of the limit (interpolated
  between earlier attempts when one was over and one under). At most 4
  attempts; the largest result that fits is kept.
- **Resolution**: Auto (in size limit mode, the largest size whose bitrate still
  gives a clean picture), Source, Steps (output heights such as 720p, or
  widths), a percentage, or one exact edge with the other following the aspect
  ratio. In size limit mode each choice shows a good / fair / poor dot; the
  output line's tooltip shows bits per pixel. The step and percentage lists are in
  `config.toml`. A preset's largest picture size (4chan's 2048x2048) is shown
  under the resolution and can be removed.
- **Motion**: frame rate cap and speed (0.25x to 4x) are checked against your
  display's refresh rate (Settings, default 60 Hz). Speeds whose output plays
  evenly are listed and the speed slider snaps to them; a setting that judders
  or stutters is explained, with the nearest smooth speeds and caps as buttons.
- **Speed ramp**: Speed under the timeline shows a lane where the speed changes
  over the clip (0.25x to 4x, up is faster, 1x in the middle). Click to add a
  point, drag it (time and speed), double-click to remove it; right-click a
  point for 1/4x, 1/2x, 1x, 2x, 4x or "Move to the playhead". Between two
  points the speed changes evenly; before the first and after the last it
  stays at that point's speed. Speeds snap to common ones and times to range
  edges and the playhead (hold Alt for neither). With points, the ramp
  replaces the Speed setting; the preview plays at the ramp's speed, the sound
  follows it (pitch kept), and the output length, size and fades use it. The
  ramp is part of the file's edit (undo, and it comes back when the file is
  opened again). It is used for the editor's file, not in Batch or Splits.
- **Quality mode**: CRF on the codec's own scale, with an optional bitrate cap.
- **Codecs**: WebM with VP9, VP8 or AV1 (Opus audio, Vorbis with VP8); MP4 with
  H.264, HEVC or AV1 (AAC audio); GIF (one palette per clip, no sound). GIF has
  no bitrate setting, so in size limit mode the picture size is chosen to fit;
  animated WebP (libwebp, no sound) works the same way, with a quality setting
  in quality mode.
- **Audio only**: Format Audio saves the sound of the ranges as Opus, MP3 or
  AAC (.m4a), with speed, reverse, fades, volume and normalization. In size
  limit mode the bitrate is worked out from the limit.
- **Sound files**: an MP3, M4A, FLAC or other sound file opens like a video.
  Format Audio cuts it; a video format makes a video of the sound with a still
  picture (Basic > Picture, or the file's embedded cover).
- **Cut only (no re-encode)**: copies one range's video and audio as they are
  into WebM or MP4, when the source's codecs fit that container. Fast and
  lossless, but the clip starts on the key frame at or before the in point, and
  crop, resolution, speed and filters do nothing.
- **Size field**: a number is MiB; `10 MB` means 10,000,000 bytes, and KB, GB,
  KiB, GiB work too. Up and Down change it by 0.5 MiB.
- **Filters**: deinterlace, denoise, stabilize, interpolation (new
  in-between frames below 1x speed), HDR to SDR tone mapping, rotate, flip
  horizontal and vertical, reverse, boomerang (forward then reverse), fade in
  and out.
- **Caption**: text drawn on the picture, at the top or bottom edge or
  anywhere (Custom: drag it on the player, or type the position across and
  down in percent), sized as a share of the output height. Font (any installed font, or a .ttf/.otf file), text
  color, outline color and outline width (none, thin, normal, thick); the
  default is white bold sans with a black outline. The look is remembered
  between runs; the text is cleared when another file opens. Shown on the
  player while editing.
- **Watermark**: an image over the picture, sized as a share of the output
  width, with opacity. Placed in a corner, anywhere (Custom: drag it on the
  player; dragging it in corner mode also switches to Custom), or tiled over
  the whole picture with tight, normal, wide or sparse spacing. Rotation
  from -90 to 90 degrees; with Tile the whole grid turns, so the rows run
  diagonally. Remembered between runs and used for batch files too.
- **Adjustments**: brightness, contrast, saturation, gamma, hue, and a master
  curve with draggable points (ffmpeg `eq`, `hue` and `curves`). They show on
  the picture while editing; the preview is close to but not exactly the
  encode, so Sample shows the real result.
- **File info**: a sidebar tab with what is in the open file: container,
  bitrates, codec and profile, frame rate and count, pixel format, color tags,
  audio and subtitle tracks. Copyable as text.
- **Audio**: off, or one track with bitrate, volume in dB and loudness
  normalization. Music: a sound file that replaces the source's sound or is
  mixed under it, with its own level and start point; it is cut to the
  output's length and faded with the clip.
- **Subtitles**: burn in a text subtitle track from the file, or an external
  `.srt` / `.ass` file. Timing follows the cut; a delay moves the subtitles
  later or earlier, and the font size can be set.
- **Preview**: a file the player cannot open gets a preview copy. When its
  streams can be decoded (H.264, VP9 or AV1 with AAC, MP3 or Opus, as in most
  MKV and TS files) they are copied into an MP4 at full resolution in seconds;
  otherwise, or when that copy does not play, a 480p H.264 copy is made.
  Encoding always reads the original.
- **Sample**: encodes 3, 6 or 10 seconds at the playhead with the current settings and
  plays it in a loop, with its size and bitrate; "Compare with the source"
  shows the source next to it.
- **Frame**: saves the frame at the playhead as a full-resolution PNG, with the
  crop applied. Shift+S (or Shift+click) saves it as the encode sees it, with
  the filters, adjustments, rotation and output size.
- **Queue**: its own tab next to the editor (`Q` switches). Jobs run one at a
  time, with progress, pass and attempt, ETA, and for finished jobs the size,
  an over-limit mark, Play, Reveal and Copy path; "Clear finished" empties the
  list. While jobs run or wait, the Queue tab shows a badge with their count
  and the running job's percentage; jobs that finish while the editor is shown
  are counted on the badge (red when one failed) until the queue is opened.
  The taskbar shows the progress of the running job. The list is saved and
  comes back after a restart: a job that was encoding when the app closed is
  marked "Interrupted", and waiting jobs wait for Start. Retry (or "Encode
  again") puts a finished job back in the queue, Edit opens its source in the
  editor with its ranges and crop, waiting jobs move up and down, and "Pause
  after this" stops when the running job ends. Each finished file is read back
  with ffprobe and compared with the plan (video, audio, picture size, length).
  Closing the app stops any running ffmpeg or yt-dlp.
- **Presets**: platform presets (below) and your own, each a commented `.toml`
  file in the config folder that can be edited by hand ("Reload config" reads
  them again). Save, overwrite, rename, delete, restore the platform presets,
  import and export as JSON.
- **Sidebar**: a Basic tab with the everyday settings and an Advanced tab with
  the encoder, filters, audio level, subtitles, metadata, extra arguments and
  the ffmpeg command (copyable). The Advanced tab shows how many of its
  settings are changed.
- **Themes**: Comfy Dark (default), Nord Frost, Gruvbox Retro, Catppuccin
  Mocha and Phosphor. Each changes shapes, fonts, borders and shadows as well as
  colors. Custom: pick 13 colors, corner radius, font and light or dark, start
  from any preset, and import or export the palette as a JSON file to share.
- **Notifications**: finished, over-limit and failed encodes (and saved frames,
  downloads) show in a stack at the top right, newest on top, each fading after
  a few seconds; the x closes one at once.
- **Help**: every setting explains itself on hover.
- The window behaves like an app: no browser context menu (text fields get
  the app's own menu: undo, redo, cut, copy, paste, delete, select all), no
  browser keys such as Find, Print, Reload or zoom,
  hover text in the theme, and the window's size and position are kept between
  runs. The empty state lists recent files. `?` lists every key.
- Output files never overwrite an existing file; a number is added instead.
  Metadata and chapters from the source are dropped.

## Keys

Keys do nothing while a text field has focus.

| key | action |
| --- | --- |
| Space | play / pause (in Splits: the preview) |
| Left / Right | seek 1 s back / forward |
| Shift + Left / Right | seek 5 s |
| `,` / `.` | one frame back / forward (also `<` / `>`; the mouse wheel over the player does the same) |
| `I` / `O` | set the start / end of the range under or nearest the playhead |
| Shift + `I` / `O` | jump to the start / end of the selected range |
| `N` | add a range at the playhead |
| Delete | remove the selected range (when there is more than one) |
| `L` | loop the selected range |
| `M` | mute / unmute the preview (also the speaker at the bottom right of the picture; the wheel over it sets the volume) |
| `C` | crop mode on / off |
| Esc | leave crop mode |
| Arrows (crop mode) | move the crop 1 px; with Shift 10 px |
| Alt + arrows (crop mode) | resize the crop |
| `P` | sample: encode 3 s at the playhead |
| `S` | save the current frame as PNG |
| Ctrl + Z | undo a change to the ranges, their order or the crop |
| Ctrl + Shift + Z, Ctrl + Y | redo |
| Ctrl + wheel over the timeline | zoom the timeline; the wheel alone pans while zoomed |
| `?`, F1 | list of keys |
| `Q` | switch between the editor and the queue |
| `B` | switch between the editor and the batch |
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
| GIF (4 MiB) | GIF | 4,194,304 bytes | none | at most 640x640, 15 fps, auto size | none |

The longest clip is advisory: the app warns when the output is longer.

Formats and codecs the preset's site does not accept are grayed out in the
sidebar, with the reason on hover; the site is read from the preset's name, so
your own presets named "4chan ..." or "X ..." get the same rules. 4chan: WebM
with VP8 or VP9, MP4 with H.264 and GIF are allowed; AV1, HEVC, WebP and
sound-only files are not. X: MP4 with H.264 and GIF. Sources (2026-11-18):
community guides (4chan's own FAQ was not found), which say 4chan refuses AV1
and takes MP4 only with H.264 and AAC; X's MP4 with H.264 and AAC.

## Requirements

- **ffmpeg** and **ffprobe**, with libvpx, libopus and libvorbis for WebM,
  libx264, libx265 and aac for MP4, libsvtav1 for AV1, libass for subtitles and
  zscale (libzimg) for tone mapping. The "full" Windows builds from
  [gyan.dev](https://www.gyan.dev/ffmpeg/builds/) and
  [BtbN](https://github.com/BtbN/FFmpeg-Builds/releases) have all of them.
  Download: <https://ffmpeg.org/download.html>.
- **yt-dlp**, only for web links: <https://github.com/yt-dlp/yt-dlp/releases>.

comfywebm looks for each program in this order: the path set in Settings (a
file or a folder), the folder of the comfywebm executable, then `PATH`, then
on macOS Homebrew's and MacPorts' folders (`/opt/homebrew/bin`,
`/usr/local/bin`, `/opt/local/bin`) and on Linux `/usr/local/bin`,
`/snap/bin` and `~/.local/bin`. When one is missing, the app says which and
how to install it on your system.
Codecs that your ffmpeg lacks are disabled in the codec list.

Extra yt-dlp arguments, for example `--cookies-from-browser firefox`, can be
set in Settings. yt-dlp runs with `--ignore-config`, so your own yt-dlp config
file does not apply.

## Files

The Windows portable copy (a file named `portable` beside `comfywebm.exe`)
keeps all of the below in `data` beside the exe instead: `data/config`,
`data/cache` and `data/webview` (the remembered options, theme and recent
files).

- Settings: `settings.json` in the app config folder (`%APPDATA%\com.comfywebm.app`
  on Windows, `~/.config/com.comfywebm.app` on Linux,
  `~/Library/Application Support/com.comfywebm.app` on macOS).
  Settings > Settings file: Import and Export (one JSON file with the
  settings of all three tabs, including the appearance values that are kept
  in the webview; presets are not in it), Show in folder, Open in editor (the
  program set for .json files, else Notepad on Windows) and Reload (reads
  settings.json again after editing it by hand). An imported file is saved
  at once; your presets and last chosen preset are kept. Settings > Files >
  Recent files turns the start screen's recent files list off (which also
  clears it) or on.
  Settings > Look > Sidebar makes the sidebar resizable by dragging its
  left edge (off by default; off, it has its standard width).
- `config.toml` in the same folder: the resolution steps (heights and widths),
  percentages, frame rate caps and audio bitrates the app offers, and whether
  steps are heights or widths by default. Written once with comments; edit it
  and press "Reload config" (the pencil button next to the preset list).
- `presets/platform/*.toml` and `presets/saved/*.toml`: one preset per file.
  Presets saved in `settings.json` by older builds are moved to
  `presets/saved` on start.
- Downloads opened for editing, preview copies, samples and timeline pictures:
  the app cache folder (`%LOCALAPPDATA%\com.comfywebm.app` on Windows). The
  download folder can be changed in Settings. Settings shows the cache size and
  can clear it; preview copies and timeline pictures over 4 GiB are deleted at
  start, oldest first.
- The queue: `queue.json` in the app config folder.
- "Download only" files: your Downloads folder, or the folder set in Settings.
- Window size and position: `window.json` in the app config folder.
- The theme, the display refresh rate, the sidebar tab, recent files and the
  last option values are kept in the webview's local storage, per machine.
- Output goes to the Desktop by default; the Output group in the sidebar (or
  Settings) switches to beside the source file or another folder. A folder set
  in 1.1.0 or earlier keeps being used. Output names follow the name template in
  Settings; the tokens are `{name}`, `{start}`, `{end}`, `{codec}`, `{height}`
  and `{date}`.

## Privacy

- No account and no analytics. The app's own network use:
  - **Update check**: at start (unless turned off in Settings > Look >
    Updates) and when you press Check now, it downloads `latest.json` from
    this repository's latest GitHub release. GitHub sees your IP address and
    the request; nothing about you, your files or your settings is sent. When
    you press Update now, it downloads the installer from the same release.
  - **yt-dlp** for a link you enter, and the thumbnail the site gives for it
    in the Open URL dialog.
  - The links in the "ffmpeg not found" screen and "Full changelog" in the
    update notice open in your browser.
- "Cookies from browser" in Settings passes `--cookies-from-browser` to
  yt-dlp, which then reads that browser's cookies for the sites you download
  from. comfywebm does not read or keep them, and they are never written into
  presets, themes or the queue.
- Error text shown in the app replaces passwords and sign-in tokens in links
  with `***`, so it can be pasted into a bug report.
- The app's web page (the webview) reads files only from the app's cache
  folder, files you pick in a dialog or drop, and media files the app opens
  (for example from recent files or the queue), and it opens only folders
  and media files in other programs. It still controls settings that run
  programs: the ffmpeg and yt-dlp locations and the extra ffmpeg and yt-dlp
  arguments.
- Settings > Files: Clear deletes preview copies, samples and timeline
  pictures; Clear downloads also deletes the cache's download folder ("Download
  and edit" files, unless the download folder was changed). "Download only"
  files and your outputs are never deleted by the app.
- A settings.json or queue.json the app cannot read is renamed to
  `<name>.bad-<time>` and kept, and the app says so at start.

## Questions

**The preview looks different from the encoded file.** The player shows the
source (or a preview copy). Rotation, flips and adjustments are drawn on top
of it approximately; speed, filters, fades and the encoder's compression are
not shown. Sample (`P`) encodes 3 seconds with the real settings, and
"Compare with the source" shows both.

**Why does an encode run more than once?** In size limit mode the first
attempt rarely lands exactly on the limit. A file over the limit, or under 98%
of it, is encoded again at a corrected bitrate, at most 4 times; the largest
file under the limit is kept. The queue shows the attempt number.

**The file came out over the limit.** After 4 attempts the smallest file is
kept and marked "over limit". Shorten the clip, lower the resolution or frame
rate, turn off the sound, or choose AV1. Retry in the queue runs it again.

**There is no sound.** Check the Audio group: presets for boards without sound
turn it off. Boomerang, GIF and WebP have no sound.

**A download fails.** The Open URL dialog shows a hint for common yt-dlp
errors. Sites that need an account work with "cookies from browser" in
Settings. Update yt-dlp often (`yt-dlp -U`); sites change.

**ffmpeg is not found.** Install ffmpeg (ffmpeg.org/download.html) and either
put it on `PATH`, put `ffmpeg` and `ffprobe` beside the app, or set the folder
in Settings and press "Search again".

**A job was "Interrupted".** The app closed while it was encoding. Retry puts
it back in the queue; it starts from the beginning.

## Building from source

You need Rust (stable), Node.js 20 or newer, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your
platform (WebView2 on Windows, WebKitGTK on Linux).

```sh
npm install
npm run tauri dev                         # development build with hot reload
npx tauri build                           # release build and installers
npx tauri build --debug --no-bundle       # debug executable only, in target/debug
```

Linux also needs the WebKitGTK packages, on Debian and Ubuntu:
`sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev patchelf`.

On Windows, `npx tauri build` writes `target/release/comfywebm.exe` and the
NSIS installer `target/release/bundle/nsis/comfywebm_1.0.7_x64-setup.exe`.
`src-tauri/tauri.windows.conf.json` limits the Windows bundle to NSIS, so no
WiX download is needed; Linux and macOS use the targets in `tauri.conf.json`.

With the `x86_64-pc-windows-gnu` toolchain, the executable loads
`WebView2Loader.dll` at run time. The build copies it to `target/debug` or
`target/release`, and the NSIS installer includes it. Keep it in the same
folder as `comfywebm.exe` when you move the executable by hand.

Releases and in-app updates: see [scripts/README.md](scripts/README.md).

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

## Credits

comfywebm takes ideas from these projects.

- [WebMConverter](https://github.com/nixxquality/WebMConverter): filter options
  in a GUI, showing the ffmpeg command, presets, crop keys.
- [webm.py](https://github.com/Kagami/webm.py): bitrate from the size limit,
  two-pass VP9 with Opus, burned-in subtitles, encoder settings.
- [mpv-webm](https://github.com/ekisu/mpv-webm): marking start and end while
  watching, drawing the crop on the picture, target file size.
