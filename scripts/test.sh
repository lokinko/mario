#!/usr/bin/env bash
set -euo pipefail

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

npm run build --prefix "$root_dir/client"
cargo fmt --manifest-path "$root_dir/server/Cargo.toml" -- --check
cargo fmt --manifest-path "$root_dir/client/src-tauri/Cargo.toml" -- --check
cargo test --manifest-path "$root_dir/server/Cargo.toml"
cargo build --manifest-path "$root_dir/server/Cargo.toml"
"$root_dir/scripts/test-local-auth.sh"
cargo test --manifest-path "$root_dir/client/src-tauri/Cargo.toml"
