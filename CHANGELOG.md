# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Work on the `exp` branch.

### Added

- Batch: a small picture of each file (from 10% in), made two at a time.
- Batch: "Join into one file" makes one job from the included files in list
  order, each with its trim, at the first file's picture size and frame
  rate; other sizes get black bars, files without sound get silence.
- Audio only: Format Audio with Opus (.opus), MP3 (.mp3) or AAC (.m4a). The
  ranges, speed, reverse, fades, volume and normalization apply. In size mode
  the bitrate comes from the limit (8-320 kbps) and is corrected by the same
  retry loop as video.
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
- Settings: cache size with Clear and Clear downloads; Search again for
  ffmpeg, ffprobe and yt-dlp.
- The sidebar can be resized by dragging its edge; double-click resets.
- The size field takes units: MB, KB, GB are decimal, MiB, KiB, GiB (or a
  bare number) binary. Up and Down step 0.5 MiB (Shift: 5 MiB).
- CI workflow: frontend typecheck and build, engine tests.
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
- Queue: Output folder button; copy a job's ffmpeg command.
- Hints in the Open URL dialog for common yt-dlp errors (sign-in needed,
  format not available, rate limits, outdated yt-dlp).
- The preview copy can be canceled, and opening another file stops one that
  is still being made.
- Preview copies: when the webview cannot open the file but can decode its
  streams (for example an MKV from "Download and edit" with H.264 or VP9),
  the streams are copied into an MP4 at full resolution in seconds instead
  of a 480p transcode. If that copy does not play, the transcode follows.

### Fixed

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

## [1.2.0] - 2026-11-15

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

## [1.1.0] - 2026-11-15

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

## [1.0.0] - 2026-11-15

First release.

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
