#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise SDK transports and crash hooks in disposable processes on loopback."""
import gzip
import http.server
import json
import os
from pathlib import Path
import resource
import shutil
import subprocess
import sys
import tempfile
import threading
import time


class Endpoint(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers["Content-Length"]))
        if self.headers.get("Content-Encoding") == "gzip":
            body = gzip.decompress(body)
        self.server.envelopes.append(body)
        delay = self.server.delay
        self.server.delay = 0
        if delay:
            time.sleep(delay)
        self.send_response(200)
        self.end_headers()
        try:
            self.wfile.write(b"{}")
        except BrokenPipeError:
            pass

    def log_message(self, *_):
        pass


def events(envelopes):
    result = []
    for body in envelopes:
        lines = body.splitlines()
        json.loads(lines[0])  # SDK envelope header.
        for index in range(1, len(lines), 2):
            header = json.loads(lines[index])
            assert header["type"] in ("event", "transaction", "trace_metric"), header
            payload = json.loads(lines[index + 1])
            if header["type"] == "trace_metric":
                assert header.get("content_type") == "application/vnd.sentry.items.trace-metric+json", header
                assert header["item_count"] == len(payload["items"]), header
                result.extend(("metric", metric) for metric in payload["items"])
            else:
                result.append((header["type"], payload))
    return result


def run(binary, configuration, crash=False, expected=True, dsn=None, cached=False, mode=None):
    with tempfile.TemporaryDirectory(prefix="arinn-sentry-") as temporary:
        root = Path(temporary)
        config = root / "config/arinn"
        config.mkdir(parents=True)
        if configuration is not None:
            (config / "telemetry.toml").write_text(configuration)
        sentinel = root / "cache/arinn/sentry-shell/old-envelope"
        if cached:
            sentinel.parent.mkdir(parents=True)
            sentinel.write_text("private-token-fixture")
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
        server.envelopes = []
        # Keep one raw envelope in flight while the fixture crashes/dumps its queue.
        server.delay = 0.4 if crash and "arinn-telemetry" in binary.name else 0
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        environment = dict(os.environ)
        for key in ("ARINN_SENTRY_DSN", "SENTRY_DSN", "ARINN_TELEMETRY_CONSENT",
                    "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
            environment.pop(key, None)
        environment.update({
            "ARINN_SENTRY_DSN": (dsn(server.server_port) if callable(dsn) else dsn) if dsn is not None else f"http://publicfixture@127.0.0.1:{server.server_port}/1",
            "XDG_CONFIG_HOME": str(root / "config"),
            "XDG_CACHE_HOME": str(root / "cache"),
            "HOME": str(root / "home"),
            "NO_PROXY": "127.0.0.1,::1",
            "QT_QPA_PLATFORM": "offscreen",
        })
        # The fixture's synthetic crash must not create a core containing test data.
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        command = [str(binary)] + ([mode] if mode else (["crash"] if crash else []))
        process = subprocess.run(command, env=environment, capture_output=True, timeout=20)
        time.sleep(0.1)
        # Inspect cache before replay so a transient private buffer is detected.
        for file in (root / "cache").rglob("*"):
            if file.is_file() and file != sentinel:
                data = file.read_bytes()
                for marker in (b"private-token-fixture", b"private.invalid", b"Private tab", b"/home/private-fixture", str(root).encode()):
                    assert marker not in data, (str(file), marker)
        if crash and expected and "arinn-telemetry" in binary.name:
            replay = subprocess.run([str(binary)], env=environment, capture_output=True, timeout=20)
            assert replay.returncode == 0, replay.stderr.decode()
            time.sleep(0.1)
        server.shutdown()
        thread.join()
        server.server_close()
        captured = events(server.envelopes)
        assert (process.returncode != 0) == crash, (command, process.returncode, process.stderr.decode())
        for file in (root / "cache").rglob("*"):
            if file.is_file():
                data = file.read_bytes()
                for marker in (b"private-token-fixture", b"private.invalid", b"Private tab", b"/home/private-fixture", str(root).encode()):
                    if file != sentinel:
                        assert marker not in data, (str(file), marker)
        if mode == "recursive":
            # A recursion fallback skips privacy hooks. It must never be cached.
            assert not any(exception.get("type") == "SIGSEGV" for kind, payload in captured if kind == "event" for exception in payload.get("exception", {}).get("values", [])), captured
            return
        if not expected:
            assert not captured, captured
            if cached:
                assert sentinel.read_text() == "private-token-fixture"
            else:
                assert not (root / "cache").exists(), "Disabled telemetry created a cache"
            return
        assert captured, (command, process.stderr.decode())
        encoded = json.dumps(captured)
        for marker in ("private-token-fixture", "private.invalid", "Private tab", "private-fixture",
                       str(root), "username", "server_name", "context_line", '"user":', '"request":'):
            assert marker not in encoded, (marker, encoded)
        component = "arinn-shell" if "arinn-telemetry" in binary.name else "arinnd"
        event_payloads = [payload for kind, payload in captured if kind == "event"]
        assert all(payload["tags"]["component"] == component for kind, payload in captured if kind != "metric")
        assert all(payload["attributes"]["component"]["value"] == component for kind, payload in captured if kind == "metric")
        assert any(event.get("breadcrumbs") for event in event_payloads), captured
        if not crash:
            assert any(kind == "transaction" for kind, _ in captured), captured
            assert any(kind == "metric" and payload["name"].endswith(".duration") for kind, payload in captured), captured
            assert any(kind == "metric" and payload["type"] == "counter" for kind, payload in captured), captured
        if crash:
            expected_type = "SIGSEGV" if component == "arinn-shell" else "RustPanic"
            assert any(exception.get("type") == expected_type for event in event_payloads
                       for exception in event.get("exception", {}).get("values", [])), captured
            assert any(event.get("exception") for event in event_payloads), captured
            assert any(exception.get("stacktrace", {}).get("frames")
                       for event in event_payloads for exception in event.get("exception", {}).get("values", [])), captured
        if component == "arinn-shell":
            directory = root / "cache/arinn/sentry-shell"
            assert directory.stat().st_mode & 0o777 == 0o700


def verify_hardening(binary):
    source = binary.parents[2] / "_deps/sentry_native-src/src"
    for name, author in (("mpack", "Nicholas Fraser"), ("jsmn", "Serge Zaitsev"), ("stb_sprintf", "Sean Barrett")):
        notice = binary.parent / "licenses" / f"sentry-native-{name}-LICENSE"
        text = notice.read_text()
        assert author in text and "Permission is hereby granted" in text and "SOFTWARE" in text
    patch = Path(__file__).resolve().parents[2] / "observability/native/harden_sdk.py"
    with tempfile.TemporaryDirectory(prefix="arinn-sdk-patch-") as temporary:
        root = Path(temporary)
        for relative in ("backends/sentry_backend_inproc.c", "sentry_transport.c"):
            target = root / "src" / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source / relative, target)
        command = [sys.executable, str(patch), str(root)]
        before = {path: path.read_bytes() for path in (root / "src").rglob("*.c")}
        subprocess.run(command, check=True, capture_output=True)
        subprocess.run(command, check=True, capture_output=True)
        assert all(path.read_bytes() == contents for path, contents in before.items())
        backend = root / "src/backends/sentry_backend_inproc.c"
        backend.write_text(backend.read_text().replace("bool should_handle = !skip_hooks;", "bool unsupported_anchor = false;"))
        rejected = subprocess.run(command, capture_output=True)
        assert rejected.returncode != 0 and b"privacy anchor changed" in rejected.stderr


def main():
    binaries = [Path(argument).resolve() for argument in sys.argv[1:]]
    assert len(binaries) == 2
    for binary in binaries:
        assert binary.exists(), f"Build the SDK fixture first: {binary}"
        if "arinn-telemetry" in binary.name:
            verify_hardening(binary)
        for configuration in (None, "", "[telemetry]\nenabled = false", "[telemetry]\nenabled = 'true'", "[telemetry]\nenabled = true\ndsn = false", "broken ["):
            run(binary, configuration, expected=False)
        run(binary, "[telemetry]\nenabled = false", cached=True, expected=False)
        run(binary, "[telemetry]\nenabled = false", crash=True, expected=False)
        for invalid_dsn in ("", "broken", "\udcff", "http://publicfixture@remote.invalid/1", "https://publicfixture:secret@remote.invalid/1"):
            run(binary, "[telemetry]\nenabled = true", expected=False, dsn=invalid_dsn)
        run(binary, "[telemetry]\nenabled = true", expected=False,
            dsn=lambda port: f"http://public\udcfffixture@127.0.0.1:{port}/1")
        run(binary, "[telemetry]\nenabled = true")
        run(binary, "[telemetry]\nenabled = true", crash=True)
        if "arinn-telemetry" in binary.name:
            run(binary, "[telemetry]\nenabled = true", crash=True, mode="recursive")
        print(f"PASS {binary.name}: disabled, malformed, enabled, crash and private payload filtering")


if __name__ == "__main__":
    main()
