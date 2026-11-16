#!/usr/bin/env python3
"""Worker benchmark: WER, real-time factor, CPU seconds and peak RSS per cpu-mode.

    python3 engine/scripts/bench.py [--bin PATH] [--modes balanced,low,fast] [--clips kal16,slt] [--threads N] [--speed 1.0] [--runs 1]

Fixtures (needs `flite` and `ffmpeg`) are synthesized once from engine/eval/dictation.txt, one clip per
flite voice, 1 s of silence between lines, and cached in ~/.cache/voicio/bench. TTS audio is
clean, so read WER only as a regression check between configs, not as an absolute number.
Audio is streamed at --speed x real time (1.0 = what the microphone does; partial-decode cadence is wall-clock).
CPU seconds are worker utime+stime for the session only (model load and idle are reported separately).
"""
import argparse, json, os, struct, subprocess, sys, threading, time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CACHE = Path.home() / ".cache/voicio/bench"
TCK = os.sysconf("SC_CLK_TCK")


def make_clip(voice):
    wav, txt = CACHE / f"{voice}.f32", CACHE / f"{voice}.txt"
    if wav.exists() and txt.exists():
        return wav, txt
    CACHE.mkdir(parents=True, exist_ok=True)
    lines = [l.strip() for l in (ROOT / "eval/dictation.txt").read_text().splitlines() if l.strip()]
    pcm = bytearray()
    for i, line in enumerate(lines):
        w = CACHE / f"{voice}-{i}.wav"
        subprocess.run(["flite", "-voice", voice, "-t", line, "-o", str(w)], check=True)
        raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(w), "-ar", "16000", "-ac", "1", "-f", "f32le", "-"],
                             check=True, capture_output=True).stdout
        w.unlink()
        pcm += raw + bytes(4 * 16000)  # 1 s silence
    wav.write_bytes(bytes(pcm))  # noise clips (room, fan) are pre-made .f32 files with an empty .txt
    txt.write_text(" ".join(lines))
    return wav, txt


def words(s):
    return "".join(c if c.isalnum() or c == "'" else " " for c in s.lower()).split()


def wer(ref, hyp):
    r, h = words(ref), words(hyp)
    row = list(range(len(h) + 1))
    for i in range(1, len(r) + 1):
        d, row[0] = row[0], i
        for j in range(1, len(h) + 1):
            above = row[j]
            row[j] = min(d + (r[i - 1] != h[j - 1]), above + 1, row[j - 1] + 1)
            d = above
    return row[-1], len(r)


def cpu(pid):
    f = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return (int(f[11]) + int(f[12])) / TCK


def hwm(pid):
    for l in Path(f"/proc/{pid}/status").read_text().splitlines():
        if l.startswith("VmHWM"):
            return int(l.split()[1]) / 1024


def frame(kind, payload):
    return struct.pack("<I", len(payload) + 1) + bytes([kind]) + payload


def control(obj):
    return frame(0, json.dumps(obj).encode())


def read_frames(stream, out):
    while True:
        h = stream.read(4)
        if len(h) < 4:
            return
        body = stream.read(struct.unpack("<I", h)[0])
        if body[0] == 0:
            out.append((time.time(), json.loads(body[1:])))


def run(binary, mode, clip, extra, speed, idle_secs, no_partials=False):
    cmd = [binary, "worker", "--cpu-mode", mode] + extra
    p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                         env={**os.environ, "VOICIO_PARENT_PID": str(os.getpid())})
    msgs = []
    threading.Thread(target=read_frames, args=(p.stdout, msgs), daemon=True).start()
    t0 = time.time()
    while not any(m["type"] == "ready" for _, m in msgs):
        time.sleep(0.05)
        if p.poll() is not None:
            sys.exit(f"worker died: {msgs}")
    load_cpu, load_s = cpu(p.pid), time.time() - t0
    time.sleep(1)
    i0 = cpu(p.pid)
    time.sleep(idle_secs)
    idle = (cpu(p.pid) - i0) / idle_secs * 100  # % of one core while loaded and idle
    audio = clip[0].read_bytes()
    n = len(audio) // 4
    c0, t0 = cpu(p.pid), time.time()
    p.stdin.write(control({"type": "start", "cleanup": False, "sample_rate": 16000, "session": 1, "partials": not no_partials}))
    step = 1600  # 100 ms chunks like the agent's capture loop
    for k in range(0, n, step):
        p.stdin.write(frame(1, audio[k * 4:(k + step) * 4]))
        p.stdin.flush()
        wait = t0 + (k + step) / 16000 / speed - time.time()
        if wait > 0:
            time.sleep(wait)
    t_end = time.time()
    p.stdin.write(control({"type": "stop"}))
    p.stdin.flush()
    while not any(m["type"] == "done" for _, m in msgs):
        time.sleep(0.02)
    t_done = time.time()
    used, peak = cpu(p.pid) - c0, hwm(p.pid)
    done = next(m for _, m in msgs if m["type"] == "done")
    partials = sum(1 for _, m in msgs if m["type"] == "segment" and not m["is_final"])
    p.stdin.write(control({"type": "shutdown"}))
    p.stdin.flush()
    p.wait(timeout=10)
    e, w = wer(clip[1].read_text(), done["text"])
    return dict(wer=100 * e / max(w, 1), errs=e, cpu=used, cpu_per_audio_min=used / (n / 16000) * 60, rtf=(t_done - t0) / (n / 16000),
                tail_s=t_done - t_end, peak_mb=peak, load_cpu=load_cpu, load_s=load_s, idle_pct=idle,
                partials=partials, text=done["text"])


def main():
    a = argparse.ArgumentParser()
    a.add_argument("--bin", default=str(ROOT / "target/release/voicio-engine"))
    a.add_argument("--modes", default="balanced,low,fast")
    a.add_argument("--clips", default="kal16,slt")
    a.add_argument("--speed", type=float, default=1.0)
    a.add_argument("--runs", type=int, default=1)
    a.add_argument("--idle", type=float, default=5)
    a.add_argument("--extra", default="", help="extra worker flags, e.g. '--threads 3 --max-utterance 8'")
    a.add_argument("--no-partials", action="store_true", help="Start{partials:false}, as when live_text is off")
    a.add_argument("--verbose", action="store_true")
    a = a.parse_args()
    clips = [make_clip(v) for v in a.clips.split(",")]
    audio_s = sum(c[0].stat().st_size / 4 / 16000 for c in clips)
    print(f"{len(clips)} clips, {audio_s:.0f}s audio, speed x{a.speed}, bin {a.bin}")
    print(f"{'mode':9} {'WER%':>6} {'err':>4} {'CPU s':>7} {'CPU/min':>8} {'RTF':>6} {'tail s':>6} {'peakMB':>6} {'idle%':>5} {'parts':>5} {'load cpu s':>10}")
    for mode in a.modes.split(","):
        rows = [[run(a.bin, mode, c, a.extra.split(), a.speed, a.idle, a.no_partials) for c in clips] for _ in range(a.runs)]
        for r in rows:
            tot = lambda k: sum(x[k] for x in r)
            words_total = sum(len(words(c[1].read_text())) for c in clips)
            print(f"{mode:9} {100 * tot('errs') / max(words_total, 1):6.2f} {tot('errs'):4d} {tot('cpu'):7.1f} {tot('cpu') / audio_s * 60:8.1f} "
                  f"{max(x['rtf'] for x in r):6.3f} {max(x['tail_s'] for x in r):6.2f} {max(x['peak_mb'] for x in r):6.0f} "
                  f"{max(x['idle_pct'] for x in r):5.1f} {tot('partials'):5d} {r[0]['load_cpu']:10.1f}")
            if a.verbose:
                for x in r:
                    print("   ", x["text"])


main()
