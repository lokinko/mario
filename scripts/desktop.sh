#!/usr/bin/env bash
set -euo pipefail

profile="${1:-debug}"
root_dir="$(cd "$(dirname "$0")/.." && pwd)"
if [[ "$(uname -s)" == "Darwin" ]] && ! /usr/bin/clang --version >/dev/null 2>&1; then
  if [[ -x "/Library/Developer/CommandLineTools/usr/bin/clang" ]]; then
    export DEVELOPER_DIR="/Library/Developer/CommandLineTools"
  fi
fi

bash "$root_dir/scripts/prepare-sidecar.sh" "$profile"
if [[ "$profile" == "release" ]]; then
  # Finder AppleScript can hang or fail in headless/non-interactive sessions.
  export CI="${CI:-true}"
  npm run desktop:build --prefix "$root_dir/client"
else
  npm run desktop:dev --prefix "$root_dir/client"
fi
