#!/usr/bin/env bash
# Builds and runs locally only. Does not start a node, sign, deploy or send RPC.
set -euo pipefail
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
contract="$here/../qpc-near-agent-finance"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
wasm_opt="${WASM_OPT:-$here/node_modules/.bin/wasm-opt}"
if [[ ! -x "$wasm_opt" ]]; then
  printf '%s\n' 'Install pinned lowering tool first: npm ci --prefix near/offline-vm --ignore-scripts --no-audit --no-fund' >&2
  exit 1
fi
"$wasm_opt" --version | grep -F 'version 123' >/dev/null || {
  printf '%s\n' 'This check requires Binaryen wasm-opt version 123.' >&2
  exit 1
}
# Disabling features for this crate alone cannot remove memory.copy/fill in
# Rust's precompiled standard library; Binaryen lowers those instructions below.
RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=mvp" cargo build \
  --manifest-path "$contract/Cargo.toml" --locked \
  --target-dir "$contract/target" --target wasm32-unknown-unknown --release
raw="$contract/target/wasm32-unknown-unknown/release/qpc_near_agent_finance.wasm"
compatible="${raw%.wasm}.compatible.wasm"
"$wasm_opt" "$raw" --llvm-memory-copy-fill-lowering -o "$compatible"
printf '%s\n' 'Raw and compatible WASM SHA-256:'
sha256sum "$raw" "$compatible"
# Optional historical red check. Its failure is required and checked, never
# silently ignored. Keep captured error separate from the final green result.
if [[ "${QPC_REPRODUCE_RAW_FAILURE:-0}" == 1 ]]; then
  log="$contract/target/raw-vm-rejection.log"
  if QPC_NEAR_WASM="$raw" cargo test --manifest-path "$here/Cargo.toml" --locked \
      wasm_authorization_is -- --nocapture >"$log" 2>&1; then
    printf '%s\n' 'Raw WASM now passes; reassess whether lowering is still needed.' >&2
    exit 1
  fi
  grep -F 'PrepareError(Deserialization)' "$log" >/dev/null || {
    printf 'Raw check failed for an unexpected reason; inspect %s\n' "$log" >&2
    exit 1
  }
  printf 'Reproduced raw preparation rejection; log: %s\n' "$log"
fi
QPC_NEAR_WASM="$compatible" cargo test --manifest-path "$here/Cargo.toml" --locked -- --nocapture
