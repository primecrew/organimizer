#!/usr/bin/env python3
"""Write THIRD-PARTY-NOTICES.txt for the Windows build of Organimizer.

Lists every crate compiled into Organimizer.exe (x86_64-pc-windows-gnu,
default features; build scripts and proc-macros included, dev-only crates
not) with its license, and reproduces each crate's own license / notice
file(s) so copyright lines are kept. The licenses of the fonts embedded by
epaint_default_fonts are included in full.

Usage: scripts/gen-third-party-notices.py OUTPUT   (run from the repo)
"""
import json, os, re, subprocess, sys
from pathlib import Path

out = Path(sys.argv[1])
meta = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--format-version", "1", "--locked",
     "--filter-platform", "x86_64-pc-windows-gnu"]))
pk = {p["id"]: p for p in meta["packages"]}
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
root = meta["resolve"]["root"]
seen, stack = set(), [root]
while stack:
    i = stack.pop()
    if i in seen:
        continue
    seen.add(i)
    for d in nodes[i]["deps"]:
        if any(k["kind"] in (None, "build") for k in d["dep_kinds"]):
            stack.append(d["pkg"])
seen.discard(root)
crates = sorted((pk[i] for i in seen), key=lambda p: (p["name"], p["version"]))

LIC_RE = re.compile(r"^(licen[cs]e|copying|notice|copyright|unlicense|ofl|ufl)", re.I)
def license_files(p):
    d = Path(p["manifest_path"]).parent
    files = []
    for f in sorted(d.iterdir()):
        if f.is_file() and LIC_RE.match(f.name):
            files.append(f)
    lf = p.get("license_file")
    if lf:
        f = d / lf
        if f.is_file() and f not in files:
            files.append(f)
    return files

def pick(files, spdx):
    """For 'A OR B' crates keep the MIT/BSD-style notice when there is one;
    Apache-2.0's full text is printed once at the end."""
    spdx = spdx or ""
    if " OR " in spdx or "/" in spdx:
        mit = [f for f in files if re.search(r"mit", f.name, re.I)]
        if mit:
            return mit + [f for f in files if re.search(r"notice|copyright", f.name, re.I)]
    return [f for f in files if not re.search(r"apache", f.name, re.I)] or files

def read(f):
    return f.read_text(encoding="utf-8", errors="replace").strip("\n").replace("\r\n", "\n")

L = []
L.append("Organimizer - third-party notices")
L.append("=" * 72)
L.append("Organimizer (formerly PhotoSorter) is Copyright (C) 2026")
L.append("PrimeCrew Software and licensed under")
L.append("the GNU General Public License v3.0 or later (see LICENSE).")
L.append("")
L.append("Organimizer.exe is built with the Rust crates listed below. Each crate")
L.append("stays under its own license; for crates offered under a choice of")
L.append("licenses (\"A OR B\") Organimizer uses them under the first permissive")
L.append("option shown in its notice below. The fonts built into the program are")
L.append("listed in section 3; the optional TMDB lookup and logo in section 6.")
L.append("")
L.append("1. CRATES (%d)" % len(crates))
L.append("-" * 72)
for p in crates:
    L.append("  %-28s %-12s %s" % (p["name"], p["version"], p["license"] or "see notice"))
L.append("")
L.append("2. CRATE LICENSE AND COPYRIGHT NOTICES")
L.append("-" * 72)
need_apache = False
need_mit = False
for p in crates:
    files = license_files(p)
    lic = p["license"] or ""
    if "Apache-2.0" in lic:
        need_apache = True
    L.append("")
    L.append("=== %s %s (%s)" % (p["name"], p["version"], lic or "see below"))
    if p.get("repository"):
        L.append("    %s" % p["repository"])
    chosen = pick(files, lic)
    if p["name"] == "epaint_default_fonts":
        chosen = [f for f in chosen if not f.name.lower().startswith(("ofl", "ufl"))]
    if not chosen:
        authors = ", ".join(p.get("authors") or []) or "the %s authors" % p["name"]
        L.append("    Copyright (c) %s" % authors)
        if "MIT" in lic:
            L.append("    Used under the MIT License; the crate package has no license file,")
            L.append("    so the standard MIT text is in section 5.")
            need_mit = True
        elif "Apache-2.0" in lic:
            L.append("    Licensed under the Apache License 2.0 (full text in section 4).")
    for f in chosen:
        L.append("--- %s" % f.name)
        L.append(read(f))

L.append("")
L.append("3. FONTS EMBEDDED IN Organimizer.exe (from epaint_default_fonts)")
L.append("-" * 72)
fonts = next((p for p in crates if p["name"] == "epaint_default_fonts"), None)
if fonts:
    fd = Path(fonts["manifest_path"]).parent / "fonts"
    L.append("  Ubuntu-Light.ttf      Ubuntu Font Licence 1.0 (UFL.txt below)")
    L.append("  NotoEmoji-Regular.ttf SIL Open Font License 1.1 (OFL.txt below)")
    L.append("  Hack-Regular.ttf      MIT / Bitstream Vera (Hack-Regular.txt below)")
    L.append("  emoji-icon-font.ttf   MIT (emoji-icon-font-mit-license.txt below)")
    L.append("  egui-icons.ttf        see egui-icons.txt below")
    for name in ["UFL.txt", "OFL.txt", "Hack-Regular.txt", "emoji-icon-font-mit-license.txt", "egui-icons.txt"]:
        f = fd / name
        if f.is_file():
            L.append("")
            L.append("--- fonts/%s" % name)
            L.append(read(f))

if need_apache:
    L.append("")
    L.append("4. APACHE LICENSE 2.0 (full text)")
    L.append("-" * 72)
    apache = None
    for p in crates:
        for f in license_files(p):
            if re.search(r"apache", f.name, re.I) and "TERMS AND CONDITIONS" in read(f):
                apache = read(f)
                break
        if apache:
            break
    L.append(apache or "See https://www.apache.org/licenses/LICENSE-2.0")

if need_mit:
    L.append("")
    L.append("5. MIT LICENSE (standard text, for crates listed above without their own file)")
    L.append("-" * 72)
    L.append("""Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.""")

L.append("")
L.append("6. TMDB (THE MOVIE DATABASE)")
L.append("-" * 72)
L.append("""This product uses the TMDB API but is not endorsed or certified by TMDB.

The optional title lookup on the Plex Media tab (off by default) uses the
TMDB API with the user's own free API key, under TMDB's API Terms of Use
(https://www.themoviedb.org/api-terms-of-use). Movie and TV data shown
from a lookup comes from TMDB, https://www.themoviedb.org.

The TMDB logo shown in Help > About (assets/tmdb/) is TMDB's approved
"primary short" logo, used unchanged for attribution as TMDB's logo
guidelines ask (https://www.themoviedb.org/about/logos-attribution).
It is a trademark of TMDB and is not covered by this program's GPL
license.""")

text = "\n".join(L) + "\n"
home = os.path.expanduser("~")
for bad in [home, os.environ.get("CARGO_HOME", "/nonexistent-x"), str(Path.cwd())]:
    assert bad not in text, "local path in notices"
out.write_text(text.replace("\n", "\r\n"), encoding="utf-8")
print("wrote %s (%d crates, %d lines)" % (out, len(crates), text.count("\n")))
