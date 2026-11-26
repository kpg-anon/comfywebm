# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Interpolation with RIFE: Settings > Tools > Interpolation chooses between
  the built-in method (ffmpeg's minterpolate) and RIFE, a neural network for
  in-between frames (model 4.26, rife-ncnn-vulkan, on the graphics card).
  Install RIFE downloads it from GitHub (about 830 MB) and keeps about 20 MB.
  Before the encode, RIFE fills the ranges' span with the new frames the slow
  motion needs, and the encode uses them.
- Presets for Mokachan and awoo: VP9 WebM in quality mode (CRF 30) with
  Opus sound. Their limits (100 MiB and 512 MB) are far above a normal clip.
- Full screen: a button at the top right of the picture, or `F`, shows the
  picture over the whole screen; `F` or Esc leaves.
- Follow: Shift while dragging the picture moves the view across only,
  keeping its height.

### Changed

- The timeline: a click or drag on a range seeks like the rest of the track,
  and the picture follows while you drag. Ctrl+drag moves a range; a plain
  drag no longer does.
- Follow: Ctrl+drag on the picture moves the whole path (it was Shift).
- The Follow lane's controls (Zoom (all), Track box, Track face, Track
  points) are in a row above the lane, so they no longer cover its lines.
- Crop mode with Follow keyframes shows the whole picture: the crop box can
  move anywhere on it, not only inside the largest view at the playhead. Its
  size is still capped at that view.

### Fixed

- "Go to this keyframe" in a Follow keyframe's menu also selects the
  keyframe.

## [1.2.2] - 2026-11-24

### Added

- Rotation keyframes: a Rotate lane under the timeline turns the picture
  partway through the clip, for a phone video that goes from upright to
  sideways. A keyframe changes the angle on its frame, or turns smoothly from
  the previous keyframe; before the first one the Rotate setting applies. The
  lane is an angle graph whose scale fits the keyframes (a turn past 180°
  is drawn on to 270°), and a row under the lane sets a keyframe's exact time
  and angle and At once or Smoothly. The
  output has the picture's shape at its first frame (or the Canvas's), and a
  part at another angle fits in it, or fills it (Advanced > Canvas > Picture).
  The encode turns the picture with ffmpeg's `rotate` on a
  frame-shaped canvas and cuts and scales it with `zoompan`; the frames before
  the first turn come out the same as without keyframes.
- Follow keyframes each have their own zoom: the view zooms smoothly between
  them (a dotted line on the Follow lane), and a row under the lane (click a
  keyframe) sets one keyframe's time, zoom and turn exactly. The Rotate and
  Follow rows stay in place, greyed out with no keyframe open, so the lanes
  do not move. The Zoom
  (all) slider still sets every keyframe. The output is the size of the widest
  view; the encode cuts it and zooms in from it with `zoompan`, centred on the
  path.
- Close the file: the × beside the file name at the top, or Ctrl+W, goes
  back to the start page with the recent files. The file's edit and settings
  are saved first.
- PgUp / PgDn move the playhead to the previous / next keyframe on the lanes
  shown (Follow, Rotate, Speed).
- Rule-of-thirds lines over the output: the grid button at the bottom left of
  the picture, while the Follow lane is shown.
- Advanced > Canvas: the output's frame as the picture's own shape (Auto),
  9:16, 16:9, 1:1, 4:5, a custom ratio (such as 2.39 : 1), or a custom size
  in pixels. A picture of
  another shape fits in it with black or a blurred copy of itself around it
  (Background), or fills it with the edges cut (Picture: Fill). The preview
  shows the frame's edge.

### Changed

- The preview's volume slider stands upright above the speaker.
- Frame (save the frame at the playhead) moved from Video tools to a button
  at the bottom right of the picture, beside the speaker and Loop. Video
  tools: Speed, Follow, Crop on the first row; Rotate, Find cuts, Sample on
  the second.
- Follow lane, right-click > "Add a keyframe here": the playhead goes to the
  new keyframe and selects it (it went to the keyframe after it).
- Opening a file you edited before brings back Rotate, the flips and the
  adjustments with the rest of its edit (Rotate was kept only with rotation
  keyframes), and its own settings as they were left: speed, direction,
  fades, filters, sound track and volume, subtitles, caption, title, file
  name and canvas. Settings > Files > Opening a file again: As it was left
  (the default), The edit only, or Start clean.
- Sample starts at the selected range's In point, wherever the playhead is.
  The dialog is the window's height and its pictures (and the side-by-side
  comparison) are scaled to fit, without scrolling; Close stays at the bottom.
- The speed chips leave out speeds within 0.5% of a whole number (1.001x,
  2.002x, 3.003x for 59.94 fps footage on a 60 Hz display).
- Esc in crop mode leaves it without the changes made there (the crop, the
  aspect lock, and a follow path the crop moved go back to how they were);
  Enter and Done keep them. The crop bar has a Cancel button.

## [1.2.1] - 2026-11-23

### Added

- Settings > Files > Size limit > "Aim for": the share of the limit an encode
  aims at, sound and video together, from 50 to 100%. The default is 97.66%,
  what webm.py's `-l` aims for (it counts the limit in kilobits of 1024 bits and
  ffmpeg reads them as 1000), in place of the old one-attempt aim of about 95%
  (a 1.5% margin on the video, then 96.5% of that).
- Settings > Files > On start: "Start from the preset" (the default) opens the
  app with the last used preset's values, and speed, playback, fades, sound
  volume and normalize, stabilize, denoise, slow motion, separate files and the
  watermark at their defaults, so options left from the last run do not end up
  in an encode unnoticed. The watermark's place and size, the caption style,
  the step list and the aspect lock are still kept. "Keep the last options"
  brings back everything, as before.
- File name template token `{preset}`: the platform (the preset name's first
  word) and the size limit, as `4chan-4MiB` or `Discord-20MB`.
- Adjustments > Auto: measures 12 frames of the ranges and sets the white
  balance (new Temperature and Tint sliders, ffmpeg `colorchannelmixer`, scaled
  to keep the light level), a contrast stretch for a flat picture and a gamma
  for a clearly too-bright (or very dark) one. The light's colour comes from an
  established colour constancy method, chosen in Settings > Tools > Auto color:
  Combined (the default: the mean of Grey-Edge, White Patch and Grey Pixels),
  Grey-Edge, Grey Pixels, White Patch, Shades of Grey or Grey World, all built
  in. Measured on 11 clips with known casts added, mean / median angle from
  the true light: Combined 4.6 / 3.6 degrees, Grey-Edge 4.8 / 3.3, Grey Pixels
  4.7 / 4.2, White Patch 5.2 / 4.4, Shades of Grey 5.9 / 4.5, Grey World 9.3 /
  7.8. A cast within 3.5 degrees is left alone and only the part beyond it is
  taken out, so a clip that is already balanced is not shifted.
- Adjustments: hold the eye (Original) at the top of the group, or hold `\`,
  to see the picture without them; letting go shows them again. The "Show on
  the picture" checkbox is gone, so adjustments can no longer stay hidden by
  mistake. Each slider and the curve have a reset button (double-click on a
  slider still resets it too).
- Follow lane: several keyframes can be selected. Drag on the lane to draw a
  box around them (Ctrl+drag adds), Ctrl+click adds or removes one,
  Shift+click selects from the last one clicked. Dragging a selected keyframe
  moves them all in time, Backspace or Delete removes them, dragging the
  picture moves their view, Esc clears the selection. The lane's right-click
  menu has Select all and Remove selected. The wheel over the lane zooms the
  timeline at the pointer (Shift+wheel pans).
- Timeline zoom: `Z` zooms to the selected Follow keyframes (two or more) or
  the selected range, `Shift+Z` shows the whole file, and dragging across the
  ruler zooms to that span (a click on the ruler seeks).
- Dragging the picture with a follow path moves the selected keyframes' view,
  or the whole path when no keyframe is selected (after a track nothing is),
  so a tracked face can be framed higher or lower without bending the path.
  Before, a drag moved (or made) one keyframe at the playhead. Shift+drag
  always moves the whole path.

### Fixed

- A file opened again with its last edit brought back started with the
  playhead on the first frame; it now starts at the selected range.
- Start screen, Recent: removing a file made the centred box shorter, so the
  Open button, every row and the next file's remove button moved. The drop box
  now stays in the middle of the window whatever the list holds, and the list
  hangs below it in the same dashed frame: it grows downward, and removing a
  file moves the older ones up and shortens the frame from the bottom.
- Track face, Track points and Track box followed from the playhead to the end
  of the file when the playhead was not inside the selected range, including
  exactly at its end (as after setting Out): a 2.9 s range got 200 keyframes
  from its end to the end of the file and none inside it. Track now never
  leaves the selected range; with the playhead outside it, the playhead goes to
  the range's start first, where you pick the face, box or points.
- After Track face (or any follow path), crop mode showed the whole source
  picture, and moving the crop did nothing, because the path sets where the
  view is and the crop only its shape. Crop mode now stays on the output as
  you see it: the largest view the path can have at the playhead is held
  still as a dashed area, and the crop box is the view as it is now inside it.
  It can shrink, grow back to the whole area and move; an aspect such as 1:1
  gives the largest box of that shape around the view's middle; moving the box
  moves the whole path by the same amount (to take in the mouth below a
  tracked face, for example).
- Crop mode: a crop as large as the picture had its corner handles on the
  stage's edges, under the timeline's divider and the bar above, so grabbing
  a corner missed the crop (and could resize the timeline instead). The
  picture now has a 16 px margin in crop mode.
- The preview lost colour while Follow turned the picture (Track face turns it
  to keep the eyes level). An unturned video is drawn by the graphics card as
  an overlay, which skips the conversion to the display's colour profile; a
  turned one is drawn with the page, which converts it. On a wide-gamut display
  the same frame measured R 180 G 88 B 251 unturned and R 159 G 98 B 245 turned.
  The window now uses an sRGB colour profile (WebView2
  `--force-color-profile=srgb`), so both are drawn the same (180/88/251 and
  181/90/251). The encoded file was not affected: ffmpeg's rotate keeps the
  colour (mean saturation 14.2 without the turn, 14.1 with it). The rest of the
  interface is also no longer converted to the display's profile.

## [1.2.0] - 2026-11-20

### Added

- Follow is a lane of keyframes under the timeline, opened by the new Follow
  button in Video tools (the six buttons are now 3 x 2). Click the lane to add
  a keyframe and drag the picture to aim the view; drag a keyframe to move it,
  double-click or right-click it to remove it. Track... follows a person and
  adds keyframes fitted to the tracked path (within 1% of it; 15 keyframes for
  8 s of a moving box) instead of a point every 0.25 s.
- Track moves the view like a camera operator: the person may sit off centre
  within a band (70% of the room between their box and the view's edge) and
  the camera holds still or pans evenly within it, instead of copying every
  movement. On a 4.6 s 60 fps dance clip the frame-to-frame change in camera
  speed went from 0.62 to 0.01 px per frame squared and the face stayed in
  the view. The zoom Track sets now has the box fill 60% of the view (was
  85%), so there is room for this.
- Settings > Tools > Tracking: Follow's Track can use VitTrack (OpenCV's
  neural tracker) or SAM 2 (Meta's SAM 2.1, which follows the person's
  outline) instead of the built-in tracker. Install buttons make a Python
  environment for comfywebm alone with uv (no system Python used); the app
  checks free disk space first (SAM 2 with CUDA needs about 7 GB). Test
  measures a tracker on a square moved along a known path (built-in 2.6 px
  off on average in 0.5 s, VitTrack 2.8 px in 1.5 s, SAM 2 1.7 px). VitTrack:
  27 s to install, a 4.6 s 4K clip tracked in 5 s; on a 4K group dance where
  the built-in tracker slid off a member's face onto the wall within 1.3 s,
  VitTrack and SAM 2 stayed on her. SAM 2 on an RTX 4070 laptop GPU (PyTorch
  2.15 for CUDA 13.2, installed in 7 minutes, 3 GB): 4.6 s of 4K 60 fps in 17 s,
  2.7 s of a 4K group dance in 8 s; it gets the frames from memory instead of a
  folder of JPEGs (2.7 s of every 6.1 s chunk) and looks at 15 frames a second
  with a GPU. On the CPU about 7.5 s per analysed frame.
- Track face (Follow lane): click on a face and the view stays on it and turns
  to keep the eyes level. OpenCV's YuNet face finder (230 KB, fetched on first
  use, run with VitTrack's OpenCV) looks at every frame afresh, after turning
  it upright by the last tilt (YuNet read a 41 degree tilt as 26). On a 4.4 s
  dance clip with a 1:1 crop the output's eye line was 2.2 degrees off level
  on average (5.1 at the 95th percentile), against 11.4 (27.4) in the source.
- Track points (Follow lane): one or two clicked points followed by a patch
  matcher at three sizes per frame; two points also turn the view.
- Follow's view has the crop's shape and moves over the whole picture (with a
  1:1 crop, a square anywhere on a vertical video), and keyframes can turn it.
  The encode uses crop with per-frame x/y and rotate with a per-frame angle
  instead of zoompan.
- Settings > Tools > Tracking: SAM 2 is marked * for the best results.
- Follow has its own zoom, separate from the sidebar's: a Zoom slider on the
  Follow lane sets it for all keyframes (they carry it to the encode), and
  Track no longer changes the sidebar zoom. The sidebar's Zoom is for the
  picture without Follow and is greyed out while Follow has keyframes. Edits
  saved by the earlier test builds (keyframes at 1x with a sidebar zoom) are
  converted when the file opens.
- `Ctrl+E` encodes and `Ctrl+Enter` adds to the queue (it encoded before); the
  buttons show their keys.
- Settings > Files > Queue: Add to queue can start encoding at once instead of
  leaving the job waiting.

- Install ffmpeg and Install yt-dlp, in Settings > Tools and on the screen
  that says one is missing, so nothing has to be fetched by hand. ffmpeg on
  Windows and Linux is BtbN's newest release-branch GPL build (n9.0.2 with
  vid.stab 1.4 today, 189 MB; installed in 46 s here), on macOS Homebrew's
  ffmpeg-full; yt-dlp is its newest release. Each goes to the app's own tools
  folder and only comfywebm's setting points at it. Progress bar, Cancel
  (nothing changes), Update and Remove. The macOS paths are untested.

- Settings > Tools > Stabilization: Stabilize can use vid.stab (an analysis
  pass per range, then the frames moved onto a smoothed camera path) or
  deshake. Automatic picks vid.stab when ffmpeg has it. Each choice says what
  it does and what to expect, and Test measures it on a shaken test picture:
  vid.stab 1.4 took the shake from 9.9 to 1.0, vid.stab before 1.4 to 4.8,
  deshake to 7.3. A 1080p range shaken by up to 65 px went from 9.2 to 0.6.
  The analysis runs once per job, also for Sample, Frame and the quality
  check. ffmpeg 8.1 builds with vid.stab before 1.4 (for example gyan.dev
  8.1.1) damaged the picture (FFmpeg ticket 22595); the app gives vid.stab a
  copy of each frame, which avoids it.
- Gyroflow as a third Stabilize method, with Install Gyroflow (its newest
  release from GitHub, 80 MB on Windows). Before the encode the ranges' span
  is cut out (x264 CRF 8, sound as PCM), Gyroflow steadies it from the picture
  alone (`--ignore-file-motion --optical-correction 100`) into ProRes 422, and
  the job is planned on that file with all times moved back by the span's
  start (ranges, speed ramp, zoom, subtitles; embedded subtitles are taken out
  as a file first). Frame-exact: a test kept exactly frames 100-120 and
  306-326. Test: 9.9 -> 3.1 (69% less) in 2.7 s. A picture with too little
  detail gives Gyroflow's own reason ("Not enough of the image could be
  tracked").

### Changed

- Size limit: one attempt by default, aimed at about 95% of the limit, so an
  encode no longer seems to start over after it finished. Settings > Files >
  Size limit > "Encode again when the file misses the limit" brings back up to
  4 attempts aimed at 99%. An 18 s 1080x1920 clip for Discord's 20 MB: 18.9 MB
  in one attempt (with retries its first attempt went over and it took two).

- Follow in the preview: while playing, the view is placed for the frame on
  screen (requestVideoFrameCallback). It used the playhead read on each
  screen refresh, which ran a median of 20 ms (more than a 60 fps frame) and
  up to 670 ms away from the frame shown, so the picture shook.
- The sidebar's zoom is "Global zoom" (its slider "Level") and no longer shows
  Follow; Follow is only in Video tools and on its lane.
- The built-in tracker's progress no longer stops at 72% while it matches the
  box (it reported only the reading of the video).
- Track reads positions to a fraction of a pixel and smooths over a fifth of
  a second without pulling the first and last positions towards the middle
  (the first point was 64 px off the drawn box on a test clip).
- Zoom and follow keyframes: the view moves on a smooth curve through the
  keyframes (cubic, slopes limited as in Fritsch-Carlson) instead of easing
  to a stop at each one. It still starts and stops gently at the first and
  last keyframe and holds at a keyframe where the movement turns back. The
  preview (zoom.ts) and the encode (zoom.rs) use the same curve; a test
  encode matched the preview's window at every frame checked.
- The Queue tab's badge: orange while a job encodes, grey while jobs wait in a
  paused queue, green when done, red when one failed.
- Settings > Look > Timeline: the thumbnail choices read "Every frame" and
  "Key frames only".
- The player's corner: the sound control comes first and Loop sits in the
  corner.
- Settings > Tools > Stabilization: the Gyroflow description is shorter.

### Fixed

- An encode with a long Follow path (Track face over 18 s: 162 keyframes)
  failed with "The filename or extension is too long. (os error 206)": the
  command was 96,289 characters and Windows allows 32,767. A filter that makes
  the command too long now goes to ffmpeg in a file (`-/filter_complex`, or
  `-filter_complex_script` before ffmpeg 7). The keyframe curve's expression
  is nested by halving instead of one level per keyframe, because ffmpeg's
  parser stops at about 100 levels ("Missing ')'"). That 18 s clip then encoded
  to 19.8 MB with the eye line 1.8 degrees off level on average.
- With Follow's turning view, a smaller output (Auto resolution, a size limit)
  came out at the view's own size: 1080x1920 where 720x1280 was planned, and
  the queue said "check output". The view is now scaled to the planned size.


## [1.1.2] - 2026-11-19

### Fixed

- The frame after the Out point could end the clip (1.1.1 and earlier). ffmpeg
  counts an input's `-t` from the first frame it keeps, not from `-ss`, so a
  range whose end fell on a frame's start let that frame in. Every WebM or
  Matroska file (times in whole milliseconds) and many MP4s were affected: a
  check of random ranges with the 1.1.1 rule found the wrong last frame in 17
  of 40 on a 60 fps MP4 and 3 of 40 on a 60 fps WebM. `-t` is now a quarter
  frame shorter; the same check found 280 of 280 ranges exact (30, 29.97 and
  60 fps; MP4, WebM and Matroska).
- Two-pass encodes with sound failed with ffmpeg builds from its master
  branch (for example gyan.dev's November 2026 nightlies): pass 2 looked for
  a pass log for the sound. The pass options now apply to the video only.

## [1.1.1] - 2026-11-19

### Changed

- The expected size beside Encode is written "~3.94MiB" instead of
  "up to 3.94 MiB".
- The Basic, Advanced and File info tabs share the sidebar's width, with
  their names centered.
- New default: Best effort and two passes. The built-in preset files get
  this when they are still as the app wrote them; a preset file you edited
  is left alone. Each Effort choice says on hover which encoder setting it
  uses. One pass in quality mode now counts as a changed Advanced setting.
- Adjustments: the six curve presets sit in two columns under "Curve".
- 0.4x and 0.8x are always among the speed chips (60 and 30 fps footage
  play at 24 fps; drawn dashed when not even on the display), in the speed
  ramp's snapping and in its point menu.
- Crop mode: Enter leaves it too, keeping the crop, like Esc.
- Queue: the running and waiting jobs are listed first, in the order they
  run; finished jobs follow, newest first.
- Frame steps (`,` `.` and the wheel) go to the middle of the frame, so the
  player cannot show the frame next to it.
- While the player is still decoding the frame at the playhead (a few hundred
  ms for a 1080p file with key frames 10 s apart), the picture shows
  "Loading frame..." and the time above the controls is dimmed. Seeks asked
  for during a seek wait for it and only the newest runs, so holding `,` or
  dragging the playhead shows frames on the way instead of none.

### Added

- Settings > Look > Timeline: thumbnails from every frame (the default) or
  from key frames only. Every frame gives one thumbnail per second, each
  from the frame at its time (a 4-minute 1080p file: 240 thumbnails in about
  4 s), and the key frame ones show until it is done. Before, a file with key
  frames far apart showed the same picture for several seconds.

### Fixed

- Cuts started one frame late: a range start inside a frame (nearly every
  start set by clicking, I or frame stepping) began the output at the next
  frame, and the Frame button (S) saved the next frame. ffmpeg's `-ss`
  starts at the first frame at or after the time, while the player shows the
  frame that holds it. Range edges now snap to the frame the player shows;
  checked against ffmpeg on 30, 29.97 and 60 fps files in MP4 and Matroska.
  Files with a variable frame rate are cut as before.
- The quality check scored MP4 jobs far too low (a CRF 18 H.264 encode got
  68 instead of 99.7; a 4 MiB one 66 instead of 83): every third frame of the
  MP4 probe was compared with the next frame of the reference. Frames are now
  paired by their number. WebM jobs were measured correctly.

## [1.1.0] - 2026-11-18

### Added

- Settings > Look > Timeline, "Resizable": drag the edge above the timeline
  up to make its track taller (up to 240 px), with bigger thumbnails and
  waveform; double-click the edge for the standard height.
- Zoom (Advanced tab): a slider with 1x in the middle; right zooms in on the
  crop (or the picture), left shows more of the picture around the crop.
- Follow a person (Advanced tab, under Zoom): draw a box around someone; the
  app follows them to the end of the range and the zoom moves with them (a
  9.5 s 4K dance clip took about 2 s to follow). Drag the picture to correct
  the path; Remove takes it away. The output is the size of the window.
- History panel (`H`, or the clock button in the top bar): undo, redo and
  every edit of the open file with what changed and when ("Moved range 2 to
  12.5 s", "Crop 960x540"), newest first; click one to go back or forward to
  it. "Start fresh" in it (after a warning) removes every edit of the file
  and its history; the encode settings stay.
- Quality check: after the settings stop changing, the app encodes a 2 s
  piece with them and measures how close it looks to the source (VMAF, 0 to
  100). The output line shows "quality N" and a warning appears under 88
  (visible compression). Before, the good/fair/poor label came from bits per
  pixel alone: a fancam at 4 MiB for 10 s was "good" at 1152x2048 while it
  measured about 82 (visible compression). Needs ffmpeg with libvmaf.

### Changed

- The picture area shows only what the output will (the crop with the zoom)
  once crop mode is done, instead of the whole picture with the rest dimmed.
- The area under the picture is two panels: Timeline (the timeline zoom on
  the ruler's right end, a ruler with small ticks, a taller thumbnail track (72 px),
  the selected range framed in yellow, a marker on the playhead) and the
  controls in three sections: Playback (a large time, start, frame back,
  play, frame forward, end), Range (Time, In, Out, Length and the range
  buttons) and Video tools (Speed, Crop, Find cuts, Sample, Frame). The
  Fewer cuts / Normal / More cuts choice moved from beside Find cuts to
  Settings > Look > Find cuts. Undo and
  redo moved into the History panel; Loop moved to the picture's bottom
  right, beside the speaker. The start screen no longer lists keys (Keys in
  the top bar, or `?`, shows them all).
- The audio waveform in the timeline is off to start with (Settings > Look >
  Timeline turns it on), so the thumbnails show clearly.
- Above Encode: the output on one line, and on its right the warning icon
  (pointing at it shows the warnings) over the quality check's score. In the
  Advanced tab, Flip is above Rotate.
- Auto resolution aims at about 0.10 bits per pixel for VP9 (3.3 times the
  old floor) and goes no smaller than 540 pixels on the short side unless the
  limit is very small. A sideways 4K fancam turned upright, 10 s in 4 MiB, now
  gets 720x1280 instead of 1152x2048. Measured at 1, 2 and 4 MiB, the sizes
  from 648x1152 to 1152x2048 looked about the same by VMAF and everything
  under 540x960 looked worse.
- Crop mode shows the picture turned and flipped as the output will be (it
  was shown unrotated), and the crop is drawn on it; a 9:16 lock is upright on
  screen.

## [1.0.8] - 2026-11-17

### Changed

- Find cuts finds cuts the way PySceneDetect's adaptive detector does: a
  frame is a cut when its change (hue, saturation and brightness against the
  frame before) stands out from the frames around it, so camera moves and
  light shows are not cuts. A flash or strobe, where the picture comes back
  within 0.4 s, is not a cut either, and a fade through black is one cut in
  its middle. Jump cuts in fancams (the framing jumps between two frames) are
  found. On a test video with 4 cuts and a strobe, the old method found 20
  cuts and the new one the 4.
- Find cuts shows how far it has read, and a Fewer cuts / Normal / More cuts
  choice appears beside it; the video is read once, so changing it is
  instant.

## [1.0.7] - 2026-11-17

### Added

- A speaker at the bottom right of the picture mutes the preview (or `M`);
  pointing at it shows a volume slider, and the wheel over it sets the
  volume. Both are remembered. They change only what you hear while
  editing, not the encode's sound.
- Formats and codecs the preset's site does not accept are grayed out, with
  the reason on hover: 4chan takes WebM (VP8, VP9), MP4 (H.264) and GIF, not
  AV1, HEVC, WebP or sound-only files; X takes MP4 (H.264) and GIF. The site
  comes from the preset's name. A warning shows when the current choice is
  one the site refuses.
- Drag a range on the timeline to move it, keeping its length; the picture
  shows its first frame while it moves. Its ends snap to the other ranges,
  scene cuts and the file's ends (Alt: no snapping). A click on it still
  seeks.

## [1.0.6] - 2026-11-17

### Changed

- Frame step is `,` and `.` (Shift is no longer needed; `<` and `>` still
  work).
- Play plays the selected range: from its start when the playhead is
  outside it, and it stops at its end (with Loop, `L`, it starts again).
  A click past the end while playing keeps playing there.
- Sample, Compare with the source: the source is on the left and the sample
  on the right, and the source shows the same crop, so both show the same
  part of the picture at the same size. The source plays at the job's speed,
  so both show the same moment.

### Fixed

- A crop with an odd height or width (a 16:9 crop of a 2160-pixel-wide
  picture is 2160x1215) gave the file non-square pixels (sample aspect
  ratio 1025:1024). Players and sites then saw it 2050 pixels wide where
  2048 were planned, and 4chan refused it as over 2048x2048. The output now
  always has square pixels at the planned size.

## [1.0.5] - 2026-11-17

### Added

- Splits: Length "Longest, loop": the shorter clips start again from their
  start until the longest one ends; a clip's sound loops with its picture.
- Splits: a Loop button beside Play, on to start with: the preview plays
  again from the start when it reaches the end.
- Splits: Space plays and pauses the preview.
- A line above Encode (and beside Encode in Splits) with the output's picture
  size, frame rate, sound or no sound, length and file size. The size is in
  the unit of the size limit: MB for a limit in decimal megabytes (Discord),
  MiB otherwise (4chan). It replaces the summary lines; the bitrates, bits per
  pixel, quality and file name are in its tooltip.

### Changed

- The window title is "comfywebm — file.mp4" (it was "file.mp4 ·
  comfywebm"); while a job runs its percentage is still in front.

### Fixed

- Splits: the frame rate used for bits per pixel, automatic picture size and
  speed changes was the first clip's, not the combined picture's.
- After a click on a button (on its icon or label), Space pressed that button
  again instead of playing the video. Focus left over from a click is now
  dropped everywhere outside dialogs, and Space and Enter never press a
  button outside a dialog.
- A click on the video or the timeline left a focus outline (the accent
  color) around it after the next key press.
- When a dialog closed, focus went back to the last button clicked before
  it, with a focus outline; now it only goes back to a text field that had
  it when the dialog opened.
- In Settings, Escape did not close the dialog after a click on one of its
  buttons.

## [1.0.4] - 2026-11-17

### Added

- **Speed ramp**: a lane under the timeline with speed points (0.25x to 4x);
  the speed changes evenly between them. The preview plays it, the sound
  follows it with its pitch kept, and the output length, size limit and
  fades take it into account.

### Changed

- Open URL: Best available is the quality until you pick another (it was the
  largest up to 1080p).

### Fixed

- At start the preset field was empty for about 3 seconds, until the search
  for ffmpeg had finished. The preset now shows at once.

## [1.0.3] - 2026-11-16

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
