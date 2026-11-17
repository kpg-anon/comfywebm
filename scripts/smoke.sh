#!/usr/bin/env bash
# Short encodes through the engine (crates/engine/examples/spec.rs), one
# PASS/FAIL line each. Test media is generated with ffmpeg into a temp
# directory that is deleted at the end (set KEEP=1 to keep it).
#
#   scripts/smoke.sh
#
# Needs ffmpeg, ffprobe, node and cargo on PATH.
set -u
cd "$(dirname "$0")/.."

work=$(mktemp -d "${TMPDIR:-/tmp}/comfywebm-smoke.XXXXXX")
[ "${KEEP:-0}" = 1 ] || trap 'rm -rf "$work"' EXIT

# Paths written into JSON go to a native Windows program, so convert them there.
native() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi; }

echo "building the example..."
cargo build -q -p comfywebm-engine --example spec || { echo "FAIL build"; exit 1; }
bin=target/debug/examples/spec
[ -x "$bin" ] || bin=target/debug/examples/spec.exe

gen() { ffmpeg -hide_banner -loglevel error -y "$@"; }
gen -f lavfi -i testsrc2=size=320x180:rate=30:duration=3 -f lavfi -i sine=frequency=440:sample_rate=48000:duration=3 \
  -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest "$work/clip.mp4"
# black picture, so burned-in text is the only bright pixels
gen -f lavfi -i color=c=black:size=320x180:rate=30:duration=3 -c:v libx264 -pix_fmt yuv420p "$work/black.mp4"
# long enough that a cancel after 1 s lands mid-encode
gen -f lavfi -i testsrc2=size=1280x720:rate=30:duration=20 -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$work/long.mp4"
# HDR-tagged (PQ, BT.2020); the picture itself is ordinary test pattern
# (setparams tags the frames; -color_trc alone on the output is not written)
gen -f lavfi -i testsrc2=size=320x180:rate=30:duration=3 -vf setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc \
  -c:v libx265 -pix_fmt yuv420p10le -x265-params log-level=error "$work/hdr.mp4"
printf '1\n00:00:01,200 --> 00:00:02,000\nHELLO WORLD\n' > "$work/subs.srt"
for f in clip.mp4 black.mp4 long.mp4 hdr.mp4; do
  [ -s "$work/$f" ] || { echo "FAIL could not generate $f"; exit 1; }
done

# spec NAME OVERRIDES_JSON: writes $work/NAME.json from the defaults below plus the overrides
spec() {
  node -e '
    const [name, over, input] = process.argv.slice(1);
    const base = {
      input, segments: [], crop: null, rotate: 0, mirror: false, maxWidth: null, maxHeight: null,
      autoScale: false, fps: null, speed: 1, playback: "forward", deinterlace: false, denoise: false,
      tonemap: false, fadeIn: 0, fadeOut: 0, container: "webm", codec: "vp9",
      rate: { mode: "crf", crf: 40, maxBitrateK: null }, twoPass: false, effort: "fast",
      audio: null, subs: null, title: null, extraArgs: [], outputName: null,
    };
    process.stdout.write(JSON.stringify({ ...base, ...JSON.parse(over) }));
  ' "$1" "$2" "$(native "$work/clip.mp4")" > "$work/$1.json"
}

# probe FILE: "key=value" lines for the first video and audio stream and the format
probe() {
  ffprobe -v error -show_entries stream=codec_type,codec_name,codec_tag_string,width,height,color_transfer:format=duration,size \
    -of default=nw=1 "$1" 2>/dev/null
}
val() { printf '%s\n' "$1" | grep -m1 "^$2=" | cut -d= -f2; }
# near A B TOL: |A-B| <= TOL
near() { awk -v a="$1" -v b="$2" -v t="$3" 'BEGIN { d = a - b; if (d < 0) d = -d; exit !(d <= t) }'; }

pass=0
fail=0
report() { # report NAME OK DETAIL
  if [ "$2" = 1 ]; then pass=$((pass + 1)); echo "PASS $1  $3"; else fail=$((fail + 1)); echo "FAIL $1  $3"; fi
}

# run NAME EXT: encodes $work/NAME.json to $work/NAME.EXT; sets $out, $info, $log
run() {
  out="$work/$1.$2"
  log=$("$bin" "$work/$1.json" "$(native "$out")" 2>&1)
  if [ $? -ne 0 ] || [ ! -s "$out" ]; then
    info=""
    return 1
  fi
  info=$(probe "$out")
  return 0
}
# the JobResult line the example prints
res() { printf '%s\n' "$log" | grep -m1 '^result:' | cut -c9-; }
fail_run() { report "$1" 0 "$(printf '%s' "$log" | tail -n 3 | tr '\n' ' ')"; }

# 1. VP8 + Vorbis, size mode
spec vp8_vorbis '{"codec":"vp8","rate":{"mode":"size","maxBytes":150000},"audio":{"track":0,"bitrateK":64,"volumeDb":0,"normalize":false}}'
if run vp8_vorbis webm; then
  s=$(val "$info" size)
  ok=0; [ "$(probe "$out" | grep -c 'codec_name=vp8\|codec_name=vorbis')" = 2 ] && [ "$s" -le 150000 ] && ok=1
  report vp8_vorbis_size $ok "size=$s limit=150000 $(res)"
else fail_run vp8_vorbis_size; fi

# 2. AV1 in WebM, size mode
spec av1_webm '{"codec":"av1","rate":{"mode":"size","maxBytes":150000}}'
if run av1_webm webm; then
  s=$(val "$info" size)
  ok=0; [ "$(val "$info" codec_name)" = av1 ] && [ "$s" -le 150000 ] && ok=1
  report av1_webm_size $ok "size=$s limit=150000 $(res)"
else fail_run av1_webm_size; fi

# 3. HEVC in MP4, size mode (2 passes through x265 stats)
spec hevc_mp4 '{"container":"mp4","codec":"hevc","rate":{"mode":"size","maxBytes":150000},"audio":{"track":0,"bitrateK":64,"volumeDb":0,"normalize":false}}'
if run hevc_mp4 mp4; then
  s=$(val "$info" size)
  ok=0; [ "$(val "$info" codec_name)" = hevc ] && [ "$(val "$info" codec_tag_string)" = hvc1 ] && [ "$s" -le 150000 ] && ok=1
  report hevc_mp4_2pass_size $ok "size=$s limit=150000 tag=$(val "$info" codec_tag_string) $(res)"
else fail_run hevc_mp4_2pass_size; fi

# 4. H.264, CRF
spec h264_crf '{"container":"mp4","codec":"h264","rate":{"mode":"crf","crf":23,"maxBitrateK":null},"audio":{"track":0,"bitrateK":96,"volumeDb":0,"normalize":false}}'
if run h264_crf mp4; then
  ok=0; [ "$(probe "$out" | grep -c 'codec_name=h264\|codec_name=aac')" = 2 ] && ok=1
  report h264_crf $ok "duration=$(val "$info" duration)"
else fail_run h264_crf; fi

# 5. reverse (audio reversed too)
spec reverse '{"playback":"reverse","audio":{"track":0,"bitrateK":64,"volumeDb":0,"normalize":false}}'
if run reverse webm; then
  d=$(val "$info" duration)
  ok=0; near "$d" 3 0.2 && [ "$(probe "$out" | grep -c 'codec_type=audio')" = 1 ] && ok=1
  report reverse $ok "duration=$d"
else fail_run reverse; fi

# 6. boomerang: twice as long, no audio
spec boomerang '{"playback":"boomerang","audio":{"track":0,"bitrateK":64,"volumeDb":0,"normalize":false}}'
if run boomerang webm; then
  d=$(val "$info" duration)
  ok=0; near "$d" 6 0.2 && [ "$(probe "$out" | grep -c 'codec_type=audio')" = 0 ] && ok=1
  report boomerang $ok "duration=$d"
else fail_run boomerang; fi

# 7. fades, volume, loudness normalize
spec fade_audio '{"fadeIn":0.5,"fadeOut":0.5,"audio":{"track":0,"bitrateK":96,"volumeDb":-6,"normalize":true}}'
if run fade_audio webm; then
  d=$(val "$info" duration)
  ok=0; near "$d" 3 0.2 && [ "$(probe "$out" | grep -c 'codec_name=opus')" = 1 ] && ok=1
  report fade_volume_normalize $ok "duration=$d"
else fail_run fade_volume_normalize; fi

# 8. rotate 90 + mirror: 320x180 becomes 180x320
spec rotate '{"rotate":90,"mirror":true}'
if run rotate webm; then
  w=$(val "$info" width); h=$(val "$info" height)
  ok=0; [ "$w" = 180 ] && [ "$h" = 320 ] && ok=1
  report rotate90_mirror $ok "${w}x${h}"
else fail_run rotate90_mirror; fi

# 9. subtitles from an .srt with a cut from 1 s: the cue at 1.2-2.0 s in the
#    source must show at 0.2-1.0 s in the output. YMAX is the brightest pixel.
spec subs "{\"input\":\"$(native "$work/black.mp4")\",\"segments\":[{\"start\":1,\"end\":3}],\"subs\":{\"kind\":\"file\",\"path\":\"$(native "$work/subs.srt")\"}}"
ymax() {
  ffmpeg -hide_banner -nostdin -ss "$2" -i "$1" -frames:v 1 -vf signalstats,metadata=print:key=lavfi.signalstats.YMAX -f null - 2>&1 |
    grep -o 'YMAX=[0-9.]*' | head -n1 | cut -d= -f2
}
if run subs webm; then
  a=$(ymax "$out" 0.5); b=$(ymax "$out" 1.5)
  ok=0; [ "${a%.*}" -ge 150 ] 2>/dev/null && [ "${b%.*}" -lt 60 ] 2>/dev/null && ok=1
  report subs_srt_cut_at_1s $ok "YMAX at 0.5 s=$a (text expected), at 1.5 s=$b (none expected)"
else fail_run subs_srt_cut_at_1s; fi

# 10. deinterlace + denoise
spec deint '{"deinterlace":true,"denoise":true}'
if run deint webm; then
  report deinterlace_denoise 1 "duration=$(val "$info" duration)"
else fail_run deinterlace_denoise; fi

# 11. speed 2x with audio
spec speed2 '{"speed":2,"audio":{"track":0,"bitrateK":64,"volumeDb":0,"normalize":false}}'
if run speed2 webm; then
  d=$(val "$info" duration)
  # every frame kept: 3 s of 30 fps at 2x is about 90 frames at 60 fps (1.0 kept 30 fps and dropped half)
  frames=$(ffprobe -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 "$out" | tr -d '
 ')
  ok=0; near "$d" 1.5 0.15 && near "${frames:-0}" 90 3 && [ "$(probe "$out" | grep -c 'codec_type=audio')" = 1 ] && ok=1
  report speed_2x_audio $ok "duration=$d frames=$frames"
else fail_run speed_2x_audio; fi

# 12. tone mapping of an HDR-tagged clip
spec tonemap "{\"input\":\"$(native "$work/hdr.mp4")\",\"tonemap\":true}"
if run tonemap webm; then
  t=$(val "$info" color_transfer)
  ok=0; [ "$t" = bt709 ] && ok=1
  report tonemap_hdr $ok "color_transfer=$t"
else fail_run tonemap_hdr; fi

# 13-15. a sound file with a still picture (cover-art mode)
gen -f lavfi -i sine=frequency=440:sample_rate=48000:duration=6 -c:a libmp3lame "$work/sound.mp3"
gen -f lavfi -i testsrc2=size=320x180 -frames:v 1 "$work/pic.png"
gen -i "$work/sound.mp3" -i "$work/pic.png" -map 0 -map 1 -c copy -disposition:v:0 attached_pic "$work/covered.mp3"
gen -f lavfi -i anoisesrc=d=120:c=pink:a=0.3 -ac 2 -c:a libmp3lame -b:a 128k "$work/song.mp3"
snd='"audio":{"track":0,"bitrateK":96,"volumeDb":0,"normalize":false}'
spec cover_png "{\"input\":\"$(native "$work/sound.mp3")\",\"cover\":\"$(native "$work/pic.png")\",\"coverSize\":[320,180],$snd}"
if run cover_png webm; then
  d=$(val "$info" duration)
  ok=0; [ "$(val "$info" width)" = 320 ] && [ "$(probe "$out" | grep -c 'codec_type=audio')" = 1 ] && near "$d" 6 0.1 && ok=1
  report cover_png $ok "$(val "$info" width)x$(val "$info" height) duration=$d"
else fail_run cover_png; fi

spec cover_embedded "{\"input\":\"$(native "$work/covered.mp3")\",$snd}"
if run cover_embedded webm; then
  d=$(val "$info" duration)
  ok=0; [ "$(val "$info" width)" = 320 ] && near "$d" 6 0.1 && ok=1
  report cover_embedded $ok "$(val "$info" width)x$(val "$info" height) duration=$d"
else fail_run cover_embedded; fi

# 120 s at 96 kbps is 1.44 MB of sound alone: the sound bitrate is lowered to fit 1 MiB
# (without that, the file is over the limit: the retries change only the video bitrate)
spec cover_long_size "{\"input\":\"$(native "$work/song.mp3")\",\"cover\":\"$(native "$work/pic.png")\",\"coverSize\":[320,180],\"rate\":{\"mode\":\"size\",\"maxBytes\":1048576},$snd}"
if run cover_long_size webm; then
  s=$(val "$info" size)
  ok=0; [ "$s" -le 1048576 ] && near "$(val "$info" duration)" 120 0.2 && ok=1
  report cover_long_size $ok "size=$s limit=1048576 $(res)"
else fail_run cover_long_size; fi

# 17. splits: a wide 640x360 30 fps clip with sound (4 s) and a tall 360x640 60 fps one without (3 s)
gen -f lavfi -i testsrc2=size=640x360:rate=30:duration=4 -f lavfi -i sine=frequency=330:sample_rate=48000:duration=4 \
  -c:v libx264 -preset ultrafast -pix_fmt yuv420p -c:a aac -shortest "$work/wide.mp4"
gen -f lavfi -i testsrc2=size=360x640:rate=60:duration=3 -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$work/tall.mp4"
wide=$(native "$work/wide.mp4"); tall=$(native "$work/tall.mp4")
aud='"audio":{"track":0,"bitrateK":64,"volumeDb":0,"normalize":false}'
# lay NAME LAYOUT_JSON [EXTRA_JSON]: the two clips (or three for a grid) with that layout
lay() {
  local clips="\"segments\":[{\"start\":0,\"end\":4},{\"start\":0,\"end\":3}],\"segmentSources\":[null,\"$tall\"]"
  case "$2" in *'"grid"'*) clips="\"segments\":[{\"start\":0,\"end\":4},{\"start\":0,\"end\":3},{\"start\":1,\"end\":4}],\"segmentSources\":[null,\"$tall\",null]" ;; esac
  spec "$1" "{\"input\":\"$wide\",$clips,\"layout\":$2,$aud${3:+,$3}}"
}
fps_of() { ffprobe -v error -select_streams v:0 -show_entries stream=r_frame_rate -of csv=p=0 "$1"; }
has_audio() { [ "$(probe "$1" | grep -c 'codec_type=audio')" = 1 ]; }
L='"width":640,"height":360,"fps":30,"gap":0,"gapColor":"#000000","weights":[],"columns":0,"sound":"clip","soundClip":0'

for fill in bars crop blur; do
  lay "seq_$fill" "{\"mode\":\"sequence\",\"fill\":\"$fill\",\"length\":\"shortest\",$L}"
  if run "seq_$fill" webm; then
    d=$(val "$info" duration)
    ok=0; [ "$(val "$info" width)" = 640 ] && [ "$(val "$info" height)" = 360 ] && near "$d" 7 0.15 && [ "$(fps_of "$out")" = 30/1 ] && has_audio "$out" && ok=1
    report "splits_sequence_$fill" $ok "$(val "$info" width)x$(val "$info" height) duration=$d fps=$(fps_of "$out")"
  else fail_run "splits_sequence_$fill"; fi
done

lay side '{"mode":"columns","fill":"bars","length":"shortest",'"$L"'}'
if run side webm; then
  d=$(val "$info" duration)
  ok=0; [ "$(val "$info" width)" = 640 ] && near "$d" 3 0.15 && has_audio "$out" && ok=1
  report splits_side_by_side $ok "$(val "$info" width)x$(val "$info" height) duration=$d"
else fail_run splits_side_by_side; fi

# stacked with a red 16 px gap: the pixel in the middle of the gap is red
lay stacked '{"mode":"rows","fill":"crop","length":"shortest","width":360,"height":640,"fps":30,"gap":16,"gapColor":"#ff0000","weights":[],"columns":0,"sound":"clip","soundClip":0}'
if run stacked webm; then
  px=$(ffmpeg -v error -ss 1 -i "$out" -vf crop=2:2:180:320 -frames:v 1 -f rawvideo -pix_fmt rgb24 - | od -An -tu1 | head -1 | tr -s ' ')
  r=$(echo $px | cut -d' ' -f1); g=$(echo $px | cut -d' ' -f2)
  ok=0; [ "$(val "$info" width)" = 360 ] && [ "$(val "$info" height)" = 640 ] && [ "$r" -gt 180 ] && [ "$g" -lt 80 ] && ok=1
  report splits_stacked_gap $ok "$(val "$info" width)x$(val "$info" height) gap pixel rgb=$px"
else fail_run splits_stacked_gap; fi

lay grid3 '{"mode":"grid","fill":"bars","length":"shortest",'"$L"'}'
if run grid3 webm; then
  d=$(val "$info" duration)
  ok=0; [ "$(val "$info" width)" = 640 ] && near "$d" 3 0.15 && ok=1
  report splits_grid_3_of_4 $ok "$(val "$info" width)x$(val "$info" height) duration=$d"
else fail_run splits_grid_3_of_4; fi

# longest: the 3 s clip holds its last frame up to 4 s
lay longest '{"mode":"columns","fill":"bars","length":"longest",'"$L"'}'
if run longest webm; then
  d=$(val "$info" duration)
  ok=0; near "$d" 4 0.15 && ok=1
  report splits_longest_holds $ok "duration=$d"
else fail_run splits_longest_holds; fi

# mix with a silent clip: sound from the one that has it
lay mix '{"mode":"columns","fill":"bars","length":"longest","width":640,"height":360,"fps":30,"gap":0,"gapColor":"#000000","weights":[],"columns":0,"sound":"mix","soundClip":0}'
if run mix webm; then
  d=$(val "$info" duration)
  ok=0; has_audio "$out" && near "$d" 4 0.15 && ok=1
  report splits_mix_with_silent $ok "duration=$d audio=$(probe "$out" | grep -c 'codec_type=audio')"
else fail_run splits_mix_with_silent; fi

lay sized '{"mode":"columns","fill":"blur","length":"shortest",'"$L"'}' '"rate":{"mode":"size","maxBytes":300000},"twoPass":true'
if run sized webm; then
  s=$(val "$info" size)
  ok=0; [ "$s" -le 300000 ] && ok=1
  report splits_size_limit $ok "size=$s limit=300000 $(res)"
else fail_run splits_size_limit; fi

# a picture as the first clip, beside the wide video: it lasts as long as the video
# (its range is 0..4 although the file has no length), sound comes from clip 2, and
# its crop (the left half, red) is used
gen -f lavfi -i "color=c=red:size=200x400,drawbox=x=100:y=0:w=100:h=400:color=blue:t=fill" -frames:v 1 "$work/pic.png"
gen -i "$work/pic.png" -frames:v 1 "$work/pic.jpg"
for ext in png jpg; do
  pic=$(native "$work/pic.$ext")
  spec "pic_$ext" "{\"input\":\"$pic\",\"segments\":[{\"start\":0,\"end\":4},{\"start\":0,\"end\":4}],\"segmentSources\":[null,\"$wide\"],\"layout\":{\"mode\":\"columns\",\"fill\":\"crop\",\"length\":\"shortest\",\"width\":640,\"height\":360,\"fps\":30,\"gap\":0,\"gapColor\":\"#000000\",\"weights\":[],\"columns\":0,\"sound\":\"clip\",\"soundClip\":1,\"crops\":[{\"x\":0,\"y\":0,\"w\":100,\"h\":400},null]},$aud}"
  if run "pic_$ext" webm; then
    d=$(val "$info" duration)
    px=$(ffmpeg -v error -ss 2 -i "$out" -vf crop=2:2:300:180 -frames:v 1 -f rawvideo -pix_fmt rgb24 - | od -An -tu1 | head -1 | tr -s ' ')
    r=$(echo $px | cut -d' ' -f1); b=$(echo $px | cut -d' ' -f3)
    ok=0; near "$d" 4 0.15 && has_audio "$out" && [ "$r" -gt 150 ] && [ "$b" -lt 100 ] && ok=1
    report "splits_picture_first_$ext" $ok "$(val "$info" width)x$(val "$info" height) duration=$d crop pixel rgb=$px"
  else fail_run "splits_picture_first_$ext"; fi
done
# one after another: the picture for 2 s, then the 4 s video
spec pic_seq "{\"input\":\"$(native "$work/pic.jpg")\",\"segments\":[{\"start\":0,\"end\":2},{\"start\":0,\"end\":4}],\"segmentSources\":[null,\"$wide\"],\"layout\":{\"mode\":\"sequence\",\"fill\":\"bars\",\"length\":\"shortest\",$L},$aud}"
if run pic_seq webm; then
  d=$(val "$info" duration)
  ok=0; near "$d" 6 0.15 && has_audio "$out" && ok=1
  report splits_picture_then_video $ok "duration=$d"
else fail_run splits_picture_then_video; fi

# 18. speed ramp: 0.5x at 0 s to 2x at 4 s on a 6 s clip with sound; the output is
# ln(4)/0.375 + 1 = 4.697 s, and the sound ends with the picture (also reversed, and
# over two ranges in reverse order: 3-6 then 0-2 with points 1 s 0.5x, 5 s 2x = 5.246 s)
gen -f lavfi -i testsrc2=size=320x180:rate=30:duration=6 -f lavfi -i sine=frequency=440:sample_rate=48000:duration=6   -c:v libx264 -preset ultrafast -pix_fmt yuv420p -c:a aac -shortest "$work/six.mp4"
last_pts() { ffprobe -v error -select_streams "$2:0" -show_entries packet=pts_time -of csv=p=0 "$1" | tail -1 | cut -d, -f1; }
ramp_case() { # NAME EXPECTED OVERRIDES
  node -e '
    const [name, over, input] = process.argv.slice(1);
    const base = {
      input, segments: [], crop: null, rotate: 0, mirror: false, maxWidth: null, maxHeight: null,
      autoScale: false, fps: null, speed: 1, playback: "forward", deinterlace: false, denoise: false,
      tonemap: false, fadeIn: 0, fadeOut: 0, container: "webm", codec: "vp9",
      rate: { mode: "crf", crf: 40, maxBitrateK: null }, twoPass: false, effort: "fast",
      audio: { track: 0, bitrateK: 64, volumeDb: 0, normalize: false }, subs: null, title: null, extraArgs: [], outputName: null,
    };
    process.stdout.write(JSON.stringify({ ...base, ...JSON.parse(over) }));
  ' "$1" "$3" "$(native "$work/six.mp4")" > "$work/$1.json"
  if run "$1" webm; then
    d=$(val "$info" duration); v=$(last_pts "$out" v); a=$(last_pts "$out" a)
    ok=0; near "$d" "$2" 0.05 && near "$v" "$a" 0.1 && near "$a" "$2" 0.1 && ok=1
    report "$1" $ok "duration=$d (expected $2) video ends $v, sound ends $a"
  else fail_run "$1"; fi
}
R='"speedRamp":[{"t":0,"speed":0.5},{"t":4,"speed":2}]'
ramp_case speed_ramp 4.697 "{$R,\"speed\":3}"
ramp_case speed_ramp_reverse 4.697 "{$R,\"playback\":\"reverse\"}"
ramp_case speed_ramp_two_ranges 5.246 '{"segments":[{"start":3,"end":6},{"start":0,"end":2}],"speedRamp":[{"t":1,"speed":0.5},{"t":5,"speed":2}]}'

# 16. cancel during pass 1: nothing may be left behind
spec cancel "{\"input\":\"$(native "$work/long.mp4")\",\"effort\":\"best\",\"rate\":{\"mode\":\"size\",\"maxBytes\":4194304}}"
log=$("$bin" "$work/cancel.json" "$(native "$work/cancel.webm")" --cancel-after 1.5 2>&1)
left=$(printf '%s\n' "$log" | grep -c '^leftover:')
ok=0; printf '%s\n' "$log" | grep -q '^canceled$' && [ "$left" = 0 ] && [ ! -e "$work/cancel.webm" ] && ok=1
report cancel_mid_encode $ok "$(printf '%s' "$log" | tr '\n' ' ')"

echo "$pass passed, $fail failed"
[ "$fail" = 0 ]
