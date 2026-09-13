#!/usr/bin/env bash
# Launch the Orbit daemon and the native desktop app together for local testing.
#
# The native Tauri app needs a running daemon plus ORBIT_SOCKET_NAME and
# ORBIT_DAEMON_TOKEN in its environment; a plain `pnpm dev` / browser preview
# cannot reach the daemon and will show "Daemon unavailable". This script
# handles the daemon, token, and env wiring so the app just works.
#
# Usage:  scripts/dev-app.sh
# Env:    ORBIT_DATA_DIR   (default .orbit-dev)
#         ORBIT_SOCKET_NAME (default orbit-workspace-daemon)
set -euo pipefail
cd "$(dirname "$0")/.."
REPO_ROOT="$PWD"

DATA_DIR="${ORBIT_DATA_DIR:-.orbit-dev}"
export ORBIT_SOCKET_NAME="${ORBIT_SOCKET_NAME:-orbit-workspace-daemon}"
# Dev secrets: unsigned dev binaries can't use the macOS Keychain, so store
# secrets in local 0600 files under the data dir instead. (Production leaves
# ORBIT_CREDENTIAL_DIR unset and uses the OS keychain.)
export ORBIT_CREDENTIAL_DIR="${ORBIT_CREDENTIAL_DIR:-$PWD/$DATA_DIR/credentials}"

# Ensure the canonical workspace package is available without fetching network
# dependencies during local startup.
pnpm install --offline --frozen-lockfile --ignore-scripts >/dev/null

# Install a real executable into the daemon's PATH. Do not depend on pnpm's
# workspace symlinks surviving a container or a different launch directory.
HELPER_DIR="$PWD/.orbit-dev/bin"
mkdir -p "$HELPER_DIR"
HELPER="$HELPER_DIR/balamos-hedera-readonly"
cat >"$HELPER" <<EOF
#!/usr/bin/env bash
set -euo pipefail
exec node "$REPO_ROOT/packages/hedera-readonly/src/cli.mjs" "\$@"
EOF
chmod 0755 "$HELPER"
export BALAMOS_HEDERA_HELPER="$HELPER"
"$HELPER" ready >/dev/null
echo "Verified Hedera helper: $HELPER"

# The daemon rejects a data dir that others can read; keep it owner-only.
[ -d "$DATA_DIR" ] && chmod 700 "$DATA_DIR" 2>/dev/null || true

# opencode bots need an opencode-go API key injected into their container. Reuse
# the local opencode CLI credential if the caller hasn't already set one. Absent
# key just means opencode bots report Unavailable; Codex bots are unaffected.
if [ -z "${ORBIT_OPENCODE_API_KEY:-}" ]; then
  AUTH="$HOME/.local/share/opencode/auth.json"
  if [ -f "$AUTH" ]; then
    ORBIT_OPENCODE_API_KEY="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1])).get('opencode-go',{}).get('key',''))" "$AUTH" 2>/dev/null || true)"
    [ -n "$ORBIT_OPENCODE_API_KEY" ] && export ORBIT_OPENCODE_API_KEY \
      && echo "Using local opencode-go key for opencode bots."
  fi
fi

started_daemon=""
cleanup() {
  if [ -n "$started_daemon" ]; then
    kill "$started_daemon" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

if pgrep -f "orbit-daemon --data-dir $DATA_DIR" >/dev/null 2>&1; then
  echo "orbit-daemon already running on $DATA_DIR; reusing it."
else
  echo "Starting orbit-daemon (data dir: $DATA_DIR)…"
  cargo run -q -p orbit-daemon -- --data-dir "$DATA_DIR" &
  started_daemon=$!
fi

echo "Waiting for the daemon token…"
for _ in $(seq 1 60); do
  [ -f "$DATA_DIR/daemon.token" ] && break
  sleep 0.5
done
if [ ! -f "$DATA_DIR/daemon.token" ]; then
  echo "error: $DATA_DIR/daemon.token never appeared." >&2
  echo "       Is Docker running and the daemon healthy? Check its output above." >&2
  exit 1
fi
export ORBIT_DAEMON_TOKEN="$(tr -d '\n' <"$DATA_DIR/daemon.token")"

echo "Launching the native Orbit app (socket: $ORBIT_SOCKET_NAME)…"
echo "Close the app window (or Ctrl+C) to stop; the daemon this script started will be stopped too."
pnpm --dir apps/desktop tauri dev
