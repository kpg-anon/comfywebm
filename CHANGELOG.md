# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

First release, planned as 1.0.0.

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
