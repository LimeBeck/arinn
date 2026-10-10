#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""#433 spike: play one clip in mpv directly on KWin (baseline) and inside the
nested compositor (embedded), and record frame timing, decoding, CPU and GPU.

    measure.py SPIKE_BINARY CLIP MODE [SECONDS]
"""
import glob, json, os, socket, subprocess, sys, time

spike, clip, mode = sys.argv[1], sys.argv[2], sys.argv[3]
seconds = float(sys.argv[4]) if len(sys.argv) > 4 else 20
run = os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
sock = os.path.join(run, "arinn-spike-mpv.sock")
TICK = os.sysconf("SC_CLK_TCK")


def ticks(pid):
    try:
        with open(f"/proc/{pid}/stat") as f:
            parts = f.read().rsplit(")", 1)[1].split()
        return int(parts[11]) + int(parts[12])
    except OSError:
        return 0


def gpu_busy():
    for path in glob.glob("/sys/class/drm/card*/device/gpu_busy_percent"):
        try:
            return int(open(path).read())
        except (OSError, ValueError):
            pass
    return None


class Ipc:
    def __init__(self, path):
        for _ in range(100):
            try:
                self.s = socket.socket(socket.AF_UNIX)
                self.s.connect(path)
                break
            except OSError:
                time.sleep(0.1)
        else:
            raise SystemExit("mpv IPC did not come up")
        self.buf = b""
        self.n = 0

    def get(self, name):
        self.n += 1
        self.s.sendall(json.dumps({"command": ["get_property", name], "request_id": self.n}).encode() + b"\n")
        while True:
            while b"\n" not in self.buf:
                self.buf += self.s.recv(65536)
            line, self.buf = self.buf.split(b"\n", 1)
            msg = json.loads(line)
            if msg.get("request_id") == self.n:
                return msg.get("data")


PROPS = ["frame-drop-count", "decoder-frame-drop-count", "vo-delayed-frame-count",
         "mistimed-frame-count"]
INFO = ["hwdec-current", "current-gpu-context", "current-vo", "display-fps",
        "estimated-display-fps", "container-fps", "width", "height", "vsync-jitter"]

env = dict(os.environ, XDG_RUNTIME_DIR=run)
host = None
if mode == "embedded":
    host_sock = os.path.join(run, "arinn-player-0")
    if os.path.exists(host_sock):
        os.unlink(host_sock)
    host = subprocess.Popen([spike], env=dict(env, WAYLAND_DISPLAY="wayland-0"),
                            stdout=subprocess.DEVNULL, stderr=open("/tmp/arinn-spike-host.log", "w"))
    for _ in range(100):
        if os.path.exists(host_sock):
            break
        time.sleep(0.1)
    else:
        raise SystemExit("nested compositor socket did not appear")
    player_env = dict(env, WAYLAND_DISPLAY="arinn-player-0")
else:
    player_env = dict(env, WAYLAND_DISPLAY="wayland-0")
player_env.pop("DISPLAY", None)

if os.path.exists(sock):
    os.unlink(sock)
mpv = subprocess.Popen(["mpv", "--fs", "--hwdec=auto-safe", "--no-audio", "--osc=no",
                        "--osd-level=0", "--no-terminal", "--loop-file=inf",
                        f"--input-ipc-server={sock}", clip], env=player_env,
                       stdout=subprocess.DEVNULL, stderr=open("/tmp/arinn-spike-mpv.log", "w"))
ipc = Ipc(sock)
time.sleep(5)  # warm-up: window mapped, decoder and swapchain settled
start = {p: ipc.get(p) for p in PROPS}
c0 = {"mpv": ticks(mpv.pid), "host": ticks(host.pid) if host else 0}
busy, t0 = [], time.monotonic()
while time.monotonic() - t0 < seconds:
    b = gpu_busy()
    if b is not None:
        busy.append(b)
    time.sleep(0.25)
elapsed = time.monotonic() - t0
end = {p: ipc.get(p) for p in PROPS}
info = {p: ipc.get(p) for p in INFO}
c1 = {"mpv": ticks(mpv.pid), "host": ticks(host.pid) if host else 0}
mpv.terminate(); mpv.wait(5)
if host:
    host.terminate(); host.wait(5)
delta = {p: (end[p] or 0) - (start[p] or 0) for p in PROPS}
print(json.dumps({
    "mode": mode, "clip": os.path.basename(clip), "seconds": round(elapsed, 1),
    **info, **{f"d_{k}": v for k, v in delta.items()},
    "cpu_mpv_pct": round(100 * (c1["mpv"] - c0["mpv"]) / TICK / elapsed, 1),
    "cpu_host_pct": round(100 * (c1["host"] - c0["host"]) / TICK / elapsed, 1),
    "gpu_busy_avg": round(sum(busy) / len(busy), 1) if busy else None,
    "gpu_busy_max": max(busy) if busy else None,
}))
