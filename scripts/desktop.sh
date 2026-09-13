#!/usr/bin/env bash
set -euo pipefail

profile="${1:-debug}"
root_dir="$(cd "$(dirname "$0")/.." && pwd)"
if [[ "$(uname -s)" == "Darwin" ]] && [[ -x "/Library/Developer/CommandLineTools/usr/bin/clang" ]]; then
  export DEVELOPER_DIR="/Library/Developer/CommandLineTools"
  export PATH="/Library/Developer/CommandLineTools/usr/bin:$PATH"
  export SDKROOT="/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk"
  export CC="clang"
  export CXX="clang++"
  export AR="ar"
  export RANLIB="ranlib"
fi

bash "$root_dir/scripts/prepare-sidecar.sh" "$profile"
if [[ "$profile" == "release" ]]; then
  # Finder AppleScript can hang or fail in headless/non-interactive sessions.
  export CI="${CI:-true}"
  npm run desktop:build --prefix "$root_dir/client"
  app_version="$(node -p 'require(process.argv[1]).version' "$root_dir/client/package.json")"
  mkdir -p "$root_dir/outputs"
  for bundle in "$root_dir/client/src-tauri/target/release/bundle/dmg/mario_${app_version}_"*.dmg; do
    [[ -f "$bundle" ]] || continue
    cp "$bundle" "$root_dir/outputs/$(basename "$bundle")"
    echo "Prepared DMG: $root_dir/outputs/$(basename "$bundle")"
  done
  if [[ -d "$root_dir/client/src-tauri/target/release/bundle/macos/mario.app" ]]; then
    staged_app="$(mktemp -d "$root_dir/outputs/.mario-app.XXXXXX")"
    trap 'rm -rf "$staged_app"' EXIT
    ditto "$root_dir/client/src-tauri/target/release/bundle/macos/mario.app" "$staged_app/mario.app"
    rm -rf "$root_dir/outputs/mario.app"
    mv "$staged_app/mario.app" "$root_dir/outputs/mario.app"
    rmdir "$staged_app"
    trap - EXIT
    echo "Prepared app: $root_dir/outputs/mario.app"
  fi
else
  npm run desktop:dev --prefix "$root_dir/client"
fi
