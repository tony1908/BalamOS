#!/usr/bin/env bash
set -euo pipefail

orbit_tmp_dir="$(mktemp -d)"
orbit_socket="orbit-smoke-$RANDOM-$RANDOM"
daemon_log="$orbit_tmp_dir/daemon.log"
ping_stdout="$orbit_tmp_dir/ping.stdout"
ping_stderr="$orbit_tmp_dir/ping.stderr"

cleanup() {
  trap - EXIT INT TERM
  if [[ -n "${orbit_daemon_pid:-}" ]]; then
    kill "$orbit_daemon_pid" 2>/dev/null || true
    wait "$orbit_daemon_pid" 2>/dev/null || true
  fi
  if [[ -n "$orbit_tmp_dir" && -d "$orbit_tmp_dir" ]]; then
    rm -rf -- "$orbit_tmp_dir"
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

cargo run -p orbit-daemon -- --socket-name "$orbit_socket" --data-dir "$orbit_tmp_dir" >"$daemon_log" 2>&1 &
orbit_daemon_pid=$!

for _ in {1..50}; do
  kill -0 "$orbit_daemon_pid" 2>/dev/null || {
    cat "$daemon_log" >&2
    exit 1
  }
  if [[ -s "$orbit_tmp_dir/daemon.token" ]]; then
    token="$(tr -d '\n' < "$orbit_tmp_dir/daemon.token")"
    if ORBIT_SOCKET_NAME="$orbit_socket" ORBIT_DAEMON_TOKEN="$token" \
      cargo run -p orbit-client --example ping >"$ping_stdout" 2>"$ping_stderr"; then
      if [[ "$(tr -d '\r\n' < "$ping_stdout")" == "pong" ]]; then
        printf 'pong\n'
        exit 0
      fi
    fi
  fi
  sleep 0.1
done
printf 'daemon smoke test timed out\n' >&2
cat "$daemon_log" >&2
cat "$ping_stderr" >&2
exit 1
