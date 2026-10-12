#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/.." && pwd)"
(cd "$root_dir" && npm run check:architecture)
npm run build --prefix "$root_dir/client"
npm run format:check --prefix "$root_dir/client"
npm test --prefix "$root_dir/client"
cargo fmt --manifest-path "$root_dir/server/Cargo.toml" -- --check
cargo test --manifest-path "$root_dir/server/Cargo.toml"
cargo clippy --manifest-path "$root_dir/server/Cargo.toml" --all-targets -- -D warnings
cargo build --manifest-path "$root_dir/server/Cargo.toml"
(cd "$root_dir" && node scripts/test-functional-api.mjs)
(cd "$root_dir" && node scripts/test-web.mjs)
"$root_dir/scripts/test-local-auth.sh"
