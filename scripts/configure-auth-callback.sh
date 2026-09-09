#!/usr/bin/env bash
set -euo pipefail

project_ref="${1:?Usage: bash scripts/configure-auth-callback.sh <project-ref> <callback-url>}"
callback_url="${2:?A deployed HTTPS callback URL is required}"
[[ "$project_ref" =~ ^[a-z0-9]{20}$ && "$callback_url" == https://* ]] || exit 2
root="$(cd "$(dirname "$0")/.." && pwd)"
# Refuse to point email at an unpublished callback.
curl -4 --max-time 20 -fsS "$callback_url" | rg -q 'mario · 邮箱验证'
echo 'Public callback is reachable; loading deployment credentials.'
management_token="${SUPABASE_ACCESS_TOKEN:-}"
if [[ -z "$management_token" ]]; then
  management_token="$(security find-generic-password -s 'Supabase CLI' -a supabase -w)"
  case "$management_token" in
    go-keyring-base64:*) management_token="$(printf '%s' "${management_token#go-keyring-base64:}" | base64 -D)" ;;
  esac
fi
api_url="https://api.supabase.com/v1/projects/$project_ref/config/auth"
echo 'Reading current auth configuration.'
previous="$(curl -4 --max-time 30 -fsS -H "Authorization: Bearer $management_token" "$api_url" | jq '{site_url,uri_allow_list,mailer_templates_confirmation_content,mailer_templates_recovery_content}')"
backup="$(mktemp /tmp/mario-auth-config-backup.XXXXXX)"
chmod 600 "$backup"
printf '%s' "$previous" > "$backup"
payload="$(printf '%s' "$previous" | jq --arg url "$callback_url" \
  '{site_url:$url, uri_allow_list: ((.uri_allow_list // "" | split(",") | map(select(length > 0)) | . + [$url] | unique) | join(","))}')"
# Default-provider free projects cannot customize templates. Redirect settings
# work independently; supplied templates are optional for custom SMTP deployments.
echo 'Updating callback routing; preserving current email templates.'
printf '%s' "$payload" | curl -4 --http1.1 --max-time 30 --fail-with-body -sS -X PATCH -H "Authorization: Bearer $management_token" \
  -H 'Content-Type: application/json' --data-binary @- "$api_url" \
  | jq '{site_url,uri_allow_list,mailer_autoconfirm,message}'
echo "Previous callback/template settings saved to $backup"
