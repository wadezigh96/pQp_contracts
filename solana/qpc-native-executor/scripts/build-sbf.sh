#!/usr/bin/env bash
# Build with official pinned compiler directly. cargo-build-sbf postprocessing
# would generate a deploy keypair; invoking compiler Cargo avoids that entirely.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
tools="${QPC_SBF_TOOLS:-${XDG_CACHE_HOME:-$HOME/.cache}/qpc-solana/v1.58/platform-tools}"
expected='rustc 1.95.0-dev (1e20dd0d5 2026-10-05)'
[[ "$($tools/rust/bin/rustc --version)" == "$expected" ]] || { echo 'Expected official platform-tools v1.58 compiler; run install-sbf-tools.sh.' >&2; exit 1; }
export PATH="$tools/rust/bin:$PATH"
export RUSTC="$tools/rust/bin/rustc"
export CC="$tools/llvm/bin/clang"
export AR="$tools/llvm/bin/llvm-ar"
export CARGO_BUILD_JOBS=1
# Avoid inherited workstation flags changing the SBF build.
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
export CARGO_TARGET_SBPF_SOLANA_SOLANA_RUSTFLAGS='-Zremap-cwd-prefix='
"$tools/rust/bin/cargo" build --manifest-path "$root/Cargo.toml" --locked \
  --target-dir "$root/target" --target sbpf-solana-solana --release --lib
elf="$root/target/sbpf-solana-solana/release/qpc_native_executor.so"
"$tools/llvm/bin/llvm-readelf" --file-header "$elf"
sha256sum "$elf"
printf 'QPC_SBF_ELF=%s\n' "$elf"
