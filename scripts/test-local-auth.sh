#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/.." && pwd)"
server_bin="$root_dir/server/target/debug/mario-server"
test_dir="$(mktemp -d /tmp/mario-local-auth.XXXXXX)"
port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
token="0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
server_pid=""

cleanup() {
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid"
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$test_dir"
}
trap cleanup EXIT

if env -u MARIO_AUTH_TOKEN -u COMPASS_AUTH_TOKEN MARIO_DATA_DIR="$test_dir" "$server_bin" --port "$port" >"$test_dir/no-token.log" 2>&1; then
  echo "server unexpectedly started without an authentication token" >&2
  exit 1
fi

env -u COMPASS_AUTH_TOKEN MARIO_DATA_DIR="$test_dir" MARIO_AUTH_TOKEN="$token" \
  "$server_bin" --port "$port" >"$test_dir/server.log" 2>&1 &
server_pid=$!

for _ in $(seq 1 40); do
  if curl -fsS -H "Authorization: Bearer $token" "http://127.0.0.1:$port/api/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if [[ "$(curl -sS -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/api/health")" != "401" ]]; then
  echo "request without token was not rejected" >&2
  exit 1
fi
if [[ "$(curl -sS -o /dev/null -w '%{http_code}' -H 'Authorization: Bearer wrong' "http://127.0.0.1:$port/api/health")" != "401" ]]; then
  echo "request with wrong token was not rejected" >&2
  exit 1
fi
if [[ "$(curl -sS -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $token" "http://127.0.0.1:$port/api/health")" != "200" ]]; then
  echo "request with current token was not accepted" >&2
  exit 1
fi
market_config="$(curl -fsS -H "Authorization: Bearer $token" "http://127.0.0.1:$port/api/market-data/security/config")"
if ! python3 -c 'import json,sys; value=json.load(sys.stdin); assert value["provider"] == "twelve-data" and isinstance(value["hasApiKey"], bool)' <<<"$market_config"; then
  echo "security price config endpoint returned an unexpected response" >&2
  exit 1
fi
if process_command="$(ps -p "$server_pid" -o command= 2>/dev/null)"; then
  if grep -Fq "$token" <<<"$process_command"; then
    echo "authentication token leaked into process arguments" >&2
    exit 1
  fi
else
  echo "process argument inspection unavailable; HTTP authentication checks still ran" >&2
fi

preflight="$(curl -sS -i -X OPTIONS \
  -H 'Origin: tauri://localhost' \
  -H 'Access-Control-Request-Method: GET' \
  -H 'Access-Control-Request-Headers: authorization' \
  "http://127.0.0.1:$port/api/health")"
if ! grep -qi 'access-control-allow-headers: content-type,authorization' <<<"$preflight"; then
  echo "CORS preflight did not allow the authenticated desktop request" >&2
  exit 1
fi

echo "local API authentication smoke test passed"
