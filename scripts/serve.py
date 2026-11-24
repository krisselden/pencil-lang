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
# A figure's name, which the page checks the same way, so both read the same files.
FIGURE = re.compile(r"[A-Za-z0-9_-]{1,40}")
# Takes and the record of which is kept are each read, changed and written by one request at a
# time, since the server answers each request on a thread of its own.
WRITING = threading.Lock()


def finite(value):
    """JSON with no NaN or Infinity in it, which a browser's JSON.parse refuses."""
    return json.dumps(value, allow_nan=False)


def said(prompt):
    """Each section of a prompt, by name, and the words it says, as the aligner would hear them."""
    out = {}
    try:
        for s in prompt.get("sections", []):
            a, b = s["lines"]
            out[s["name"]] = [w for line in prompt["lines"][a:b] for token, _, _ in line
                              if (w := re.sub(r"[^\w']", "", token.lower().replace("’", "'")))]
    except (KeyError, TypeError, ValueError):
        return {}
    return out


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
        """The figure a request is for, or None, after a 400, for a name the page wouldn't use."""
        figure = query.get("take", "")
        if FIGURE.fullmatch(figure):
            return figure
        self.send_error(400, "a figure's name is letters, digits, _ and -")
        return None

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

    def prompt(self, path):
        try:
            prompt = json.loads(path.read_text())
            return prompt if isinstance(prompt, dict) else {}
        except (OSError, ValueError):
            return {}

    def section_names(self, figure):
        """The sections the figure's prompt has now."""
        return set(said(self.prompt(self.narration / f"{figure}.prompt.json")))

    def held(self, take):
        """The words of each section a take held when it was read, by the name it had then."""
        then = said(self.prompt(self.narration / "takes" / f"{take['name']}.prompt.json"))
        listed = take.get("sections")
        return {n: then[n] for n in listed if n in then} if isinstance(listed, list) else {}

    def holds(self, take, now):
        """The sections a take holds now: each with the words of one it held when it was read.
        A take follows its section's words, not its name, so a section renamed, by an anchor
        added before its first or a first word changed, keeps its takes. A blank line added or
        taken away in a section, or a word changed, leaves a take of it holding nothing there."""
        words = list(self.held(take).values())
        return [n for n, w in now.items() if w in words]

    def opening(self, take, section):
        """A section's first words as the take's prompt writes them, to name what was kept."""
        prompt = self.prompt(self.narration / "takes" / f"{take}.prompt.json")
        try:
            a, b = next(s["lines"] for s in prompt["sections"] if s["name"] == section)
            tokens = [token for line in prompt["lines"][a:b] for token, _, _ in line]
        except (KeyError, StopIteration, TypeError, ValueError):
            return ""
        return " ".join(tokens[:6]) + (" …" if len(tokens) > 6 else "")

    def resolved(self, figure):
        """What's kept, by the section's name now, and what's kept for words the script no longer
        has. Each entry of the record is under the section's name when it was kept, which is only
        a label, and names the take and the section as the take's own prompt names it; it belongs
        to the section that has now the words the take read there, whatever that's called now."""
        now = said(self.prompt(self.narration / f"{figure}.prompt.json"))
        kept, unmatched = {}, []
        # An entry under a section's own name comes first, then the rest, in order.
        record = self.kept(figure)
        for key, entry in sorted(record.items(), key=lambda e: e[0] not in now):
            words = self.read(figure, entry)
            found = [n for n, w in now.items() if w == words] if words else []
            if found:
                kept.setdefault(found[0], entry["take"])
            else:
                unmatched.append({"section": key, "take": entry.get("take", ""),
                                  "words": self.opening(entry.get("take", ""), entry.get("section", ""))})
        return {"kept": kept, "unmatched": unmatched}, record, now

    def read(self, figure, entry):
        """The words a kept take read for the section it was kept for, or None if it can't be
        read: it's gone, or the entry isn't `{"take", "section"}`."""
        if not (isinstance(entry, dict) and isinstance(entry.get("take"), str)
                and isinstance(entry.get("section"), str)):
            return None
        about = self.about(figure, entry["take"])
        return self.held({**about, "name": entry["take"]}).get(entry["section"]) if about else None

    def do_GET(self):
        """`GET /takes?take=figure` lists the figure's takes, newest last, and which is kept for
        each section."""
        url = urllib.parse.urlsplit(self.path)
        if url.path != "/takes":
            return self.ranged() or super().do_GET()
        figure = self.figure(dict(urllib.parse.parse_qsl(url.query)))
        if figure is None:
            return
        takes = self.narration / "takes"
        timed = self.narration / f"{figure}.pen"
        now = timed.read_text() if timed.exists() else None
        words = said(self.prompt(self.narration / f"{figure}.prompt.json"))
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
            take["sections"] = self.holds(take, words)
            listed.append(take)
        self.reply({"takes": listed, **self.resolved(figure)[0]})

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
        self.end_headers()
        self.wfile.write(body)
        return True

    def kept(self, figure):
        path = self.narration / "takes" / f"{figure}.kept.json"
        try:
            kept = json.loads(path.read_text())
            return {k: v for k, v in kept.items() if isinstance(k, str)}
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
        an empty `keep` goes back to the draft's voice for it. An empty `keep` also clears an
        entry kept for words the script no longer has, by the name it was kept under."""
        figure, section, keep = self.figure(query), query.get("section", ""), query.get("keep", "")
        if figure is None:
            return
        takes = self.narration / "takes"
        with WRITING:
            _, record, now = self.resolved(figure)
            # Only a section the figure has now, or an entry to clear, and only a take of this
            # figure that holds the section.
            if section not in now and (keep or section not in record):
                self.send_error(400, "no such section")
                return
            about = self.about(figure, keep) if keep else None
            if keep and (about is None or section not in self.holds({**about, "name": keep}, now)):
                self.send_error(400, "no take of this figure holds that section")
                return
            # Every entry for this section's words goes, whatever it was kept under, so there's
            # one for it.
            if section in now:
                for key, entry in list(record.items()):
                    if key == section or self.read(figure, entry) == now[section]:
                        record.pop(key)
            else:
                record.pop(section, None)
            if keep:
                # Under the section's name now, with the name the take's prompt gives it.
                then = next(n for n, w in self.held({**about, "name": keep}).items() if w == now[section])
                record[section] = {"take": keep, "section": then}
            takes.mkdir(parents=True, exist_ok=True)
            # Written whole and then moved into place, so a reload never reads half of it.
            path = takes / f"{figure}.kept.json"
            partial = path.with_suffix(".partial")
            partial.write_text(json.dumps(record, indent=1, sort_keys=True) + "\n")
            os.replace(partial, path)
            answer = self.resolved(figure)[0]
        self.reply(answer)

    def take(self, query):
        """Keeps a take the prompter recorded, so a reader on a phone never handles a file. The
        server names the files: the recording, and beside it a `.json` of where in the figure it
        starts and ends and the sections it holds, from the query string, with the figure and the
        prompter's timings as they were when it was read."""
        try:
            length = int(self.headers.get("Content-Length") or 0)
        except ValueError:
            length = 0
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
        if figure is None:
            return
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
