#!/usr/bin/env bash
# Official Anza release, pinned by GitHub release asset SHA-256. Linux x86_64.
# Downloads compiler tools only; never invokes deploy/keygen or creates keys.
set -euo pipefail
[[ "$(uname -s)-$(uname -m)" == Linux-x86_64 ]] || { echo 'This pinned installer supports Linux x86_64 only.' >&2; exit 1; }
version=v1.58
sha=7b82eb420b5aebd1e5590db0ae25c523eca06724d4238ae02139690535636c37
destination="${1:-${XDG_CACHE_HOME:-$HOME/.cache}/qpc-solana/$version}"
mkdir -p "$destination"
archive="$destination/platform-tools-linux-x86_64.tar.bz2"
if [[ ! -f "$archive" ]]; then
  curl --fail --location --retry 2 \
    "https://github.com/anza-xyz/platform-tools/releases/download/$version/platform-tools-linux-x86_64.tar.bz2" \
    --output "$archive.partial"
  mv "$archive.partial" "$archive"
fi
printf '%s  %s\n' "$sha" "$archive" | sha256sum --check -
# Release archive contains rust/, llvm/, version.md at its root.
# Extract only after checksum passes.
mkdir -p "$destination/platform-tools"
tar -xjf "$archive" -C "$destination/platform-tools"
printf 'QPC_SBF_TOOLS=%s/platform-tools\n' "$(cd "$destination" && pwd)"
"$destination/platform-tools/rust/bin/rustc" --version
