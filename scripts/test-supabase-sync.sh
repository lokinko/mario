#!/usr/bin/env bash
set -euo pipefail

project_ref="${1:-}"
if [[ ! "$project_ref" =~ ^[a-z0-9]{20}$ ]]; then
  echo "Usage: bash scripts/test-supabase-sync.sh <project-ref>" >&2
  exit 2
fi

for command_name in supabase jq curl openssl uuidgen security; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command: $command_name" >&2
    exit 2
  fi
done

for service_name in "com.lokinko.mario" "com.compassinvest.desktop"; do
  for account_name in "cloud-access-token" "cloud-refresh-token"; do
    if security find-generic-password -s "$service_name" -a "$account_name" >/dev/null 2>&1; then
      echo "Refusing to overwrite an existing mario cloud session in the system keychain." >&2
      echo "Sign out in the app before running the isolated Supabase sync test." >&2
      exit 2
    fi
  done
done

repo_root=$(cd "$(dirname "$0")/.." && pwd)
server_binary="$repo_root/server/target/debug/mario-server"
if [[ ! -x "$server_binary" ]]; then
  echo "Build the server first: cargo build --manifest-path server/Cargo.toml" >&2
  exit 2
fi

keys_file=$(mktemp)
create_response=$(mktemp)
delete_response=$(mktemp)
device_a_dir=$(mktemp -d)
device_b_dir=$(mktemp -d)
device_a_log=$(mktemp)
device_b_log=$(mktemp)
chmod 600 "$keys_file" "$create_response" "$delete_response" "$device_a_log" "$device_b_log"

device_a_pid=""
device_b_pid=""
test_user_id=""
service_role_key=""
test_email=""
test_password=""

stop_server() {
  local server_pid="$1"
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
}

delete_keyring_entry() {
  local service_name="$1"
  local account_name="$2"
  security delete-generic-password -s "$service_name" -a "$account_name" >/dev/null 2>&1 || true
}

remove_temp_dir() {
  local directory="$1"
  if [[ -d "$directory" ]]; then
    find "$directory" -type f -delete
    find "$directory" -depth -type d ! -path "$directory" -exec rmdir {} \; 2>/dev/null || true
    rmdir "$directory" 2>/dev/null || true
  fi
}

cleanup() {
  stop_server "$device_a_pid"
  stop_server "$device_b_pid"

  if [[ -n "$test_user_id" ]]; then
    delete_keyring_entry "com.lokinko.mario" "cloud-encryption-key:$test_user_id"
    delete_keyring_entry "com.compassinvest.desktop" "cloud-encryption-key:$test_user_id"
  fi
  delete_keyring_entry "com.lokinko.mario" "cloud-access-token"
  delete_keyring_entry "com.lokinko.mario" "cloud-refresh-token"
  delete_keyring_entry "com.compassinvest.desktop" "cloud-access-token"
  delete_keyring_entry "com.compassinvest.desktop" "cloud-refresh-token"

  if [[ -n "$test_user_id" && -n "$service_role_key" ]]; then
    curl -sS -o "$delete_response" \
      -H "apikey: $service_role_key" \
      -H "Authorization: Bearer $service_role_key" \
      -X DELETE "https://$project_ref.supabase.co/auth/v1/admin/users/$test_user_id" || true
  fi

  for temp_file in "$keys_file" "$create_response" "$delete_response" "$device_a_log" "$device_b_log"; do
    [[ ! -e "$temp_file" ]] || unlink "$temp_file"
  done
  remove_temp_dir "$device_a_dir"
  remove_temp_dir "$device_b_dir"
}
trap cleanup EXIT

supabase projects api-keys --project-ref "$project_ref" --output json > "$keys_file"
publishable_key=$(jq -r '.[] | select(.type == "publishable") | .api_key' "$keys_file" | head -1)
service_role_key=$(jq -r '.[] | select(.name == "service_role") | .api_key' "$keys_file" | head -1)
if [[ -z "$publishable_key" || -z "$service_role_key" ]]; then
  echo "The project is missing a publishable or service_role key." >&2
  exit 1
fi

test_suffix=$(uuidgen | tr '[:upper:]' '[:lower:]' | tr -d '-')
test_email="mario-sync-e2e-$test_suffix@example.com"
test_password=$(openssl rand -base64 32 | tr -d '\n')
marker="mario-e2e-$test_suffix"
create_payload=$(jq -nc \
  --arg email "$test_email" \
  --arg password "$test_password" \
  '{email:$email,password:$password,email_confirm:true,user_metadata:{purpose:"mario encrypted sync e2e"}}')
create_status=$(curl -sS -o "$create_response" -w '%{http_code}' \
  -H "apikey: $service_role_key" \
  -H "Authorization: Bearer $service_role_key" \
  -H 'Content-Type: application/json' \
  --data "$create_payload" \
  "https://$project_ref.supabase.co/auth/v1/admin/users")
if [[ "$create_status" != "200" ]]; then
  echo "Could not create the isolated sync test account (HTTP $create_status)." >&2
  jq -r '.message // .msg // .error // "Unknown Supabase error"' "$create_response" >&2
  exit 1
fi
test_user_id=$(jq -r '.id' "$create_response")
if [[ ! "$test_user_id" =~ ^[0-9a-f-]{36}$ ]]; then
  echo "Supabase returned an invalid test user id." >&2
  exit 1
fi

start_server() {
  local data_dir="$1"
  local port="$2"
  local auth_token="$3"
  local log_file="$4"
  MARIO_DATA_DIR="$data_dir" MARIO_AUTH_TOKEN="$auth_token" \
    "$server_binary" --port "$port" >"$log_file" 2>&1 &
  local server_pid=$!
  for _ in $(seq 1 80); do
    if curl -fsS -H "Authorization: Bearer $auth_token" \
      "http://127.0.0.1:$port/api/health" >/dev/null 2>&1; then
      printf '%s' "$server_pid"
      return 0
    fi
    sleep 0.1
  done
  echo "Timed out waiting for local mario service on port $port." >&2
  sed -n '1,80p' "$log_file" >&2
  return 1
}

api_call() {
  local port="$1"
  local auth_token="$2"
  local method="$3"
  local path="$4"
  local body="${5:-}"
  if [[ -n "$body" ]]; then
    curl -fsS \
      -H "Authorization: Bearer $auth_token" \
      -H 'Content-Type: application/json' \
      -X "$method" --data "$body" \
      "http://127.0.0.1:$port/api$path"
  else
    curl -fsS \
      -H "Authorization: Bearer $auth_token" \
      -H 'Content-Type: application/json' \
      -X "$method" \
      "http://127.0.0.1:$port/api$path"
  fi
}

project_url="https://$project_ref.supabase.co"
config_payload=$(jq -nc --arg url "$project_url" --arg publishableKey "$publishable_key" \
  '{url:$url,publishableKey:$publishableKey}')
login_payload=$(jq -nc --arg email "$test_email" --arg password "$test_password" \
  '{email:$email,password:$password}')
goal_payload=$(jq -nc --arg name "$marker" \
  '{name:$name,targetAmount:120000,currentAmount:12000,monthlyContribution:3000,targetDate:"2030-12-31",priority:"高"}')

device_a_port=43271
device_a_auth=$(openssl rand -hex 32)
device_a_pid=$(start_server "$device_a_dir" "$device_a_port" "$device_a_auth" "$device_a_log")
api_call "$device_a_port" "$device_a_auth" PUT /cloud/config "$config_payload" \
  | jq -e '.configured == true and .signedIn == false' >/dev/null
api_call "$device_a_port" "$device_a_auth" POST /cloud/login "$login_payload" \
  | jq -e '.signedIn == true and .emailConfirmationPending == false' >/dev/null
api_call "$device_a_port" "$device_a_auth" POST /goals "$goal_payload" \
  | jq -e --arg marker "$marker" 'any(.goals[]; .name == $marker)' >/dev/null
push_result=$(api_call "$device_a_port" "$device_a_auth" POST /cloud/sync/push)
printf '%s' "$push_result" \
  | jq -e '.direction == "push" and .revision == 1 and .recordCount > 0' >/dev/null
recovery_key=$(api_call "$device_a_port" "$device_a_auth" GET /cloud/recovery-key | jq -r '.recoveryKey')
if [[ ! "$recovery_key" =~ ^mario-sync-v1: ]]; then
  echo "The first device did not create a mario recovery key." >&2
  exit 1
fi
stop_server "$device_a_pid"
device_a_pid=""

delete_keyring_entry "com.lokinko.mario" "cloud-encryption-key:$test_user_id"
delete_keyring_entry "com.compassinvest.desktop" "cloud-encryption-key:$test_user_id"

device_b_port=43272
device_b_auth=$(openssl rand -hex 32)
device_b_pid=$(start_server "$device_b_dir" "$device_b_port" "$device_b_auth" "$device_b_log")
api_call "$device_b_port" "$device_b_auth" PUT /cloud/config "$config_payload" \
  | jq -e '.configured == true' >/dev/null
api_call "$device_b_port" "$device_b_auth" POST /cloud/login "$login_payload" \
  | jq -e '.signedIn == true' >/dev/null
recovery_payload=$(jq -nc --arg recoveryKey "$recovery_key" \
  '{recoveryKey:$recoveryKey,confirmReplace:false}')
api_call "$device_b_port" "$device_b_auth" PUT /cloud/recovery-key "$recovery_payload" >/dev/null
pull_result=$(api_call "$device_b_port" "$device_b_auth" POST /cloud/sync/pull '{"confirmReplace":true}')
printf '%s' "$pull_result" \
  | jq -e '.direction == "pull" and .revision == 1 and .recordCount > 0' >/dev/null
api_call "$device_b_port" "$device_b_auth" GET /snapshot \
  | jq -e --arg marker "$marker" 'any(.goals[]; .name == $marker)' >/dev/null

remote_check=$(supabase db query --linked --project-ref "$project_ref" --output-format json \
  "select revision, schema_version, length(ciphertext) as ciphertext_length, position('$marker' in ciphertext) = 0 as plaintext_marker_absent from public.sync_blobs where user_id = '$test_user_id';")
printf '%s' "$remote_check" \
  | jq -e '.rows | length == 1 and .[0].revision == 1 and .[0].schema_version > 0 and .[0].ciphertext_length > 0 and .[0].plaintext_marker_absent == true' >/dev/null

api_call "$device_b_port" "$device_b_auth" DELETE /cloud/session >/dev/null
stop_server "$device_b_pid"
device_b_pid=""

delete_status=$(curl -sS -o "$delete_response" -w '%{http_code}' \
  -H "apikey: $service_role_key" \
  -H "Authorization: Bearer $service_role_key" \
  -X DELETE "https://$project_ref.supabase.co/auth/v1/admin/users/$test_user_id")
if [[ "$delete_status" != "200" ]]; then
  echo "The sync test passed, but cleanup could not delete the test account (HTTP $delete_status)." >&2
  exit 1
fi
delete_keyring_entry "com.lokinko.mario" "cloud-encryption-key:$test_user_id"
delete_keyring_entry "com.compassinvest.desktop" "cloud-encryption-key:$test_user_id"
test_user_id=""

remaining=$(supabase db query --linked --project-ref "$project_ref" --output-format json \
  "select count(*)::integer as count from public.sync_blobs where user_id = '$(jq -r '.id' "$create_response")';")
printf '%s' "$remaining" | jq -e '.rows[0].count == 0' >/dev/null

echo "Supabase encrypted sync E2E passed: device A push -> device B recovery-key pull; remote plaintext check and cleanup passed."
