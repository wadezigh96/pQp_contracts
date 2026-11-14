#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_DIR="${ROOT}/test-artifacts"
ARTIFACT="${ARTIFACT_DIR}/near-zcash-demo-preflight.json"
mkdir -p "${ARTIFACT_DIR}"

python3 - "${ROOT}" "${ARTIFACT}" <<'PY'
import datetime
import json
import pathlib
import re
import shutil
import sys

root = pathlib.Path(sys.argv[1])
artifact = pathlib.Path(sys.argv[2])
# Required toolchains/runtimes are listed separately from optional ways to run
# services. Presence of a binary is not itself proof that a scenario was run.
tools = {name: shutil.which(name) for name in [
    "cargo", "rustc", "near-sandbox", "zcashd", "zebrad", "lightwalletd", "docker"
]}
sandbox_harness_present = (root / "near/sandbox-tests").is_dir()
route_source = (root / "agent-router/src/zcash-route.mjs").read_text()
zcash_sources = list((root / "zcash/qpc-zcash-agent/src").rglob("*.rs"))
backend_implementation_present = any(
    re.search(r"\bimpl(?:\s*<[^>]*>)?\s+ShieldedWalletBackend\s+for\b", path.read_text())
    for path in zcash_sources
)
shielded_route_unimplemented = "NOT_IMPLEMENTED" in route_source
backend_missing = shielded_route_unimplemented or not backend_implementation_present
rust_toolchain_present = bool(tools["cargo"] and tools["rustc"])
near_sandbox_runtime_present = bool(tools["near-sandbox"])
zcash_regtest_node_candidate = "zcashd" if tools["zcashd"] else ("zebrad" if tools["zebrad"] else None)

blockers = []
if not rust_toolchain_present:
    blockers.append({"type": "MISSING_RUST_TOOLCHAIN", "requiredTools": [name for name in ["cargo", "rustc"] if tools[name] is None]})
if not near_sandbox_runtime_present:
    blockers.append({"type": "MISSING_NEAR_SANDBOX_RUNTIME", "requiredTool": "near-sandbox"})
if not sandbox_harness_present:
    blockers.append({"type": "NEAR_SANDBOX_TEST_HARNESS_NOT_IMPLEMENTED", "expectedPath": "near/sandbox-tests/"})
if zcash_regtest_node_candidate is None:
    blockers.append({"type": "MISSING_ZCASH_REGTEST_NODE", "supportedReferenceBinary": "zcashd", "alternativeCandidate": "zebrad (compatibility must be validated)"})
if backend_missing:
    blockers.append({
        "type": "PAYMENT_BACKEND_NOT_IMPLEMENTED",
        "capabilities": [
            "shielded_transaction_construction_and_signing",
            "broadcast_and_actual_transaction_id",
            "wallet_note_sync_and_receive_detection",
            "independent_receipt_and_confirmation_verification",
        ],
    })

near_sandbox_blocked = not (rust_toolchain_present and near_sandbox_runtime_present and sandbox_harness_present)
zcash_regtest_blocked = zcash_regtest_node_candidate is None or backend_missing
result = {
    "schema": "qpc.near-zcash-demo-preflight.v1",
    "generatedAtUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "overallStatus": "BLOCKED" if blockers else "NOT RUN",
    "networkPolicy": {"mainnet": "NEVER_USED", "publicTestnetDeployment": "NOT_AUTHORIZED"},
    "prerequisites": {
        "requiredForRustChecks": {"cargo": tools["cargo"], "rustc": tools["rustc"]},
        "requiredForNearSandbox": {"nearSandbox": tools["near-sandbox"], "integrationHarness": sandbox_harness_present},
        "zcashRegtestNodeCandidates": {"zcashd": tools["zcashd"], "zebradUnvalidatedAlternative": tools["zebrad"]},
        "optionalTools": {"lightwalletd": tools["lightwalletd"], "docker": tools["docker"]},
    },
    "scenarios": {
        "nearSandbox": "BLOCKED" if near_sandbox_blocked else "NOT RUN",
        "zcashRegtest": "BLOCKED" if zcash_regtest_blocked else "NOT RUN",
        "shieldedPaymentBackend": "BLOCKED" if backend_missing else "NOT RUN",
        "nearToZcash": "BLOCKED",
        "zcashToNear": "BLOCKED",
    },
    "blockers": blockers,
    "transactions": [],
    "transactionIds": None,
    "notes": [
        "This preflight is read-only with respect to networks, wallets, and chain state; it writes this local JSON report.",
        "No node was started, no RPC was called, no wallet or key was opened, and no transaction was created or broadcast.",
        "A unit test, mock callback, offline VM result, or txid-shaped string is not network settlement.",
    ],
}
artifact.write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result, indent=2))
print(f"Machine-readable preflight artifact: {artifact}")
PY
