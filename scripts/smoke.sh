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
  frames=$(ffprobe -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 "$out" | tr -d ' ')
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

# 13. cancel during pass 1: nothing may be left behind
spec cancel "{\"input\":\"$(native "$work/long.mp4")\",\"effort\":\"best\",\"rate\":{\"mode\":\"size\",\"maxBytes\":4194304}}"
log=$("$bin" "$work/cancel.json" "$(native "$work/cancel.webm")" --cancel-after 1.5 2>&1)
left=$(printf '%s\n' "$log" | grep -c '^leftover:')
ok=0; printf '%s\n' "$log" | grep -q '^canceled$' && [ "$left" = 0 ] && [ ! -e "$work/cancel.webm" ] && ok=1
report cancel_mid_encode $ok "$(printf '%s' "$log" | tr '\n' ' ')"

echo "$pass passed, $fail failed"
[ "$fail" = 0 ]
