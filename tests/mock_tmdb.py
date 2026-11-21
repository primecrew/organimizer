#!/usr/bin/env python3
"""Local mock of the few TMDB v3 endpoints Organimizer uses, for the
automated Windows UI test (Wine). Serves the hand-written fixtures in
tests/fixtures/tmdb and generated JPEG posters. Never talks to the internet.

    python3 tests/mock_tmdb.py 8799

Start Organimizer with the hidden debug overrides (accepted for
http://127.0.0.1 / http://localhost only, see tmdb::api_base):

    ORGANIMIZER_TMDB_API=http://127.0.0.1:8799/3
    ORGANIMIZER_TMDB_IMAGES=http://127.0.0.1:8799/t/p

and a made-up 32-hex key in Organimizer.ini (tmdb_key); the mock only checks
that some key is sent, so no real key is ever needed or used.
"""
import json, os, subprocess, sys, tempfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

HERE = os.path.dirname(os.path.abspath(__file__))
FIX = os.path.join(HERE, "fixtures", "tmdb")
POSTERS = {"/alien-poster.jpg": "#1d2b3a", "/alien3-poster.jpg": "#3a1d1d", "/expanse-poster.jpg": "#123d2e"}
TMP = tempfile.mkdtemp(prefix="mock-tmdb-")

def poster(path):
    out = os.path.join(TMP, path.strip("/"))
    if not os.path.exists(out):
        label = path.strip("/").split("-")[0].upper()
        subprocess.run(["convert", "-size", "92x138", "xc:" + POSTERS[path], "-fill", "white", "-gravity", "center",
                        "-pointsize", "14", "-annotate", "0", label, "-quality", "85", out], check=True)
    with open(out, "rb") as f:
        return f.read()

def fixture(name):
    with open(os.path.join(FIX, name), "rb") as f:
        return f.read()

class H(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        # log the path only, never the query (it carries the key)
        code = args[1] if len(args) > 1 else "-"
        sys.stderr.write("mock-tmdb: %s %s %s\n" % (self.command, urlparse(self.path).path, code))
        sys.stderr.flush()

    def send(self, code, body, ctype="application/json"):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        u = urlparse(self.path)
        q = parse_qs(u.query)
        p = u.path
        if p.startswith("/t/p/"):
            name = "/" + p.split("/", 4)[-1]
            return self.send(200, poster(name), "image/jpeg") if name in POSTERS else self.send(404, b"")
        if "api_key" not in q and not self.headers.get("Authorization", "").startswith("Bearer "):
            return self.send(401, b'{"status_code":7,"status_message":"Invalid API key: You must be granted a valid key."}')
        query = q.get("query", [""])[0].lower()
        routes = {
            "/3/movie/348": "movie_348.json",
            "/3/tv/63639": "tv_63639.json",
            "/3/tv/63639/season/2": "tv_63639_season_2.json",
            "/3/tv/63639/season/3": "tv_63639_season_3.json",
        }
        if p == "/3/search/movie":
            return self.send(200, fixture("search_movie_alien.json" if "alien" in query else "search_empty.json"))
        if p == "/3/search/tv":
            return self.send(200, fixture("search_tv_expanse.json" if "expanse" in query else "search_empty.json"))
        if p in routes:
            return self.send(200, fixture(routes[p]))
        return self.send(404, b'{"status_code":34,"status_message":"The resource you requested could not be found."}')

if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8799
    ThreadingHTTPServer(("127.0.0.1", port), H).serve_forever()
