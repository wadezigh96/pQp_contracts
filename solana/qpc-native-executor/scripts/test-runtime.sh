#!/usr/bin/env bash
# Offline LiteSVM SBF execution. No wallet, RPC, transaction signing or deploy.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export CARGO_BUILD_JOBS=1
export QPC_SBF_ELF="${QPC_SBF_ELF:-$root/target/sbpf-solana-solana/release/qpc_native_executor.so}"
[[ -f "$QPC_SBF_ELF" ]] || { echo 'Build the SBF ELF first using scripts/build-sbf.sh.' >&2; exit 1; }
# Feature isolation: runtime deps do not enlarge the processor's own lockfile.
cargo test --manifest-path "$root/runtime-tests/Cargo.toml" --locked "$@"
