#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/.." && pwd)"
if [[ "$(uname -s)" == "Darwin" ]] && ! /usr/bin/clang --version >/dev/null 2>&1; then
  if [[ -x "/Library/Developer/CommandLineTools/usr/bin/clang" ]]; then
    export DEVELOPER_DIR="/Library/Developer/CommandLineTools"
  fi
fi

npm run build --prefix "$root_dir/client"
cargo fmt --manifest-path "$root_dir/server/Cargo.toml" -- --check
cargo fmt --manifest-path "$root_dir/client/src-tauri/Cargo.toml" -- --check
cargo test --manifest-path "$root_dir/server/Cargo.toml"
cargo test --manifest-path "$root_dir/client/src-tauri/Cargo.toml"
