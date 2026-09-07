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

target_triple="$(rustc -vV | sed -n 's/^host: //p')"

if [[ "$profile" == "release" ]]; then
  cargo build --release --manifest-path "$root_dir/server/Cargo.toml"
  source_binary="$root_dir/server/target/release/mario-server"
else
  cargo build --manifest-path "$root_dir/server/Cargo.toml"
  source_binary="$root_dir/server/target/debug/mario-server"
fi

destination_dir="$root_dir/client/src-tauri/binaries"
mkdir -p "$destination_dir"
cp "$source_binary" "$destination_dir/mario-server-$target_triple"
chmod +x "$destination_dir/mario-server-$target_triple"

echo "Prepared sidecar: $destination_dir/mario-server-$target_triple"
