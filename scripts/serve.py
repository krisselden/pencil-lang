#!/usr/bin/env python3
"""Serves the repo for the playground, telling the browser to check every file again.

`python3 -m http.server` sends no Cache-Control, so a browser can keep a WASM build from before
the last `scripts/build-web.sh` and run it against newer pages and sheets. Usage:
`scripts/serve.py [port]`, 8741 by default, then open http://localhost:8741/web/.
"""

import functools
import http.server
import pathlib
import sys


class NoCache(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8741
    root = pathlib.Path(__file__).resolve().parent.parent
    handler = functools.partial(NoCache, directory=str(root))
    with http.server.ThreadingHTTPServer(("127.0.0.1", port), handler) as server:
        print(f"serving {root} at http://localhost:{port}/web/")
        server.serve_forever()


if __name__ == "__main__":
    main()
