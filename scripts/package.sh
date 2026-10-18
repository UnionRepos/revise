#!/usr/bin/env bash
# Package a compiled `revise` binary into a release archive + checksum.
#
# Works on Linux, macOS and Windows (Git Bash). All three GitHub runner
# images ship bash, so the release matrix shares this single implementation.
#
# Usage:
#   scripts/package.sh <target-triple>
#
# Environment:
#   VERSION  Version string for the archive name (default: [package].version)
#   OUT_DIR  Directory to write archives into      (default: dist)
#
# Produces:
#   dist/revise-<version>-<target>.tar.gz   (unix targets)
#   dist/revise-<version>-<target>.zip      (windows targets)
#   dist/<archive>.sha256
set -euo pipefail

target="${1:?usage: package.sh <target-triple>}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

version="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
if [ -z "$version" ]; then
  echo "error: could not determine version from Cargo.toml" >&2
  exit 1
fi

dist="${OUT_DIR:-dist}"
mkdir -p "$dist"
dist="$(cd "$dist" && pwd)"

# --- locate the binary -------------------------------------------------------
case "$target" in
  *windows*) exe="revise.exe" ;;
  *)         exe="revise" ;;
esac
bin="target/${target}/release/${exe}"
if [ ! -f "$bin" ]; then
  echo "error: binary not found: $bin" >&2
  echo "       (did the build step run with --target ${target}?)" >&2
  exit 1
fi

# --- stage contents ----------------------------------------------------------
name="revise-${version}-${target}"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

cp "$bin" "$stage/$exe"
chmod +x "$stage/$exe" 2>/dev/null || true

# Ship license/readme when present; tolerate their absence so packaging
# never fails purely for want of docs.
for f in LICENSE LICENSE.md LICENSE.txt README.md; do
  [ -f "$f" ] && cp "$f" "$stage/"
done

# Windows users get the shell-integration wrappers alongside the exe.
case "$target" in
  *windows*)
    [ -d scripts ] && cp scripts/revise.ps1 scripts/revise.bat "$stage/" 2>/dev/null || true
    ;;
esac

# --- strip -------------------------------------------------------------------
# Only strip ELF (linux) binaries. Stripping Mach-O or PE here risks breaking
# signatures/build metadata for no meaningful size win.
case "$target" in
  *linux*)
    strip_cmd=""
    if command -v llvm-strip >/dev/null 2>&1; then
      strip_cmd=llvm-strip
    elif command -v strip >/dev/null 2>&1; then
      strip_cmd=strip
    fi
    if [ -n "$strip_cmd" ]; then
      "$strip_cmd" "$stage/$exe" 2>/dev/null || echo "warning: strip failed, continuing" >&2
    fi
    ;;
esac

# --- archive -----------------------------------------------------------------
case "$target" in
  *windows*)
    archive="$dist/${name}.zip"
    rm -f "$archive"
    if command -v zip >/dev/null 2>&1; then
      ( cd "$stage" && zip -q -r -X "$archive" . )
    elif command -v 7z >/dev/null 2>&1; then
      ( cd "$stage" && 7z a -tzip -bd "$archive" ./* >/dev/null )
    elif command -v powershell >/dev/null 2>&1; then
      powershell -NoProfile -Command \
        "Compress-Archive -Path '${stage}/*' -DestinationPath '${archive}' -Force"
    else
      # Last resort: every GitHub runner image ships a Python interpreter.
      py=""
      for candidate in python3 python; do
        if command -v "$candidate" >/dev/null 2>&1; then py="$candidate"; break; fi
      done
      if [ -z "$py" ]; then
        echo "error: no zip, 7z, powershell or python available to create ${archive}" >&2
        exit 1
      fi
      "$py" - "$stage" "$archive" <<'PY'
import os, sys, zipfile
stage, archive = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zf:
    for name in sorted(os.listdir(stage)):
        path = os.path.join(stage, name)
        if os.path.isfile(path):
            info = zipfile.ZipInfo(name, date_time=(2020, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o755 << 16
            with open(path, "rb") as fh:
                zf.writestr(info, fh.read())
PY
    fi
    ;;
  *)
    archive="$dist/${name}.tar.gz"
    rm -f "$archive"
    # GNU tar can write byte-for-byte reproducible archives; bsdtar (macOS) cannot.
    if tar --version 2>/dev/null | grep -q 'GNU tar'; then
      tar -C "$stage" \
        --sort=name --owner=0 --group=0 --numeric-owner \
        --mtime='UTC 2020-01-01' \
        -czf "$archive" .
    else
      tar -C "$stage" -czf "$archive" .
    fi
    ;;
esac

# --- checksum ----------------------------------------------------------------
checksum() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1"
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1"
  else
    openssl dgst -sha256 -r "$1"
  fi
}

( cd "$dist" && checksum "$(basename "$archive")" > "$(basename "$archive").sha256" )

echo "packaged: $archive"
cat "${archive}.sha256"
