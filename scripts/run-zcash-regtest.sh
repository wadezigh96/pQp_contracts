#!/bin/sh
# Start the isolated local Regtest services, run the real adapter integration, then remove them.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
compose="$root/zcash/regtest/docker-compose.yml"
if [ "$(uname -s)" != Linux ]; then
  echo "This harness requires a compatible Linux host with Docker Compose v2; no services were started." >&2
  exit 2
fi
command -v docker >/dev/null 2>&1 || { echo "Docker is required; no services were started." >&2; exit 2; }
docker compose version >/dev/null 2>&1 || { echo "Docker Compose v2 is required; no services were started." >&2; exit 2; }
work=$(mktemp -d "${TMPDIR:-/tmp}/qpc-zcash-regtest.XXXXXX")
chmod 700 "$work"
project="qpc-regtest-$$"
cleanup() {
  result=$?
  if [ "$result" -ne 0 ]; then
    # Preserve actionable startup/test diagnostics while redacting the generated
    # RPC credentials before they can enter CI logs.
    QPC_REGTEST_DIR="$work" QPC_REGTEST_PROJECT="$project" \
      docker compose -p "$project" -f "$compose" logs --no-color 2>&1 |
      sed -e "s/$user/[REDACTED]/g" -e "s/$password/[REDACTED]/g" >&2 || true
  fi
  QPC_REGTEST_DIR="$work" QPC_REGTEST_PROJECT="$project" docker compose -p "$project" -f "$compose" down --volumes --remove-orphans >/dev/null 2>&1 || true
  rm -rf -- "$work"
  return "$result"
}
trap cleanup EXIT HUP INT TERM
mkdir -p "$work/node" "$work/lightwalletd-data"
chmod 700 "$work/node" "$work/lightwalletd-data"
user=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')
password=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
cat > "$work/zcashd.conf" <<EOF_CONF
i-am-aware-zcashd-will-be-replaced-by-zebrad-and-zallet-in-2025=1
regtest=1
server=1
daemon=0
listen=0
connect=0
dnsseed=0
discover=0
txindex=1
nuparams=5ba81b19:1
nuparams=76b809bb:1
nuparams=2bb40e60:1
nuparams=f5b9230b:1
nuparams=e9ff75a6:1
nuparams=c2d6d0b4:1
nuparams=c8e71055:1
nuparams=4dec4df0:1
nuparams=b4d0d6c2:1
rpcuser=$user
rpcpassword=$password
rpcbind=0.0.0.0
rpcallowip=172.30.8.0/24
EOF_CONF
cat > "$work/lightwalletd.yml" <<EOF_CONF
no-tls-very-insecure: true
log-file: /dev/stdout
rpcuser: $user
rpcpassword: $password
rpchost: zcashd
rpcport: 18232
grpc-bind-addr: 0.0.0.0:9067
data-dir: /var/lib/lightwalletd
EOF_CONF
chmod 600 "$work/zcashd.conf" "$work/lightwalletd.yml"
export QPC_REGTEST_DIR="$work" QPC_REGTEST_PROJECT="$project" QPC_CONTAINER_UID="$(id -u):$(id -g)" QPC_REGTEST_COMPOSE="$compose"
docker compose -p "$project" -f "$compose" up -d --wait
cat > "$work/zcash-cli" <<'EOF_CLI'
#!/bin/sh
exec docker compose -p "$QPC_REGTEST_PROJECT" -f "$QPC_REGTEST_COMPOSE" exec -T zcashd zcash-cli -regtest -conf=/run/qpc/zcashd.conf -datadir=/var/lib/zcash "$@"
EOF_CLI
chmod 700 "$work/zcash-cli"
QPC_ZCASH_CLI="$work/zcash-cli"
export QPC_REGTEST_EVIDENCE="$work/evidence.json"
export QPC_ZCASH_CLI QPC_ZCASH_CONF="$work/zcashd.conf" QPC_ZCASH_RPC_ENDPOINT=http://127.0.0.1:18232 QPC_LIGHTWALLETD_ENDPOINT=http://127.0.0.1:9067
cd "$root/zcash/qpc-zcash-agent"
# Fail closed if the named real integration test was accidentally removed,
# renamed, or no longer ignored (Cargo otherwise exits successfully on zero tests).
listed=$(cargo test --locked --features regtest-integration --test regtest_incoming -- --list --ignored)
printf '%s\n' "$listed" | grep -Fx 'receives_and_persists_a_real_shielded_regtest_note: test' >/dev/null || {
  echo "Expected ignored real Regtest integration test was not discovered." >&2
  exit 1
}
cargo test --locked --features regtest-integration --test regtest_incoming -- --ignored --exact receives_and_persists_a_real_shielded_regtest_note --nocapture
test -s "$QPC_REGTEST_EVIDENCE"
cat "$QPC_REGTEST_EVIDENCE"
