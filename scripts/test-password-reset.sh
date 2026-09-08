#!/usr/bin/env bash
set -euo pipefail

# Uses only a temporary account. generate_link supplies test email evidence
# without sending email; the application still uses the public verify endpoint.
project_ref="${1:?Usage: bash scripts/test-password-reset.sh <project-ref>}"
[[ "$project_ref" =~ ^[a-z0-9]{20}$ ]] || exit 2
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
chmod 700 "$test_dir"
test_user_id=""
server_pid=""
admin_key=""
cleanup() {
  if [[ -n "$server_pid" ]]; then kill "$server_pid" 2>/dev/null || true; wait "$server_pid" 2>/dev/null || true; fi
  if [[ -n "$test_user_id" ]]; then
    curl -fsS -X DELETE -H "apikey: $admin_key" -H "Authorization: Bearer $admin_key" \
      "https://$project_ref.supabase.co/auth/v1/admin/users/$test_user_id" >/dev/null || { echo 'Temporary user cleanup failed' >&2; return 1; }
  fi
  find "$test_dir" -type f -delete
  rmdir "$test_dir"
}
trap cleanup EXIT
keys="$(supabase projects api-keys --project-ref "$project_ref" --output json)"
admin_key="$(printf '%s' "$keys" | jq -er '.[] | select(.name == "service_role") | .api_key')"
public_key="$(printf '%s' "$keys" | jq -er '[.[] | select(.type == "publishable")][0].api_key')"
test_email="mario-reset-$(uuidgen | tr '[:upper:]' '[:lower:]')@example.com"
old_password="$(openssl rand -hex 24)Aa1!"
new_password="$(openssl rand -hex 24)Bb2!"
project_url="https://$project_ref.supabase.co"
admin_request() {
  curl -fsS -H "apikey: $admin_key" -H "Authorization: Bearer $admin_key" \
    -H 'Content-Type: application/json' --data-binary @- "$project_url/auth/v1/admin/$1"
}
test_user_id="$(jq -nc --arg email "$test_email" --arg password "$old_password" \
  '{email:$email,password:$password,email_confirm:true}' | admin_request users | jq -er '.id')"
[[ "$test_user_id" =~ ^[0-9a-f-]{36}$ ]] || exit 1
port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
local_token="$(openssl rand -hex 32)"
MARIO_DATA_DIR="$test_dir" MARIO_AUTH_TOKEN="$local_token" "$repo_root/server/target/debug/mario-server" --port "$port" >"$test_dir/server.log" 2>&1 &
server_pid=$!
local_api() {
  curl -fsS -H "Authorization: Bearer $local_token" -H 'Content-Type: application/json' \
    -X "$1" --data-binary @- "http://127.0.0.1:$port/api$2"
}
for _ in $(seq 1 80); do
  if curl -fsS -H "Authorization: Bearer $local_token" "http://127.0.0.1:$port/api/health" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
jq -nc --arg url "$project_url" --arg publishableKey "$public_key" '{url:$url,publishableKey:$publishableKey}' \
  | local_api PUT /cloud/config >/dev/null
# Exercise both default email links and OTP templates against real GoTrue.
for proof_kind in action_link email_otp; do
  link="$(jq -nc --arg email "$test_email" '{email:$email,type:"recovery"}' | admin_request generate_link)"
  proof="$(printf '%s' "$link" | jq -er --arg kind "$proof_kind" '.[$kind] // .properties[$kind]')"
  verification="$(jq -nc --arg email "$test_email" --arg proof "$proof" '{email:$email,proof:$proof}' | local_api POST /cloud/password/verify)"
  recovery_id="$(printf '%s' "$verification" | jq -er '.recoveryId')"
  jq -nc --arg recoveryId "$recovery_id" --arg password "$new_password" '{recoveryId:$recoveryId,password:$password}' \
    | local_api POST /cloud/password/reset | jq -e '.message | contains("密码已更新")' >/dev/null
  # Test login directly to avoid changing the real user's system keychain.
  login_status="$(jq -nc --arg email "$test_email" --arg password "$old_password" '{email:$email,password:$password}' \
    | curl -sS -o "$test_dir/login.json" -w '%{http_code}' -H "apikey: $public_key" -H 'Content-Type: application/json' \
      --data-binary @- "$project_url/auth/v1/token?grant_type=password")"
  [[ "$login_status" == 400 ]] || { echo 'Old password unexpectedly accepted' >&2; exit 1; }
  jq -nc --arg email "$test_email" --arg password "$new_password" '{email:$email,password:$password}' \
    | curl -fsS -H "apikey: $public_key" -H 'Content-Type: application/json' --data-binary @- "$project_url/auth/v1/token?grant_type=password" \
    | jq -e --arg id "$test_user_id" '.user.id == $id and (.access_token | length > 0)' >/dev/null
  old_password="$new_password"
  new_password="$(openssl rand -hex 24)Cc3!"
done
echo 'Password reset E2E passed: link and OTP verification, reset, old password rejected, new password accepted.'
