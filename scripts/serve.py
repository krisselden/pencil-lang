#!/usr/bin/env python3
"""Serves the repo for the playground, telling the browser to check every file again.

`python3 -m http.server` sends no Cache-Control, so a browser can keep a WASM build from before
the last `scripts/build-web.sh` and run it against newer pages and sheets. Usage:
`scripts/serve.py [port]`, 8741 by default, then open http://localhost:8741/web/.

It also keeps takes that `web/prompter.html` records, in lab/narration/out/takes. A phone can only
record from a page served over https, so to read on a phone, run a tunnel to this port, as
`cloudflared tunnel --url http://localhost:8741`, and open the prompter at the address it gives.
Anyone with that address can read the repo and add takes while the tunnel runs.
"""

import functools
import http.server
import json
import pathlib
import re
import sys
import time
import urllib.parse


class NoCache(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()

    def do_POST(self):
        """Keeps a take the prompter recorded, so a reader on a phone never handles a file. Only
        `POST /takes` is accepted, and the server names the files: the recording, and beside it a
        `.json` of where in the figure it starts, from the query string."""
        url = urllib.parse.urlsplit(self.path)
        length = int(self.headers.get("Content-Length") or 0)
        if url.path != "/takes" or not 0 < length <= TAKE_BYTES:
            self.send_error(400 if url.path == "/takes" else 404)
            return
        query = dict(urllib.parse.parse_qsl(url.query))
        extension = {"audio/mp4": "m4a", "audio/webm": "webm", "audio/ogg": "ogg"}.get(
            self.headers.get("Content-Type", "").split(";")[0], "audio")
        figure = re.sub(r"[^\w-]", "", query.get("take", "take"))[:40] or "take"
        stamp = time.strftime("%Y%m%d-%H%M%S")
        takes = pathlib.Path(self.directory, TAKES)
        takes.mkdir(parents=True, exist_ok=True)
        name = f"{figure}-{stamp}"
        (takes / f"{name}.{extension}").write_bytes(self.rfile.read(length))
        # The figure as it was timed when this was read, since a later retime would move it.
        timed = takes.parent / f"{figure}.pen"
        if timed.exists():
            (takes / f"{name}.pen").write_text(timed.read_text())
        about = {"figure": figure, "recording": f"{name}.{extension}"}
        for key in ("start", "end"):
            try:
                about[key] = float(query[key])
            except (KeyError, ValueError):
                pass
        about["sheets"] = query.get("sheets", "")
        (takes / f"{name}.json").write_text(json.dumps(about, indent=1) + "\n")
        body = json.dumps({"saved": f"{TAKES}/{name}.{extension}"}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


# Where takes go, under the repo, and how big one can be.
TAKES = "lab/narration/out/takes"
TAKE_BYTES = 64 << 20


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8741
    root = pathlib.Path(__file__).resolve().parent.parent
    handler = functools.partial(NoCache, directory=str(root))
    with http.server.ThreadingHTTPServer(("127.0.0.1", port), handler) as server:
        print(f"serving {root} at http://localhost:{port}/web/")
        server.serve_forever()


if __name__ == "__main__":
    main()
