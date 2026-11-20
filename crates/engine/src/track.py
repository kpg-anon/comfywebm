# comfywebm's tracker for Follow > Track with the Python methods (engine pytrack.rs).
# Written next to the app's Python environment and run by it; not run by hand.
#
#   python track.py --method vittrack|sam2|face --model FILE --w W --h H --box X,Y,BW,BH --fps FPS
#
# stdin: raw BGR frames, W x H each (ffmpeg -pix_fmt bgr24), the first one is
# where the box (in these frames' pixels) was drawn.
# stdout, one JSON object per line:
#   {"device": "cuda", "step": 2}     once, before the first frame is read
#   {"i": 12, "x": .., "y": .., "w": .., "h": .., "s": ..}
#       frame i (0-based, counting every frame read): the centre and size of
#       what is followed, in the frames' pixels, and how sure the tracker is (0..1;
#       below 0.45 for VitTrack, 0.3 for SAM 2, the position is the last good one)
#   {"i": 12, "e": [[x, y], [x, y]], "w": .., "h": .., "s": ..}
#       face: the eyes (left one in the picture first) and the face box's size
#   {"p": 0.42}                       progress, 0..1 of the frames analysed so far
# Errors go to stderr; the exit code is not 0 then.

import argparse
import json
import os
import sys
import tempfile

import numpy as np


def out(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def frames(w, h):
    """Each frame from stdin as an H x W x 3 uint8 array (BGR)."""
    n = w * h * 3
    src = sys.stdin.buffer
    while True:
        buf = src.read(n)
        if len(buf) < n:
            return
        yield np.frombuffer(buf, np.uint8).reshape(h, w, 3)


def vittrack(a):
    import cv2

    p = cv2.TrackerVit_Params()
    p.net = a.model
    t = cv2.TrackerVit_create(p)
    out({"device": "cpu", "step": 1})
    x, y, bw, bh = a.box
    last = (x + bw / 2, y + bh / 2, bw, bh)
    for i, f in enumerate(frames(a.w, a.h)):
        if i == 0:
            t.init(f, (int(x), int(y), int(bw), int(bh)))
            out({"i": 0, "x": last[0], "y": last[1], "w": bw, "h": bh, "s": 1.0})
            continue
        ok, r = t.update(f)
        s = float(t.getTrackingScore())
        if ok and s >= 0.45:
            last = (r[0] + r[2] / 2, r[1] + r[3] / 2, float(r[2]), float(r[3]))
        out({"i": i, "x": last[0], "y": last[1], "w": last[2], "h": last[3], "s": s})
        if i % 15 == 0 and a.frames > 0:
            out({"p": min(1.0, i / a.frames)})


def face(a):
    """OpenCV's YuNet face finder on every frame: the face nearest the last one
    (at first, the middle of --box), and its eyes. Nothing drifts: each frame is
    looked at afresh, so a blink or a fast move does not lose the face. YuNet
    reads a tilted face's eyes as less tilted than they are (26 degrees for 41),
    so each frame is first turned by the tilt found in the frame before, around
    the face, and the eyes are turned back."""
    import math
    import cv2

    det = cv2.FaceDetectorYN.create(a.model, "", (a.w, a.h), 0.6, 0.3, 50)
    out({"device": "cpu", "step": 1})
    x, y, bw, bh = a.box
    near = (x + bw / 2, y + bh / 2)
    last = None
    tilt = 0.0  # the eye line's angle in the last frame, degrees (clockwise on screen)
    for i, f in enumerate(frames(a.w, a.h)):
        if abs(tilt) > 2:
            # turned so the face is upright (OpenCV: positive is counter-clockwise on screen)
            m = cv2.getRotationMatrix2D(near, tilt, 1.0)
            g = cv2.warpAffine(f, m, (a.w, a.h), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REPLICATE)
            back = cv2.invertAffineTransform(m)
        else:
            g, back = f, None

        def unturn(px, py):
            if back is None:
                return [float(px), float(py)]
            return [float(back[0][0] * px + back[0][1] * py + back[0][2]), float(back[1][0] * px + back[1][1] * py + back[1][2])]

        _, faces = det.detect(g)
        best = None
        if faces is not None:
            for fc in faces:
                e = sorted([unturn(fc[4], fc[5]), unturn(fc[6], fc[7])])
                ex, ey = (e[0][0] + e[1][0]) / 2, (e[0][1] + e[1][1]) / 2
                d = (ex - near[0]) ** 2 + (ey - near[1]) ** 2
                if best is None or d < best[0]:
                    best = (d, fc, e)
        # a face far from the last one (another person) is not taken over
        if best is not None and last is not None:
            eye = ((last[0][0] - last[1][0]) ** 2 + (last[0][1] - last[1][1]) ** 2) ** 0.5
            if best[0] ** 0.5 > max(4 * eye, 0.15 * a.w):
                best = None
        if best is not None:
            fc, e = best[1], best[2]
            last = (e[0], e[1], float(fc[2]), float(fc[3]))
            near = ((e[0][0] + e[1][0]) / 2, (e[0][1] + e[1][1]) / 2)
            tilt = math.degrees(math.atan2(e[1][1] - e[0][1], e[1][0] - e[0][0]))
            out({"i": i, "e": e, "w": last[2], "h": last[3], "s": float(fc[14])})
        elif last is not None:
            out({"i": i, "e": [last[0], last[1]], "w": last[2], "h": last[3], "s": 0.0})
        if i % 15 == 0 and a.frames > 0:
            out({"p": min(1.0, i / a.frames)})


def sam2(a):
    import torch
    from PIL import Image
    from sam2.build_sam import build_sam2_video_predictor

    if torch.cuda.is_available():
        device = "cuda"
    elif getattr(torch.backends, "mps", None) and torch.backends.mps.is_available():
        device = "mps"
    else:
        device = "cpu"
    # the camera path is smoothed over tenths of a second: 10 analysed frames a second
    # are enough without a GPU, 30 with one
    want = 30 if device != "cpu" else 10
    step = max(1, round(a.fps / want)) if a.fps > 0 else 1
    out({"device": device, "step": step})
    cfg = "configs/sam2.1/sam2.1_hiera_s.yaml" if "small" in os.path.basename(a.model) else "configs/sam2.1/sam2.1_hiera_t.yaml"
    pred = build_sam2_video_predictor(cfg, a.model, device=device)
    auto = torch.autocast("cuda", dtype=torch.bfloat16) if device == "cuda" else torch.autocast("cpu", enabled=False)

    x, y, bw, bh = a.box
    # the drawn box's centre against the outline's middle on the first frame: the
    # same offset is kept, so the framing stays as drawn
    offset = None
    last = (x + bw / 2, y + bh / 2, bw, bh)
    prompt_mask = None
    chunk = []  # (frame index, frame)
    CHUNK = 48
    done = 0
    total = max(1, a.frames // step)

    def run(chunk):
        nonlocal offset, last, prompt_mask, done
        with tempfile.TemporaryDirectory(prefix="cwm-sam2-") as d:
            for k, (_, f) in enumerate(chunk):
                Image.fromarray(f[:, :, ::-1]).save(os.path.join(d, f"{k:05d}.jpg"), quality=92)
            with torch.inference_mode(), auto:
                st = pred.init_state(video_path=d, offload_video_to_cpu=True)
                if prompt_mask is None:
                    pred.add_new_points_or_box(st, frame_idx=0, obj_id=1, box=np.array([x, y, x + bw, y + bh], dtype=np.float32))
                else:
                    pred.add_new_mask(st, frame_idx=0, obj_id=1, mask=prompt_mask)
                for k, _, logits in pred.propagate_in_video(st):
                    m = (logits[0, 0] > 0).cpu().numpy()
                    i = chunk[k][0]
                    if k == 0 and prompt_mask is not None:
                        continue  # the last frame of the chunk before, already sent
                    s = float(torch.sigmoid(logits[0, 0][logits[0, 0] > 0]).mean()) if m.any() else 0.0
                    ys, xs = np.nonzero(m)
                    if len(xs) > 20 and s >= 0.3:
                        cx, cy = float(xs.mean()), float(ys.mean())
                        if offset is None:
                            offset = (x + bw / 2 - cx, y + bh / 2 - cy)
                        last = (cx + offset[0], cy + offset[1], float(xs.max() - xs.min() + 1), float(ys.max() - ys.min() + 1))
                        prompt_mask = m
                    out({"i": i, "x": last[0], "y": last[1], "w": last[2], "h": last[3], "s": s})
                    done += 1
                    out({"p": min(1.0, done / total)})
                pred.reset_state(st)

    for i, f in enumerate(frames(a.w, a.h)):
        if i % step:
            continue
        chunk.append((i, f.copy()))
        if len(chunk) == CHUNK:
            run(chunk)
            # the next chunk starts on this chunk's last frame, prompted with its outline
            chunk = [chunk[-1]]
    if len(chunk) > 1 or (chunk and prompt_mask is None):
        run(chunk)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--method", required=True, choices=["vittrack", "sam2", "face"])
    ap.add_argument("--model", required=True)
    ap.add_argument("--w", type=int, required=True)
    ap.add_argument("--h", type=int, required=True)
    ap.add_argument("--box", required=True, type=lambda v: [float(q) for q in v.split(",")])
    ap.add_argument("--fps", type=float, default=0)
    ap.add_argument("--frames", type=int, default=0, help="about how many frames will come, for progress")
    a = ap.parse_args()
    {"vittrack": vittrack, "sam2": sam2, "face": face}[a.method](a)


if __name__ == "__main__":
    main()
