#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
project_root="$(cd -- "${script_dir}/.." && pwd)"
cd "${project_root}"

for command_name in node npm cargo rustc; do
  if ! command -v "${command_name}" >/dev/null 2>&1; then
    echo "Missing required command: ${command_name}" >&2
    exit 1
  fi
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script must be run on macOS." >&2
  exit 1
fi

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-10.15}"
# Skip Finder AppleScript customization so the DMG build is reliable in
# terminals and headless CI runners.
export CI="${CI:-true}"

bundle_dir="${project_root}/target/release/bundle"
if [[ -d "${bundle_dir}" ]]; then
  rm -rf -- "${bundle_dir}"
fi

npm ci
npm run tauri build -- --bundles app,dmg

echo "macOS bundles: ${project_root}/target/release/bundle"
