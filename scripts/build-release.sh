#!/usr/bin/env bash
# Release build of PhotoSorter.exe (x86_64-pc-windows-gnu, static CRT) with
# build-machine paths remapped, so the exe contains no local folder names.
# Usage: scripts/build-release.sh    (run from anywhere; needs mingw-w64)
set -euo pipefail
cd "$(dirname "$0")/.."
SRC="$(pwd)"
CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
SYSROOT="$(rustc --print sysroot)"
# Same flags as the target section of .cargo/config.toml (an env var
# replaces those), plus the path remapping (rustc uses the last matching
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
cargo build --release --target x86_64-pc-windows-gnu --locked
echo "built: target/x86_64-pc-windows-gnu/release/PhotoSorter.exe"
