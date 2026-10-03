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
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"{}")

    def log_message(self, *_):
        pass


def events(envelopes):
    result = []
    for body in envelopes:
        lines = body.splitlines()
        json.loads(lines[0])  # SDK envelope header.
        for index in range(1, len(lines), 2):
            header = json.loads(lines[index])
            assert header["type"] == "event", header  # No session, attachment, log or trace.
            result.append(json.loads(lines[index + 1]))
    return result


def run(binary, configuration, crash=False, expected=True, dsn=None, cached=False):
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
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        environment = dict(os.environ)
        for key in ("ARINN_SENTRY_DSN", "SENTRY_DSN", "ARINN_TELEMETRY_CONSENT",
                    "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
            environment.pop(key, None)
        environment.update({
            "ARINN_SENTRY_DSN": dsn if dsn is not None else f"http://publicfixture@127.0.0.1:{server.server_port}/1",
            "XDG_CONFIG_HOME": str(root / "config"),
            "XDG_CACHE_HOME": str(root / "cache"),
            "HOME": str(root / "home"),
            "NO_PROXY": "127.0.0.1,::1",
            "QT_QPA_PLATFORM": "offscreen",
        })
        # The fixture's synthetic crash must not create a core containing test data.
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        command = [str(binary)] + (["crash"] if crash else [])
        process = subprocess.run(command, env=environment, capture_output=True, timeout=20)
        time.sleep(0.1)
        server.shutdown()
        thread.join()
        server.server_close()
        captured = events(server.envelopes)
        assert (process.returncode != 0) == crash, (command, process.returncode, process.stderr.decode())
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
        assert all(event["tags"]["component"] == component for event in captured)
        assert any(event.get("breadcrumbs") for event in captured), captured
        if crash:
            assert any(event.get("exception") for event in captured), captured
            assert any(exception.get("stacktrace", {}).get("frames")
                       for event in captured for exception in event.get("exception", {}).get("values", [])), captured
        if component == "arinn-shell":
            directory = root / "cache/arinn/sentry-shell"
            assert directory.stat().st_mode & 0o777 == 0o700


def main():
    binaries = [Path(argument).resolve() for argument in sys.argv[1:]]
    assert len(binaries) == 2
    for binary in binaries:
        assert binary.exists(), f"Build the SDK fixture first: {binary}"
        for configuration in (None, "", "[telemetry]\nenabled = false", "[telemetry]\nenabled = 'true'", "broken ["):
            run(binary, configuration, expected=False)
        run(binary, "[telemetry]\nenabled = false", cached=True, expected=False)
        run(binary, "[telemetry]\nenabled = false", crash=True, expected=False)
        for invalid_dsn in ("", "broken", "http://publicfixture@remote.invalid/1", "https://publicfixture:secret@remote.invalid/1"):
            run(binary, "[telemetry]\nenabled = true", expected=False, dsn=invalid_dsn)
        run(binary, "[telemetry]\nenabled = true")
        run(binary, "[telemetry]\nenabled = true", crash=True)
        print(f"PASS {binary.name}: disabled, malformed, enabled, crash and private payload filtering")


if __name__ == "__main__":
    main()
