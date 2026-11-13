#!/usr/bin/env bash
# Deploy qpc-near to NEAR Testnet.
#
# Safe default: build and print the exact deployment plan only.
# To submit the deployment transaction, pass --execute explicitly.
# This deploys the current policy prototype; it does not add an in-contract
# ML-KEM/ML-DSA/SLH-DSA verifier or make cross-chain signing post-quantum.

set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NETWORK="testnet"
CONTRACT_ID="${CONTRACT_ID:-}"
DEPLOYER_ID="${DEPLOYER_ID:-}"
OWNER_ID="${OWNER_ID:-}"
EXECUTE=0

usage() {
  cat <<'USAGE'
Usage:
  CONTRACT_ID=qpc-demo.testnet DEPLOYER_ID=deployer.testnet \
    ./script/near/deploy-testnet.sh

  CONTRACT_ID=qpc-demo.testnet DEPLOYER_ID=deployer.testnet \
    OWNER_ID=owner.testnet ./script/near/deploy-testnet.sh --execute

Environment:
  CONTRACT_ID  Existing NEAR Testnet account that will host the contract.
  DEPLOYER_ID  Testnet account whose key signs and pays for deployment.
  OWNER_ID     Owner passed to QpcNear::new; defaults to DEPLOYER_ID.

Options:
  --execute    Submit the deploy + init transaction. Without this flag the
               script only builds the WASM and prints the command.
  --help       Show this help.

The script never creates accounts, imports keys, changes NEAR CLI config, or
executes on mainnet. Configure access keys with near-cli separately.
USAGE
}

fail() {
  echo "deploy-testnet: $*" >&2
  exit 1
}

while (($# > 0)); do
  case "$1" in
    --execute)
      EXECUTE=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

[[ -n "$CONTRACT_ID" ]] || fail "CONTRACT_ID is required"
[[ -n "$DEPLOYER_ID" ]] || fail "DEPLOYER_ID is required"
OWNER_ID="${OWNER_ID:-$DEPLOYER_ID}"

is_testnet_account() {
  [[ "$1" =~ ^[a-z0-9][a-z0-9._-]*\.testnet$ ]] && [[ "$1" != *..* ]]
}

is_testnet_account "$CONTRACT_ID" || fail "CONTRACT_ID must be a valid .testnet account"
is_testnet_account "$DEPLOYER_ID" || fail "DEPLOYER_ID must be a valid .testnet account"
is_testnet_account "$OWNER_ID" || fail "OWNER_ID must be a valid .testnet account"

command -v cargo >/dev/null 2>&1 || fail "cargo is required to build qpc-near"
command -v near >/dev/null 2>&1 || fail "near-cli-rs is required; install near-cli-rs and configure a testnet key"

WASM_PATH="$ROOT_DIR/near/target/wasm32-unknown-unknown/release/qpc_near.wasm"

echo "Building qpc-near for NEAR WASM..."
cargo build \
  --manifest-path "$ROOT_DIR/near/Cargo.toml" \
  --package qpc-near \
  --release \
  --target wasm32-unknown-unknown

[[ -s "$WASM_PATH" ]] || fail "WASM artifact not found at $WASM_PATH"

WASM_SHA256="$(sha256sum "$WASM_PATH" | awk '{print $1}')"
INIT_ARGS="{\"owner_id\":\"$OWNER_ID\"}"

cat <<PLAN

QPC NEAR Testnet deployment plan
  network:     $NETWORK
  contract:    $CONTRACT_ID
  deployer:    $DEPLOYER_ID
  owner:       $OWNER_ID
  wasm:        $WASM_PATH
  sha256:      $WASM_SHA256

The transaction will deploy qpc_near.wasm and call:
  QpcNear::new({"owner_id":"$OWNER_ID"})

PLAN

if ((EXECUTE == 0)); then
  cat <<'DRYRUN'
Dry-run only. No transaction was submitted.
Review the contract ID, deployer, owner, and WASM hash, then rerun with --execute.
DRYRUN
  exit 0
fi

cat <<CONFIRM
About to submit a NEAR Testnet transaction using near-cli.
This operation changes code and initializes the contract account.
CONFIRM

near contract deploy "$CONTRACT_ID" \
  use-file "$WASM_PATH" \
  with-init-call new \
  json-args "$INIT_ARGS" \
  prepaid-gas '100 Tgas' \
  attached-deposit '0 NEAR' \
  network-config "$NETWORK" \
  sign-as "$DEPLOYER_ID" \
  sign-with-keychain \
  send

echo "Deployment transaction submitted. Verify the receipt and state with:"
echo "  near contract call-function as-read-only $CONTRACT_ID get_policy json-args '{\"policy_id\":\"missing\"}' network-config $NETWORK now"
echo "  near contract view-storage $CONTRACT_ID all as-text network-config $NETWORK now"
