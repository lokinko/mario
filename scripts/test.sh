#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/.." && pwd)"
(cd "$root_dir" && npm run check:architecture)
(cd "$root_dir" && npm run test:qa-eval)
(cd "$root_dir" && npm run test:benchmark)
npm run build --prefix "$root_dir/client"
npm run format:check --prefix "$root_dir/client"
npm test --prefix "$root_dir/client"
cargo fmt --manifest-path "$root_dir/server/Cargo.toml" -- --check
cargo clippy --manifest-path "$root_dir/server/Cargo.toml" --all-targets -- -D warnings
cargo build --manifest-path "$root_dir/server/Cargo.toml"
# Rust tests and HTTP workflows share one disposable PostgreSQL cluster; each test uses its own account.
(cd "$root_dir" && node scripts/postgres-cluster.mjs bash -c '
  set -euo pipefail
  cargo test --manifest-path server/Cargo.toml
  node scripts/test-functional-api.mjs
  node scripts/test-web.mjs
  node scripts/test-codex-bridge.mjs
')
(cd "$root_dir" && npm run test:financial)
