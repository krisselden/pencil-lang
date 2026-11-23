#!/usr/bin/env python3
"""Serves the repo for the playground, telling the browser to check every file again.

`python3 -m http.server` sends no Cache-Control, so a browser can keep a WASM build from before
the last `scripts/build-web.sh` and run it against newer pages and sheets. Usage:
`scripts/serve.py [port] [--narration DIR]`, 8741 by default, then open http://localhost:8741/web/.

It also keeps takes that `web/prompter.html` records, in lab/narration/out/takes, and which take is
kept for each section of a script, in `<figure>.kept.json` beside them. `--narration DIR` serves
DIR in place of lab/narration/out and keeps takes there, so a test or a try never touches real
recordings. A phone can only record from a page served over https, so to read on a phone, run a
tunnel to this port, as `cloudflared tunnel --url http://localhost:8741`, and open the prompter at
the address it gives. Anyone with that address can read the repo and add takes while the tunnel
runs.
"""

import functools
import http.server
import json
import math
import os
import pathlib
import re
import sys
import threading
import time
import urllib.parse

# Where narration is served from under the repo, and how big one take can be.
NARRATION = "lab/narration/out"
TAKE_BYTES = 64 << 20
NAME = re.compile(r"[\w-]{1,80}")
# Takes and the record of which is kept are each read, changed and written by one request at a
# time, since the server answers each request on a thread of its own.
WRITING = threading.Lock()


def finite(value):
    """JSON with no NaN or Infinity in it, which a browser's JSON.parse refuses."""
    return json.dumps(value, allow_nan=False)


class NoCache(http.server.SimpleHTTPRequestHandler):
    # Pencil's own files are text, so a browser shows them rather than downloading them.
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".pen": "text/plain; charset=utf-8",
        ".style": "text/plain; charset=utf-8",
    }
    # The folder served as lab/narration/out, which holds the timed figures and the takes.
    narration = None

    def end_headers(self):
        self.send_header("Cache-Control", "no-cache")
        if self.command in ("GET", "HEAD"):
            self.send_header("Accept-Ranges", "bytes")
        super().end_headers()

    def translate_path(self, path):
        """A path under lab/narration/out is found in the narration folder. The comparison
        ignores case, since a Mac's disk does, so `Out/` can't reach the repo's own folder when
        another is served in its place."""
        found = super().translate_path(path)
        served = os.path.join(self.directory, NARRATION)
        if found.casefold() == served.casefold() or found.casefold().startswith(served.casefold() + os.sep):
            return str(self.narration / found[len(served) + 1 :]) if len(found) > len(served) else str(self.narration)
        return found

    def reply(self, value, status=200):
        body = finite(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def figure(self, query):
        return re.sub(r"[^\w-]", "", query.get("take", "take"))[:40] or "take"

    def about(self, figure, name):
        """A take of the figure, as its `.json` describes it, or None if there's no such take or
        it can't be described: it isn't JSON, it's another figure's, or a time isn't a number."""
        if not NAME.fullmatch(name):
            return None
        try:
            take = json.loads((self.narration / "takes" / f"{name}.json").read_text(),
                              parse_constant=lambda c: (_ for _ in ()).throw(ValueError(c)))
            finite(take)
        except (OSError, ValueError):
            return None
        times = [take.get(key) for key in ("start", "end")]
        if take.get("figure") != figure or not all(isinstance(t, (int, float)) for t in times):
            return None
        return take

    def section_names(self, figure):
        """The sections the figure's prompt has now."""
        try:
            prompt = json.loads((self.narration / f"{figure}.prompt.json").read_text())
            return {s["name"] for s in prompt.get("sections", [])}
        except (OSError, ValueError, KeyError, TypeError):
            return set()

    def do_GET(self):
        """`GET /takes?take=figure` lists the figure's takes, newest last, and which is kept for
        each section."""
        url = urllib.parse.urlsplit(self.path)
        if url.path != "/takes":
            return self.ranged() or super().do_GET()
        figure = self.figure(dict(urllib.parse.parse_qsl(url.query)))
        takes = self.narration / "takes"
        timed = self.narration / f"{figure}.pen"
        now = timed.read_text() if timed.exists() else None
        listed = []
        for about in sorted(takes.glob(f"{figure}-*.json")):
            if about.name.endswith((".prompt.json", ".kept.json")):
                continue
            take = self.about(figure, about.stem)
            if take is None:
                continue
            pen = about.with_suffix(".pen")
            # Whether it was read with the figure as it's timed now, and so plays back on it.
            take.update(name=about.stem, current=pen.exists() and pen.read_text() == now)
            listed.append(take)
        self.reply({"takes": listed, "kept": self.kept(figure)})

    def ranged(self):
        """Part of a file, for a `Range: bytes=a-b` request. A browser won't seek in audio a
        server can't send part of, so without it the guide and a take played back from partway
        would start from their beginning."""
        found = re.fullmatch(r"bytes=(\d*)-(\d*)", self.headers.get("Range", "").strip())
        path = pathlib.Path(self.translate_path(self.path))
        if not found or not path.is_file() or found[1] == found[2] == "":
            return False
        size = path.stat().st_size
        first, last = found[1], found[2]
        # A range that ends before it starts isn't one, and the whole file is sent (RFC 9110).
        if first and last and int(last) < int(first):
            return False
        start = int(first) if first else max(0, size - int(last))
        end = min(int(last), size - 1) if first and last else size - 1
        if start > end:
            self.send_response(416)
            self.send_header("Content-Range", f"bytes */{size}")
            self.end_headers()
            return True
        with path.open("rb") as file:
            file.seek(start)
            body = file.read(end - start + 1)
        self.send_response(206)
        self.send_header("Content-Type", self.guess_type(str(path)))
        self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Accept-Ranges", "bytes")
        self.end_headers()
        self.wfile.write(body)
        return True

    def kept(self, figure):
        path = self.narration / "takes" / f"{figure}.kept.json"
        try:
            kept = json.loads(path.read_text())
            return {k: v for k, v in kept.items() if isinstance(k, str) and isinstance(v, str)}
        except (OSError, ValueError, AttributeError):
            return {}

    def do_POST(self):
        url = urllib.parse.urlsplit(self.path)
        query = dict(urllib.parse.parse_qsl(url.query))
        if url.path == "/kept":
            return self.keep(query)
        if url.path == "/takes":
            return self.take(query)
        self.send_error(404)

    def keep(self, query):
        """`POST /kept?take=figure&section=name&keep=take` keeps that take for the section, and
        an empty `keep` goes back to the draft's voice for it."""
        figure, section, keep = self.figure(query), query.get("section", ""), query.get("keep", "")
        takes = self.narration / "takes"
        # Only a section the figure has now, and only a take of this figure that holds it.
        if section not in self.section_names(figure):
            self.send_error(400, "no such section")
            return
        about = self.about(figure, keep) if keep else None
        if keep and (about is None or section not in about.get("sections", [])):
            self.send_error(400, "no take of this figure holds that section")
            return
        with WRITING:
            kept = self.kept(figure)
            if keep:
                kept[section] = keep
            else:
                kept.pop(section, None)
            takes.mkdir(parents=True, exist_ok=True)
            # Written whole and then moved into place, so a reload never reads half of it.
            path = takes / f"{figure}.kept.json"
            partial = path.with_suffix(".partial")
            partial.write_text(json.dumps(kept, indent=1, sort_keys=True) + "\n")
            os.replace(partial, path)
        self.reply({"kept": kept})

    def take(self, query):
        """Keeps a take the prompter recorded, so a reader on a phone never handles a file. The
        server names the files: the recording, and beside it a `.json` of where in the figure it
        starts and ends and the sections it holds, from the query string, with the figure and the
        prompter's timings as they were when it was read."""
        length = int(self.headers.get("Content-Length") or 0)
        try:
            start, end = float(query["start"]), float(query["end"])
        except (KeyError, ValueError):
            start = end = math.nan
        if not 0 < length <= TAKE_BYTES or not (math.isfinite(start) and math.isfinite(end) and 0 <= start < end):
            self.send_error(400)
            return
        extension = {"audio/mp4": "m4a", "audio/webm": "webm", "audio/ogg": "ogg",
                     "audio/wav": "wav"}.get(self.headers.get("Content-Type", "").split(";")[0], "audio")
        figure = self.figure(query)
        body = self.rfile.read(length)
        about = {"figure": figure, "start": start, "end": end, "sheets": query.get("sheets", "")}
        # The section it was recorded as, if it holds it, and every section it holds whole, of
        # those the figure has.
        names = self.section_names(figure)
        about["sections"] = [s for s in query.get("sections", "").split(",") if s in names]
        if query.get("section") in about["sections"]:
            about["section"] = query["section"]
        takes = self.narration / "takes"
        with WRITING:
            takes.mkdir(parents=True, exist_ok=True)
            stamp = f"{figure}-{time.strftime('%Y%m%d-%H%M%S')}"
            name, k = stamp, 1
            while (takes / f"{name}.json").exists():
                k += 1
                name = f"{stamp}-{k}"
            about["recording"] = f"{name}.{extension}"
            (takes / about["recording"]).write_bytes(body)
            # The figure and its prompt as they were timed when this was read, since a later
            # retime would move them.
            for suffix in (".pen", ".prompt.json"):
                timed = self.narration / f"{figure}{suffix}"
                if timed.exists():
                    (takes / f"{name}{suffix}").write_text(timed.read_text())
            (takes / f"{name}.json").write_text(json.dumps(about, indent=1) + "\n")
        self.reply({"saved": f"{NARRATION}/takes/{name}.{extension}", "take": {**about, "name": name,
                                                                                "current": True}})


def main():
    args = sys.argv[1:]
    root = pathlib.Path(__file__).resolve().parent.parent
    narration = root / NARRATION
    if "--narration" in args:
        at = args.index("--narration")
        narration = pathlib.Path(args[at + 1]).resolve()
        del args[at : at + 2]
    port = int(args[0]) if args else 8741
    NoCache.narration = narration
    handler = functools.partial(NoCache, directory=str(root))
    with http.server.ThreadingHTTPServer(("127.0.0.1", port), handler) as server:
        print(f"serving {root} at http://localhost:{port}/web/, narration from {narration}")
        server.serve_forever()


if __name__ == "__main__":
    main()
