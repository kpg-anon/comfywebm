# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Microsoft Defender reported the Windows installer and the installed app as
  `Trojan:Win32/Bearfoos.A!ml` and removed them. This was a false positive
  from Defender's machine-learning check on the builds made with the GNU
  (MinGW) toolchain. Windows builds are now made with Microsoft's own (MSVC)
  toolchain, which Defender did not flag in testing, and every release is
  scanned with Defender before it is published.

## [1.0.2] - 2026-11-16

### Added

- **Splits takes pictures**: a PNG, JPEG, WebP, BMP or TIFF can be a clip.
  Beside a video it stays on screen for the whole result; one after another
  it shows for a length you set.
- **Crop and zoom in Splits**: each clip can be cropped with the editor's
  aspect buttons, a Part button for the shape of its space, a custom ratio
  and a zoom slider. In the preview, drag a part to move its picture and use
  the wheel to zoom.
- **Sizes by hand**: drag the line between two parts to change their sizes.
- **Match**: a button on each clip makes it as long as another clip.
- **Splits timeline**: each video has a trim track of the whole file with a
  filmstrip and handles that snap to the other clips' lengths, and a seek
  bar covers the whole result.
- **Right-click menu**: text fields have the app's own menu (undo, redo, cut,
  copy, paste, delete, select all) instead of the browser's; Splits parts and
  rows have one for crop, match, move and remove.

### Changed

- Splits starts with Side by side.
- Splits takes its sound from the first clip that has sound, and its frame
  rate from the videos only.

### Fixed

- A picture added to Splits showed nothing, and with a picture in the splits
  Play stopped at once: the picture had a length of 0 (PNG) or 0.04 s (JPEG).

## [1.0.1] - 2026-11-16

### Added

- **Splits**: a tab that combines two or more clips into one output, even
  when they differ in size, shape and frame rate. One after another, or at
  the same time side by side, stacked or in a grid. Plain choices for the
  final shape, the shape of each part, the size (shown in pixels), how each
  clip fills its space (whole with bars, filled, or over a blurred copy), the
  sizes of the parts, where the sound comes from, the length, and a gap. A
  live preview plays the parts together. The size limit, format, caption,
  watermark and the other sidebar settings apply to the result.

## [1.0.0] - 2026-11-16

The first public release.

- **Fits the limit**: size mode works out the bitrate from the limit and the
  length, encodes in two passes and tries again when the file is over.
  Presets for 4chan, Discord and X, plus your own; quality (CRF) mode when
  size does not matter.
- **Edit on the timeline**: several ranges per file, thumbnails and the audio
  waveform, scene cut detection, snapping, undo and redo.
- **Picture**: crop with aspect presets and black bar detection, rotate,
  mirror, speed from 0.25x to 4x with interpolation, reverse and boomerang,
  fades, color adjustments and a curve, HDR to SDR tone mapping.
- **On top**: captions in any installed font, anywhere on the picture; a
  watermark in a corner, anywhere, or tiled and turned; burned-in subtitles.
- **Formats**: WebM (VP9, VP8, AV1), MP4 (H.264, HEVC, AV1), GIF, animated
  WebP, and sound only (Opus, MP3, AAC); cut-only stream copy.
- **Many files**: a batch tab with per-file checks and one-click fixes,
  joining files into one, and a queue that survives restarts.
- **Web links** through yt-dlp, with a quality choice and partial downloads.
- **Downloads for Windows** (installer and portable zip), **macOS** (Apple
  silicon and Intel) and **Linux** (AppImage, .deb, .rpm), with in-app
  updates for the installer, macOS and the AppImage. This is the first
  release on macOS and Linux; the builds are made on GitHub and have not been
  tried much on those systems yet, so please report what does not work.

comfywebm uses your own ffmpeg (and yt-dlp for web links); see the README for
how to install them on each system.

## Development before 1.0.0

Changes after build 1.2.0, made before the first public release. Earlier
builds were numbered 1.0.0 to 1.2.0 before the version went back to 1.0.0;
their notes are kept below under those numbers.

### Added

- In-app updates: at start (Settings > Look > Updates, on by default) and
  with Check now, the app reads latest.json from the latest GitHub release.
  A newer version shows a notice with its changelog; Update now downloads
  the installer, checks its signature, installs it and restarts the app.
- `scripts/release.ps1` and `scripts/bump-version.ps1`: one command builds the
  signed installer, writes latest.json from the CHANGELOG section, tags and
  publishes the GitHub release. See scripts/README.md.
- Caption position Custom: drag the caption on the player, or type the
  position across and down in percent.
- Watermark rotation, -90 to 90 degrees. With Tile the whole grid turns, so
  the rows run diagonally.
- Settings > Files > Recent files: turn the start screen's list off (which
  also clears it) or on, and clear it.
- Settings > Look > Sidebar: resizable by dragging its left edge. Off by
  default.
- Caption font, text color, outline color and outline width (none, thin,
  normal, thick). The font list holds the installed fonts; File picks a
  .ttf/.otf that is not installed. The look is remembered between runs.
- Watermark (was Logo) placement: Custom, by dragging the image on the
  player, and Tile, the image repeated over the whole picture with tight,
  normal, wide or sparse spacing.
- Settings > Settings file: Import, Export, Show in folder, Open in editor
  and Reload.
- Batch: a checkbox in the header selects or deselects all files.
- Drop-down lists jump to the option starting with the letters typed while
  the list is open.
- Hover text on the Forward, Reverse and Boomerang buttons.
- Sound files open (MP3, M4A, Opus, FLAC and others). Format Audio cuts them
  with no picture; a video format makes a video of the sound with a still
  picture: Basic > Picture, or the file's embedded cover. The picture runs at
  about 1 frame per second, with a keyframe every 10 seconds, and in size mode
  a file under the limit is not encoded again to fill it. A long sound that
  would not fit the limit next to the picture gets a lower bitrate, with a
  warning. The room kept for the picture comes from its size (0.9 bytes per
  pixel plus 32 KiB): a 400 s sound at 4 MiB with 96 kbps asked gets 77 kbps
  next to a 640x360 picture and 44 kbps next to a detailed 1920x1080 one
  (VP8: at least 48 kbps, as its Vorbis encoder does not open lower). In size mode the picture has one
  keyframe instead of one every 10 s: a detailed 1920x1080 picture with a
  400 s sound was 62 MB at 4 MiB before, 3.4 MB now.
- Batch: a small picture of each file (from 10% in), made two at a time.
- Batch: "Join into one file" makes one job from the included files in list
  order, each with its trim, at the first file's picture size and frame
  rate; other sizes get black bars, files without sound get silence.
- Audio only: Format Audio with Opus (.opus), MP3 (.mp3) or AAC (.m4a). The
  ranges, speed, reverse, fades, volume and normalization apply. In size mode
  the bitrate comes from the limit (8-320 kbps) and is corrected by the same
  retry loop as video.
- Sample length: 3, 6 or 10 seconds, chosen in the Sample dialog.
- Ctrl+Shift+Enter adds to the queue without starting it.
- Sample dialog in quality mode: a rough size estimate for the whole output
  from the sample's bitrate.
- Sample dialog: "Compare with the source" plays the source over the same
  seconds next to the sample.
- A queued job keeps the output folder that was set when it was added.
- Settings: Interface size (80-150%), since browser zoom is turned off.
- Settings are split into Look, Tools and Files tabs; with ffmpeg missing
  the dialog opens on Tools.
- Queue: Copy error on a failed job.
- The system's "reduce motion" setting turns off the app's animations.
- Dialogs keep Tab inside themselves (wrapping at the ends) and give focus
  back to the control that had it when they close.
- Filters: Stabilize (ffmpeg deshake, per range, before the crop) and Smooth
  slow motion (at speeds below 1x, minterpolate makes in-between frames up to
  the source rate or the cap instead of repeating frames; slow to encode).
- Music: a sound file that replaces the source's sound or is mixed under it
  (Audio group), with its own level and start point, cut to the output's
  length and faded with the clip. A silent video can get sound this way.
- Subtitles: delay (seconds, either way) and font size.
- Logo: an image (PNG with transparency, JPEG, WebP) in a corner of the
  picture, sized as a share of the output width, with opacity. Remembered,
  and used for batch files too. Shown on the player.
- Caption: text on the picture (Advanced > Caption), top or bottom,
  white with a black outline, sized as a share of the output height, with
  several lines allowed. The player shows where it goes.
- Crop: "Find bars" crops black bars (letterbox or pillarbox), found with
  ffmpeg cropdetect at a quarter, half and three quarters of the file.
- Animated WebP output (libwebp_anim): quality on a 0-100 scale in quality
  mode (shown as 100 minus the libwebp quality, so lower is better as for
  the other codecs); in size mode quality 75 and the picture size is chosen
  to fit. ffprobe cannot read animated WebP, so the output check skips it.
- GIF output: Format GIF, one palette per clip. In size mode the picture
  size is chosen to fit (GIF has no bitrate setting). New platform preset
  "GIF (4 MiB)"; existing installs get it from Restore platform presets.
- Cut only (no re-encode): copies one range's video and audio into WebM or
  MP4 when the codecs fit. Fast and lossless; the cut starts on a key frame.
- Timeline filmstrip (60 thumbnails) and audio waveform, cached, with
  buttons to turn each off. Zoom to the selected range.
- The queue is saved and survives a restart. A job that was encoding when
  the app closed is marked "Interrupted"; waiting jobs wait for Start.
- Queue: Retry / Encode again, Edit (opens the source with the job's ranges
  and crop), move waiting jobs up and down, Pause after the running job, a
  short summary per job.
- Each finished output is probed and compared with the plan (video stream,
  audio, picture size, length); differences are shown on the job and in the
  notification.
- Reopening a file restores its ranges, output order and crop (last 40
  files); Ctrl+Z removes the restored edit.
- Undo and redo also cover rotation, mirror, vertical flip and the
  adjustments (a slider drag is one step).
- Settings: cache size with Clear and Clear downloads; Search again for
  ffmpeg, ffprobe and yt-dlp.
- Settings > Tools says when a saved ffmpeg or yt-dlp location does not
  hold the tool and another copy (beside the app or on PATH) is used.
- The sidebar can be resized by dragging its edge; double-click resets.
- The size field takes units: MB, KB, GB are decimal, MiB, KiB, GiB (or a
  bare number) binary. Up and Down step 0.5 MiB (Shift: 5 MiB).
- CI workflow: frontend typecheck and build, engine tests, and frontend
  unit tests (`npm test`, Node's own test runner, no new packages).
- The JSON field names of EncodeSpec, MediaInfo and Settings are checked in
  (`crates/engine/contract/`); an engine test and `npm run check` fail when
  the Rust types or `src/lib/types.ts` drift from them.
- Shift+S (or Shift+click Frame) saves the frame as the encode sees it:
  filters, adjustments, crop, rotation, flips and the output size.
- Hovering the timeline shows the thumbnail at the pointer.
- The player plays at the Speed setting (pitch kept, as in the encode).
- The player shows fade in and fade out (darkening at the output's start
  and end).
- Find cuts: scene cuts of the video (ffmpeg scene score over 0.3) shown as
  ticks on the timeline; range edges snap to them, and `[` / `]` jump to the
  previous / next cut. Cached per file.
- The preset line lists which settings differ from the preset, with Reset.
- Range edges snap to the playhead, other ranges and the file's ends; Alt
  places them freely.
- The window title shows the running job's percentage and the open file.
- The summary under Encode shows the file name the encode will write (full
  path on hover).
- Queue: Output folder button; copy a job's ffmpeg command; Remove waiting;
  a small picture of each job's source.
- Hints in the Open URL dialog for common yt-dlp errors (sign-in needed,
  format not available, rate limits, outdated yt-dlp).
- The preview copy can be canceled, and opening another file stops one that
  is still being made.
- Preview copies: when the webview cannot open the file but can decode its
  streams (for example an MKV from "Download and edit" with H.264 or VP9),
  the streams are copied into an MP4 at full resolution in seconds instead
  of a 480p transcode. If that copy does not play, the transcode follows.

### Changed

- The header shows the new app icon. The icon files have a transparent
  margin, so the Windows title bar icon is not against the window's edge.
- The sidebar can only be resized when Settings > Look > Sidebar is on
  (it could always be dragged before).
- New app icon (the sleeping man with the play-button pillow).
- "Smooth slow motion" is called "Interpolation".
- "Logo" is called "Watermark".

### Fixed

- Settings: checkboxes were as tall as a text field and sat above their
  labels.
- Queue: the job summary showed "Â·" instead of "·".
- The engine's contract test failed on Windows checkouts with CRLF line
  endings.
- The webview could read any file on disk through the asset protocol
  (scope `**`). The scope now holds only the app's cache folder; a file is
  added when ffprobe reads it as media, when it is picked in a dialog or
  dropped, and a logo image when a plan uses it. The release build's
  content security policy no longer allows the dev server's localhost
  addresses.
- ffprobe reads some text files (.nfo, .ans and similar) as video, so they
  could be opened and passed that check. Streams from ffmpeg's text-art
  decoders no longer count as video.
- The reverse and boomerang memory warning came at any output over 30 s,
  whatever the picture size. It now estimates the frames held (output size x
  frame count) and warns above about 1 GB, with the number: 40 s at 1280x720
  and 30 fps is 1.7 GB; the same at 640x360 does not warn.
- The webview could open any path in its default program (permission
  `open-path` with scope `**`; opening an executable runs it). The
  permission is removed; Play, Output folder and Open folder go through a
  new `open_item` command that opens only folders and media files.
- Preset export wrote JSON to whatever path the webview named, so it could
  replace any file. The export and import file dialogs now run in Rust (as
  theme import and export do), and the webview's save-dialog permission is
  removed. `open_item` also refuses macOS `.app` bundles.
- A settings.json or queue.json that could not be read was replaced by
  defaults on the next save. It is now renamed to `<name>.bad-<time>`
  and a notice at start says so. Two settings saves at the same moment
  can no longer interleave on the temp file.
- The picture size shown for a resolution step could be 2 px narrower
  than the encode's (1920x1080 at 487 px high: shown 864, encoded 866);
  the app now rounds as the engine does.
- Error text shown in the app (and copied with Copy error) could contain
  web links with a user name and password or sign-in tokens in their query.
  Those parts are now replaced by `***`; `v`, `list`, `t`, `index`, `p`,
  `page` and `id` parameters are kept.
- Closing the app left ffmpeg running when a job, sample or download was
  active; all child processes are now stopped on exit.
- A finished output or saved frame could replace a file that appeared at
  the same name during the encode (for example from a second instance).
  Names are now reserved with an exclusive create before the rename.
- A failed or canceled yt-dlp download could delete partial files of
  another download in the same folder. Each download now keeps its partial
  files in its own temp folder.
- A new sample deleted the files of a sample still playing, and closing the
  sample dialog did not stop the encode.
- A file changed on disk kept its old probe result until restart.
- Opening two files quickly could show the first one after the second.
- Double-clicking Encode added the job twice.
- ffprobe on a file on a stalled network share, or a hanging tool check,
  blocked forever; they now stop after 60 s and 20 s.
- After the last job ended, the queue still reported itself as started.
- Quiet text in the Nord Frost and Catppuccin Mocha themes was just under
  the 4.5:1 contrast for small text (4.46 and 4.44 on panels); now 4.63 and
  4.68.
- Adding a large or network folder to the batch no longer freezes the
  window while it is listed.
- Preview copies and timeline pictures above 4 GiB are deleted at start,
  oldest first.

## Build 1.2.0 - 2026-11-15

### Added

- Batch tab: add files, a folder (optionally with subfolders) or drop them;
  every file is planned with the current settings and checked (picture too
  poor for the size limit, too long for the preset, scaled down, orientation or
  size unlike the rest, variable frame rate, interlaced, HDR, no audio), with
  one-click fixes and suggestions for the whole batch. Per-file trim and crop
  (through the editor), frame rate cap and sound switch. `B` switches to it.
- Adjustments in the Advanced tab: brightness, contrast, saturation, gamma, hue
  and a master curve editor, previewed on the picture.
- Vertical flip, next to horizontal flip under Rotate.
- File info tab in the sidebar.
- Custom theme: 13 colors, corners, font, light or dark; start from a preset;
  import and export as JSON. It replaces the Paper theme.
- yt-dlp "cookies from browser" setting.
- Stacking notifications at the top right for finished encodes, saved frames
  and downloads.
- Hover text that explains each setting.
- `config.toml` and one `.toml` file per preset in the config folder, editable
  by hand; Reload config, Open config folder and Restore platform presets in
  the preset manager.

### Changed

- Resolution steps are output heights by default (240p to 2160p), with widths
  as the other choice; the lists come from `config.toml`.
- Frame rate caps are listed from low to high.
- The output folder defaults to the Desktop, with "beside the source file" and
  another folder as choices.
- Frame step keys are `<` and `>`.
- The preset's picture limit is no longer an editable field in Advanced; it is
  shown under the resolution with an explanation and can be removed.
- "Folder" and "File name" in the sidebar are now "Output folder" and "Output
  filename".
- Theme import and export open their file dialogs in the Rust side, so no
  command takes a file path from the page for them.

## Build 1.1.0 - 2026-11-15

### Added

- Theme presets in Settings: Comfy Dark (default), Nord Frost, Gruvbox Retro,
  Catppuccin Mocha, Phosphor, Paper. Each sets shapes, fonts, borders, shadows
  and spacing as well as colors.
- App icon drawn for comfywebm (frame, play mark, timeline with an in/out range
  and a playhead), and the icon and version in the header.
- Sidebar tabs: Basic and Advanced. Filters, subtitles, audio level, encoder
  effort, the picture limit, metadata, extra arguments and the ffmpeg command
  are under Advanced, which shows how many of its settings are changed.
- Resolution choices: Auto, Source, long-edge steps, percentage, or one exact
  edge with the other from the aspect ratio. In size limit mode each shows a
  good / fair / poor dot for the bitrate the limit allows.
- Motion check: the frame rate cap and speed are checked against the display
  refresh rate (Settings, default 60 Hz). Speed is a slider that snaps to speeds
  that play evenly; judder and stutter are explained with the nearest smooth
  speeds and caps as buttons.
- "Download only" in the Open URL dialog: keeps the original file in the
  Downloads folder (or a folder set in Settings), without a look-up first.
- Crop aspect locks 4:5, 3:4, 2:3, 3:2, 5:4, 2:1, 1.85:1, 2.39:1, 32:9, 9:21,
  1:2 and a custom ratio field.
- Editor and Queue tabs in the header (`Q` switches). The queue no longer takes
  space under the timeline. The Queue tab's badge shows the number of jobs
  running or waiting and the running job's percentage, then the number that
  finished (red if one failed) until the queue is opened. "Clear finished"
  removes finished jobs from the list.
- Timeline zoom: Ctrl+wheel zooms at the pointer (down to about 20 frames
  across), the wheel pans while zoomed, buttons zoom out, fit and zoom in. A
  time ruler above the track, an overview bar below it while zoomed, and the
  time under the pointer while hovering. The view follows the playhead.
- Undo and redo (Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y, and buttons) for ranges, their
  output order and the crop. A drag is one step.
- Key list (`?`, F1, or the keyboard button in the header).
- Hover text drawn in the theme instead of the native tooltip; icon buttons show
  their name.
- The window's size, position and maximized state are kept between runs
  (`window.json` in the app config folder). A position on a monitor that is no
  longer connected is not used.
- Recent files (the last 10) in the empty state.
- Edit strip for two or more ranges: shows the joined output in order, drag to
  reorder, click to jump, "Play edit" plays the ranges in sequence.

### Changed

- Size limit mode fills more of the limit: the first attempt leaves a 1.5%
  margin (was 3%); a result under 98% of the limit (was 88%) is encoded again
  at a bitrate aimed at 99%, interpolated between earlier attempts; at most 4
  attempts (was 3). On test clips a 4 MiB limit gave 3.95 MiB in one attempt.
- Drag and drop: the drop target is the preview area; the overlay covers only
  that area, and a drop elsewhere does nothing.
- All drop-down lists are drawn by the app and follow the theme.
- The WebView2 context menu and browser keys (Find, Print, Reload, zoom,
  history) are turned off; text fields keep their own menu.

### Fixed

- Speeding up dropped frames. After `setpts` ffmpeg kept writing at the source
  frame rate, so 1.5x of a 30 fps source kept 2 of every 3 frames (121 frames
  at 30 fps instead of 180 at 45 fps for 6 s of source). The output rate is now
  set to source fps x speed (or the cap), and every frame is kept.
- Drop-down lists closed when the mouse button was released, so no option
  could be picked.

## Build 1.0.0 - 2026-11-15

First build.

### Added

- Open local files by dialog, drag and drop, or as the first command-line
  argument. Opening or dropping several files adds one whole-file job each.
- Open web links through yt-dlp: title, uploader, duration and thumbnail,
  quality choice, optional partial download, progress and cancel. Pasting a
  link opens it.
- Player with timeline, frame stepping, mouse-wheel frame stepping, loop of the
  selected range, and a preview copy (H.264) for files the webview cannot play.
- Several ranges per file, joined into one output or encoded as separate files.
- Crop drawn on the picture, with handles, aspect lock (free, 1:1, 16:9, 9:16,
  4:3, 21:9), arrow-key move and resize, and numeric fields.
- Size limit mode: bitrate from the limit, clip length and audio bitrate; two
  passes; up to 3 attempts that lower the bitrate when over the limit and raise
  it when well under; the largest result that fits is kept.
- Auto size: lowers the output resolution when the bitrate is too low for the
  picture size. Bits per pixel and a good / fair / poor rating in the sidebar.
- Quality (CRF) mode with an optional bitrate cap.
- WebM output with VP9, VP8 or AV1; MP4 output with H.264, HEVC or AV1.
  Opus, Vorbis or AAC audio.
- Filters: deinterlace, denoise, HDR to SDR tone mapping, rotate, mirror,
  speed, reverse, boomerang, fade in and out, maximum size, frame rate cap.
- Audio track choice, bitrate, volume and loudness normalization.
- Burned-in text subtitles from the file or from an `.srt` / `.ass` file,
  timed to the cut.
- Sample: a 3-second encode at the playhead with the current settings.
- Frame export as a full-resolution PNG with the crop applied.
- Encode queue with progress, ETA, attempts, cancel, Play, Reveal and Copy
  path, and taskbar progress.
- Built-in platform presets for 4chan, Discord and X, plus quality presets.
  User presets with save, overwrite, rename, delete, import and export.
- Settings for the ffmpeg and yt-dlp locations, extra yt-dlp arguments, the
  download folder, the output folder and the output name template.
- The ffmpeg command for the current settings, shown and copyable.
- `scripts/smoke.sh`: short real encodes of generated test clips.
