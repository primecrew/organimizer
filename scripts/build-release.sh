#!/usr/bin/env bash
# Windows release of PhotoSorter: PhotoSorter.exe (x86_64-pc-windows-gnu,
# static CRT, build-machine paths remapped), the portable zip and the NSIS
# installer. Needs mingw-w64, makensis (NSIS 3) and python3.
# Usage: scripts/build-release.sh            (outputs into dist/)
set -euo pipefail
cd "$(dirname "$0")/.."
SRC="$(pwd)"
CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
SYSROOT="$(rustc --print sysroot)"
TARGET=x86_64-pc-windows-gnu

# ---- identity (keep in sync with src/about.rs and src/settings.rs)
APP_NAME="PhotoSorter"
APP_ID="PhotoSorter"                 # Add/Remove Programs key
APP_DIRNAME="PhotoSorter"            # install folder name (like Encode FFS: no brand level)
APP_EXE="PhotoSorter.exe"
PUBLISHER="PrimeCrew Software"       # brand Start Menu folder, shared with Encode FFS
MARKER="PhotoSorter.installed"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
VERSION4="$VERSION.0"
DOC_DIR="dist/v$VERSION"
grep -q "pub const PUBLISHER: &str = \"$PUBLISHER\"" src/about.rs || { echo "PUBLISHER differs from src/about.rs" >&2; exit 1; }
grep -q "pub const INSTALLED_MARKER: &str = \"$MARKER\"" src/settings.rs || { echo "MARKER differs from src/settings.rs" >&2; exit 1; }
for f in README.txt HELP.md; do [ -f "$DOC_DIR/$f" ] || { echo "missing $DOC_DIR/$f" >&2; exit 1; }; done

# ---- exe. Same flags as the target section of .cargo/config.toml (an env
# var replaces those), plus the path remapping (rustc uses the last matching
# prefix, so the most specific ones come last).
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="\
-C target-feature=+crt-static -C link-arg=-static -C link-arg=-static-libgcc \
--remap-path-prefix=$HOME=home \
--remap-path-prefix=$RUSTUP_HOME=rustup \
--remap-path-prefix=$SYSROOT=rust \
--remap-path-prefix=$CARGO_HOME=cargo \
--remap-path-prefix=$CARGO_HOME/git/checkouts=git \
--remap-path-prefix=$CARGO_HOME/registry/src=crates \
--remap-path-prefix=$SRC=photo-sorter"
cargo build --release --target "$TARGET" --locked
EXE="target/$TARGET/release/$APP_EXE"
if strings -a "$EXE" | grep -qE "$SRC|$HOME/|$CARGO_HOME"; then
  echo "build-machine path found in $EXE" >&2; exit 1
fi
EXE_SHA=$(sha256sum "$EXE" | cut -d' ' -f1)
echo "== $APP_NAME $VERSION  exe sha256 $EXE_SHA"

# ---- docs shipped with both: README (exe SHA filled in), HELP, LICENSE, notices
python3 scripts/gen-third-party-notices.py "$DOC_DIR/THIRD-PARTY-NOTICES.txt"
python3 - "$DOC_DIR/README.txt" "$EXE_SHA" <<'PY'
import re, sys
p, sha = sys.argv[1:3]
s = open(p, encoding="utf-8").read()
s2 = re.sub(r"(SHA-256 \(PhotoSorter\.exe\)\n  )\S+", r"\g<1>" + sha, s)
assert "SHA-256 (PhotoSorter.exe)" in s2
open(p, "w", encoding="utf-8").write(s2)
PY
cp "$EXE" "$DOC_DIR/$APP_EXE"

# ---- portable zip (settings are created next to the exe on first start)
ZIP="dist/$APP_NAME-$VERSION-win64.zip"
rm -f "$ZIP"
python3 - "$ZIP" "$EXE" "$DOC_DIR" <<'PY'
import sys, zipfile
z, exe, d = sys.argv[1:4]
with zipfile.ZipFile(z, "w", zipfile.ZIP_DEFLATED) as f:
    f.write(exe, "PhotoSorter.exe")
    for name in ["README.txt", "HELP.md", "THIRD-PARTY-NOTICES.txt"]:
        f.write(f"{d}/{name}", name)
    f.write("LICENSE", "LICENSE")
PY

# ---- installer
command -v makensis >/dev/null || { echo "makensis not found (apt install nsis)" >&2; exit 1; }
SETUP="dist/$APP_NAME-$VERSION-setup.exe"
makensis -V2 -INPUTCHARSET UTF8 \
  -DAPP_NAME="$APP_NAME" -DAPP_ID="$APP_ID" -DAPP_DIRNAME="$APP_DIRNAME" -DAPP_EXE="$APP_EXE" \
  -DAPP_VERSION="$VERSION" -DVERSION4="$VERSION4" -DPUBLISHER="$PUBLISHER" -DMARKER="$MARKER" \
  -DICON="$SRC/assets/icon.ico" -DEXE_PATH="$SRC/$EXE" -DSRC_DIR="$SRC" -DDOC_DIR="$SRC/$DOC_DIR" \
  -DOUTFILE="$SRC/$SETUP" installer/installer.nsi

( cd dist && sha256sum "$(basename "$ZIP")" "$(basename "$SETUP")" > "$APP_NAME-$VERSION-SHA256SUMS.txt" \
  && sha256sum "v$VERSION/$APP_EXE" >> "$APP_NAME-$VERSION-SHA256SUMS.txt" )
cat "dist/$APP_NAME-$VERSION-SHA256SUMS.txt"
