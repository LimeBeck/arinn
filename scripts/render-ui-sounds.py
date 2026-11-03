#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Render Hearth light interface sounds as 48 kHz mono WAV files.

The synthesis matches docs/design/prototypes/hearth-light/hearth-light.html
(Sound). A fixed RNG seed keeps tap pitch jitter and noise offsets stable so
--check reproduces the checked-in files byte-for-byte.
"""

from __future__ import annotations

import argparse
import hashlib
import math
import random
import struct
import sys
import wave
from pathlib import Path

SAMPLE_RATE = 48_000
SEED = 0x48454152  # "HEAR"
SOUNDS = (
    "tap",
    "edge",
    "select",
    "back",
    "open",
    "close",
    "ignite",
    "kindle",
    "error",
)


def _lowpass_onepole(samples: list[float], cutoff_hz: float) -> list[float]:
    if not samples:
        return samples
    rc = 1.0 / (2.0 * math.pi * cutoff_hz)
    dt = 1.0 / SAMPLE_RATE
    alpha = dt / (rc + dt)
    out = [samples[0]]
    for sample in samples[1:]:
        out.append(out[-1] + alpha * (sample - out[-1]))
    return out


def _compress(samples: list[float], threshold: float = 0.125, ratio: float = 3.0) -> list[float]:
    out: list[float] = []
    for sample in samples:
        level = abs(sample)
        if level <= threshold:
            out.append(sample)
            continue
        sign = 1.0 if sample >= 0 else -1.0
        reduced = threshold + (level - threshold) / ratio
        out.append(sign * reduced)
    return out


def _mix(buffer: list[float], offset: int, chunk: list[float], gain: float = 1.0) -> None:
    end = min(len(buffer), offset + len(chunk))
    for i in range(offset, end):
        buffer[i] += chunk[i - offset] * gain


def _env(length: int, attack: float, decay: float, peak: float) -> list[float]:
    attack_samples = max(1, int(attack * SAMPLE_RATE))
    decay_samples = max(1, int(decay * SAMPLE_RATE))
    env = [0.0] * length
    for i in range(min(attack_samples, length)):
        env[i] = peak * (i + 1) / attack_samples
    tail_start = attack_samples
    for i in range(tail_start, length):
        t = (i - tail_start) / decay_samples
        env[i] = peak * math.exp(-8.0 * t)
    return env


def _osc(
    kind: str,
    freq: float,
    start: float,
    attack: float,
    decay: float,
    peak: float,
    freq_end: float | None = None,
) -> list[float]:
    start_i = int(start * SAMPLE_RATE)
    total = int((attack + decay + 0.05) * SAMPLE_RATE)
    chunk = [0.0] * total
    env = _env(total, attack, decay, peak)
    phase = 0.0
    for i in range(total):
        t = i / SAMPLE_RATE
        if freq_end is not None and attack + decay > 0:
            mix = min(1.0, t / (attack + decay * 0.6))
            f = freq * ((freq_end / freq) ** mix)
        else:
            f = freq
        phase += f / SAMPLE_RATE
        wave_sample = math.sin(2.0 * math.pi * phase)
        if kind == "triangle":
            wave_sample = 2.0 * abs(2.0 * (phase - math.floor(phase + 0.5))) - 1.0
        chunk[i] = wave_sample * env[i]
    out = [0.0] * (start_i + total)
    _mix(out, start_i, chunk)
    return out


def _noise(
    rng: random.Random,
    start: float,
    attack: float,
    decay: float,
    peak: float,
    mode: str,
    freq: float,
    q: float,
    freq_end: float | None = None,
) -> list[float]:
    start_i = int(start * SAMPLE_RATE)
    total = int((attack + decay + 0.05) * SAMPLE_RATE)
    chunk = [0.0] * total
    env = _env(total, attack, decay, peak)
    offset = rng.random() * 0.5
    for i in range(total):
        t = (i / SAMPLE_RATE) + offset
        if freq_end is not None and attack + decay > 0:
            mix = min(1.0, t / (attack + decay))
            f = freq * ((freq_end / freq) ** mix)
        else:
            f = freq
        raw = rng.uniform(-1.0, 1.0)
        if mode == "bandpass":
            chunk[i] = raw * math.sin(2.0 * math.pi * f * t) * env[i] * q * 0.35
        elif mode == "lowpass":
            chunk[i] = raw * env[i] * (f / (f + 800.0))
        else:  # highpass
            chunk[i] = raw * env[i] * (800.0 / (800.0 + f))
    out = [0.0] * (start_i + total)
    _mix(out, start_i, chunk)
    return out


def synthesize(name: str, rng: random.Random) -> list[float]:
    t0 = 0.005
    layers: list[list[float]] = []
    if name == "tap":
        j = 1.0 + (rng.random() * 0.06 - 0.03)
        layers.append(_osc("sine", 560 * j, t0, 0.002, 0.06, 0.22, 420 * j))
        layers.append(_noise(rng, t0, 0.001, 0.018, 0.05, "bandpass", 2200, 1.2))
        length = int(0.12 * SAMPLE_RATE)
    elif name == "edge":
        layers.append(_osc("sine", 190, t0, 0.003, 0.09, 0.26, 150))
        layers.append(_noise(rng, t0, 0.001, 0.03, 0.05, "lowpass", 600, 0.7))
        length = int(0.15 * SAMPLE_RATE)
    elif name == "select":
        layers.append(_osc("triangle", 392, t0, 0.004, 0.22, 0.14))
        layers.append(_osc("sine", 392, t0, 0.004, 0.26, 0.10))
        layers.append(_osc("sine", 587.3, t0 + 0.018, 0.004, 0.2, 0.09))
        length = int(0.35 * SAMPLE_RATE)
    elif name == "back":
        layers.append(_osc("triangle", 587.3, t0, 0.004, 0.14, 0.09))
        layers.append(_osc("sine", 392, t0 + 0.02, 0.004, 0.16, 0.08))
        length = int(0.25 * SAMPLE_RATE)
    elif name == "open":
        layers.append(_noise(rng, t0, 0.06, 0.14, 0.2, "bandpass", 500, 1.1, 1600))
        length = int(0.25 * SAMPLE_RATE)
    elif name == "close":
        layers.append(_noise(rng, t0, 0.05, 0.14, 0.17, "bandpass", 1600, 1.1, 500))
        length = int(0.25 * SAMPLE_RATE)
    elif name == "ignite":
        for i, f in enumerate((110, 164.8, 220, 277.2)):
            offset = t0 + i * 0.06
            layers.append(_osc("sine", f, offset, 0.6, 1.2, 0.07))
            layers.append(_osc("triangle", f * 1.003, offset, 0.7, 1.1, 0.03))
        layers.append(_osc("sine", 1320, t0, 0.002, 0.08, 0.05))
        layers.append(_noise(rng, t0, 0.002, 0.05, 0.03, "highpass", 3000, 0.7))
        length = int(2.0 * SAMPLE_RATE)
    elif name == "kindle":
        layers.append(_osc("sine", 220, t0, 0.08, 0.5, 0.09))
        layers.append(_osc("sine", 329.6, t0 + 0.04, 0.08, 0.45, 0.07))
        layers.append(_osc("triangle", 220.6, t0, 0.1, 0.4, 0.03))
        layers.append(_osc("sine", 1320, t0, 0.002, 0.06, 0.035))
        length = int(0.65 * SAMPLE_RATE)
    elif name == "error":
        layers.append(_osc("sine", 329.6, t0, 0.006, 0.16, 0.16))
        layers.append(_osc("sine", 261.6, t0 + 0.13, 0.006, 0.2, 0.16))
        length = int(0.4 * SAMPLE_RATE)
    else:
        raise KeyError(name)

    buffer = [0.0] * length
    for layer in layers:
        _mix(buffer, 0, layer[:length])
    buffer = _lowpass_onepole(buffer, 5200)
    buffer = _compress(buffer)
    peak = max(abs(sample) for sample in buffer) or 1.0
    if peak > 0.99:
        scale = 0.99 / peak
        buffer = [sample * scale for sample in buffer]
    return buffer


def write_wav(path: Path, samples: list[float]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(SAMPLE_RATE)
        frames = bytearray()
        for sample in samples:
            clipped = max(-1.0, min(1.0, sample))
            frames.extend(struct.pack("<h", int(clipped * 32767)))
        handle.writeframes(frames)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def render_all(out_dir: Path) -> dict[str, str]:
    rng = random.Random(SEED)
    hashes: dict[str, str] = {}
    for name in SOUNDS:
        samples = synthesize(name, rng)
        target = out_dir / f"{name}.wav"
        write_wav(target, samples)
        hashes[name] = sha256(target)
    return hashes


def check(out_dir: Path) -> int:
    rng = random.Random(SEED)
    failed = False
    for name in SOUNDS:
        path = out_dir / f"{name}.wav"
        if not path.is_file():
            print(f"missing {path}", file=sys.stderr)
            failed = True
            continue
        expected = sha256(path)
        samples = synthesize(name, rng)
        temp = out_dir / f".{name}.check.wav"
        write_wav(temp, samples)
        actual = sha256(temp)
        temp.unlink(missing_ok=True)
        if expected != actual:
            print(f"{name}.wav does not match synthesis (hash {expected} != {actual})", file=sys.stderr)
            failed = True
    return 1 if failed else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="Verify checked-in WAV files match this script (default: write files)",
    )
    parser.add_argument(
        "output",
        nargs="?",
        type=Path,
        default=Path("apps/arinn-shell/sounds"),
        help="Output directory for WAV files",
    )
    args = parser.parse_args()
    if args.check:
        return check(args.output)
    render_all(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
