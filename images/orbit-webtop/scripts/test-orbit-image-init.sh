#!/usr/bin/env bash
set -euo pipefail
# Production image contract: orbit-webtop:0.4.0; tests use a source-specific suffix.

if [[ -n "${ORBIT_WEBTOP_IMAGE:-}" ]]; then
  image="$ORBIT_WEBTOP_IMAGE"
  printf 'testing explicitly selected image: %s\n' "$image"
else
  source_tag="orbit-webtop:test-$(git rev-parse --short HEAD)"
  printf 'building checked-out image for source-contract test: %s\n' "$source_tag"
  docker build --tag "$source_tag" images/orbit-webtop
  image="$source_tag"
fi
config_dir=$(mktemp -d)
trap 'rm -rf "$config_dir"' EXIT

run_hook() {
  docker run --rm --user root --entrypoint /bin/sh -v "$config_dir:/config" "$image" -c \
    /custom-cont-init.d/20-orbit-codex
}

# Exercise repair of pre-existing private directories owned by root with loose modes.
docker run --rm --user root --entrypoint /bin/sh -v "$config_dir:/config" "$image" -c \
  'mkdir -p /config/.codex/skills/orbit-desktop-control /config/orbit/captures /config/.cache /config/.config/opencode &&
   chmod 755 /config/.codex /config/.codex/skills /config/.codex/skills/orbit-desktop-control /config/orbit /config/orbit/captures /config/.cache /config/.config /config/.config/opencode'
run_hook
run_hook
skill="$config_dir/.codex/skills/orbit-desktop-control/SKILL.md"
docker run --rm --user root --entrypoint /bin/sh -v "$config_dir:/config" "$image" -c \
   'for private_dir in /config/.codex /config/.codex/skills /config/.codex/skills/orbit-desktop-control /config/orbit /config/orbit/captures /config/.cache /config/.config /config/.config/opencode; do
     test "$(stat -c %a "$private_dir")" = 700 &&
     test "$(stat -c %U:%G "$private_dir")" = abc:abc
   done &&
   test -f /config/.codex/skills/orbit-desktop-control/SKILL.md &&
   test "$(stat -c %a /config/.codex/skills/orbit-desktop-control/SKILL.md)" = 600 &&
   test "$(stat -c %U:%G /config/.codex/skills/orbit-desktop-control/SKILL.md)" = abc:abc &&
    cmp /opt/orbit/skills/orbit-desktop-control/SKILL.md /config/.codex/skills/orbit-desktop-control/SKILL.md &&
    test -f /config/.config/opencode/AGENTS.md &&
    test "$(stat -c %a /config/.config/opencode/AGENTS.md)" = 600 &&
    test "$(stat -c %U:%G /config/.config/opencode/AGENTS.md)" = abc:abc &&
    cmp /opt/orbit/opencode/AGENTS.md /config/.config/opencode/AGENTS.md'

docker run --rm --user abc --entrypoint /bin/sh -v "$config_dir:/config" "$image" -c \
  'test -r /config/.codex/skills/orbit-desktop-control/SKILL.md &&
   test -r /config/.config/opencode/AGENTS.md &&
   touch /config/orbit/captures/.orbit-init-probe &&
   rm /config/orbit/captures/.orbit-init-probe'

# Construct the attack as root inside Linux: the victim is a directory, not a file.
docker run --rm --user root --entrypoint /bin/sh -v "$config_dir:/config" "$image" -c \
  'rm -rf /config/.codex/skills/orbit-desktop-control &&
   mkdir /config/victim && chmod 755 /config/victim &&
   ln -s /config/victim /config/.codex/skills/orbit-desktop-control'
if run_hook; then
  echo 'symlink attack was not rejected' >&2
  exit 1
fi
docker run --rm --entrypoint /bin/sh -v "$config_dir:/config" "$image" -c \
  'test -d /config/victim && test "$(stat -c %U:%G /config/victim)" = root:root && test "$(stat -c %a /config/victim)" = 755'
