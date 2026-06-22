#!/usr/bin/env bash
set -euo pipefail

env_file=""

usage() {
  cat <<'USAGE'
Usage: scripts/setup-macos-signing-keychain.sh [--env-file <path>]

Creates an ephemeral macOS keychain, imports the production P12 signing
certificate, prepares codesign access, and stores a notarytool profile.
The script writes RICOCHET_MACOS_SIGN_IDENTITY and
RICOCHET_MACOS_NOTARY_PROFILE to GITHUB_ENV when running in GitHub Actions.
Pass --env-file locally to write a sourceable shell file with non-secret
environment values needed by the package script. It never prints certificate,
key, or password material.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env-file)
      env_file="${2:?--env-file requires a path}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

append_github_env() {
  local name="$1"
  local value="$2"
  if [[ -n "${GITHUB_ENV:-}" ]]; then
    printf '%s=%s\n' "$name" "$value" >> "$GITHUB_ENV"
  fi
}

shell_quote() {
  printf "'"
  printf '%s' "$1" | sed "s/'/'\\\\''/g"
  printf "'"
}

append_local_env() {
  local name="$1"
  local value="$2"
  if [[ -n "$env_file" ]]; then
    mkdir -p "$(dirname "$env_file")"
    printf 'export %s=%s\n' "$name" "$(shell_quote "$value")" >> "$env_file"
  fi
}

append_env_output() {
  local name="$1"
  local value="$2"
  append_github_env "$name" "$value"
  append_local_env "$name" "$value"
}

decode_base64_value() {
  local value="$1"
  local output="$2"
  if printf '%s' "$value" | base64 --decode > "$output" 2>/dev/null; then
    return 0
  fi
  printf '%s' "$value" | base64 -D > "$output"
}

p12_base64="${RICOCHET_MACOS_CERT_P12_BASE64:-}"
p12_password="${RICOCHET_MACOS_CERT_PASSWORD:-}"
sign_identity="${RICOCHET_MACOS_SIGN_IDENTITY:-}"
notary_profile="${RICOCHET_MACOS_NOTARY_PROFILE:-}"
keychain_password="${RICOCHET_MACOS_KEYCHAIN_PASSWORD:-}"

apple_id="${RICOCHET_MACOS_NOTARY_APPLE_ID:-}"
apple_team_id="${RICOCHET_MACOS_NOTARY_TEAM_ID:-}"
apple_password="${RICOCHET_MACOS_NOTARY_PASSWORD:-}"
api_key_base64="${RICOCHET_MACOS_NOTARY_API_KEY_BASE64:-}"
api_key_id="${RICOCHET_MACOS_NOTARY_KEY_ID:-}"
api_issuer_id="${RICOCHET_MACOS_NOTARY_ISSUER_ID:-}"

missing=()
[[ -n "$p12_base64" ]] || missing+=("RICOCHET_MACOS_CERT_P12_BASE64")
[[ -n "$p12_password" ]] || missing+=("RICOCHET_MACOS_CERT_PASSWORD")
[[ -n "$notary_profile" ]] || missing+=("RICOCHET_MACOS_NOTARY_PROFILE")

has_apple_notary=false
if [[ -n "$apple_id" && -n "$apple_team_id" && -n "$apple_password" ]]; then
  has_apple_notary=true
fi

has_api_notary=false
if [[ -n "$api_key_base64" && -n "$api_key_id" && -n "$api_issuer_id" ]]; then
  has_api_notary=true
fi

if [[ "$has_apple_notary" != "true" && "$has_api_notary" != "true" ]]; then
  missing+=("complete notary credentials: either RICOCHET_MACOS_NOTARY_APPLE_ID/RICOCHET_MACOS_NOTARY_TEAM_ID/RICOCHET_MACOS_NOTARY_PASSWORD or RICOCHET_MACOS_NOTARY_API_KEY_BASE64/RICOCHET_MACOS_NOTARY_KEY_ID/RICOCHET_MACOS_NOTARY_ISSUER_ID")
fi

if [[ "${#missing[@]}" -gt 0 ]]; then
  printf 'macOS signing keychain setup is missing required secret(s): %s\n' "${missing[*]}" >&2
  exit 1
fi

if [[ -z "$keychain_password" ]]; then
  keychain_password="$(uuidgen)-$(uuidgen)"
fi

runner_temp="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
runner_temp="$(cd "$runner_temp" && pwd -P)"
secret_dir="$(mktemp -d "$runner_temp/ricochet-macos-signing-import.XXXXXX")"
keychain_dir="$(mktemp -d "$runner_temp/ricochet-macos-keychain.XXXXXX")"
p12_file="$secret_dir/signing-certificate.p12"
api_key_file="$secret_dir/notary-api-key.p8"
keychain_path="$keychain_dir/ricochet-signing.keychain-db"
previous_keychain_list_file="$keychain_dir/previous-keychains.txt"
previous_default_keychain="$(security default-keychain -d user 2>/dev/null | sed -e 's/^[[:space:]]*"//' -e 's/"$//' || true)"

cleanup() {
  rm -rf "$secret_dir"
}
trap cleanup EXIT

cleanup_keychain_on_failure() {
  local status=$?
  if [[ "$status" -eq 0 ]]; then
    return
  fi

  if [[ -n "$previous_keychain_list_file" && -f "$previous_keychain_list_file" ]]; then
    restore_keychains=()
    while IFS= read -r line; do
      if [[ -n "$line" ]]; then
        restore_keychains+=("$line")
      fi
    done < "$previous_keychain_list_file"
    if [[ "${#restore_keychains[@]}" -gt 0 ]]; then
      security list-keychains -d user -s "${restore_keychains[@]}" || true
    fi
  fi

  if [[ -n "$previous_default_keychain" ]]; then
    security default-keychain -d user -s "$previous_default_keychain" || true
  fi

  if [[ -e "$keychain_path" ]] && [[ "$(basename -- "$keychain_path")" == "ricochet-signing.keychain-db" ]] && [[ "$(basename -- "$keychain_dir")" == ricochet-macos-keychain.* ]] && [[ "$(cd "$(dirname -- "$keychain_dir")" && pwd -P)" == "$runner_temp" ]]; then
    security delete-keychain "$keychain_path" || rm -f "$keychain_path"
  fi
  if [[ "$(basename -- "$secret_dir")" == ricochet-macos-signing-import.* ]] && [[ "$(cd "$(dirname -- "$secret_dir")" && pwd -P)" == "$runner_temp" ]]; then
    rm -rf "$secret_dir"
  fi
  if [[ "$(basename -- "$keychain_dir")" == ricochet-macos-keychain.* ]] && [[ "$(cd "$(dirname -- "$keychain_dir")" && pwd -P)" == "$runner_temp" ]]; then
    rm -rf "$keychain_dir"
  fi
}
trap cleanup_keychain_on_failure EXIT

decode_base64_value "$p12_base64" "$p12_file"

security create-keychain -p "$keychain_password" "$keychain_path"
security unlock-keychain -p "$keychain_password" "$keychain_path"
security set-keychain-settings -lut 21600 "$keychain_path"

existing_keychains=()
: > "$previous_keychain_list_file"
while IFS= read -r line; do
  cleaned="$(printf '%s' "$line" | sed -e 's/^[[:space:]]*"//' -e 's/"$//')"
  if [[ -n "$cleaned" ]]; then
    existing_keychains+=("$cleaned")
    printf '%s\n' "$cleaned" >> "$previous_keychain_list_file"
  fi
done < <(security list-keychains -d user)
security list-keychains -d user -s "$keychain_path" "${existing_keychains[@]}"
security default-keychain -d user -s "$keychain_path"

security import "$p12_file" -k "$keychain_path" -P "$p12_password" -T /usr/bin/codesign -T /usr/bin/security >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$keychain_password" "$keychain_path" >/dev/null

append_env_output "RICOCHET_MACOS_KEYCHAIN_PATH" "$keychain_path"
append_env_output "RICOCHET_MACOS_PREVIOUS_KEYCHAIN_LIST_FILE" "$previous_keychain_list_file"
if [[ -n "$previous_default_keychain" ]]; then
  append_env_output "RICOCHET_MACOS_PREVIOUS_DEFAULT_KEYCHAIN" "$previous_default_keychain"
fi

if [[ -z "$sign_identity" ]]; then
  sign_identity="$(security find-identity -v -p codesigning "$keychain_path" | awk -F '"' '/Developer ID Application/ { print $2; exit }')"
fi
if [[ -z "$sign_identity" ]]; then
  sign_identity="$(security find-identity -v -p codesigning "$keychain_path" | awk -F '"' '/"/ { print $2; exit }')"
fi
if [[ -z "$sign_identity" ]]; then
  echo "Could not resolve a codesigning identity from the imported P12." >&2
  exit 1
fi
if ! security find-identity -v -p codesigning "$keychain_path" | grep -F "$sign_identity" >/dev/null; then
  echo "RICOCHET_MACOS_SIGN_IDENTITY does not match an imported codesigning identity." >&2
  exit 1
fi

if [[ "$has_api_notary" == "true" ]]; then
  decode_base64_value "$api_key_base64" "$api_key_file"
  xcrun notarytool store-credentials "$notary_profile" \
    --keychain "$keychain_path" \
    --key "$api_key_file" \
    --key-id "$api_key_id" \
    --issuer "$api_issuer_id" >/dev/null
else
  xcrun notarytool store-credentials "$notary_profile" \
    --keychain "$keychain_path" \
    --apple-id "$apple_id" \
    --team-id "$apple_team_id" \
    --password "$apple_password" >/dev/null
fi

append_env_output "RICOCHET_MACOS_SIGN_IDENTITY" "$sign_identity"
append_env_output "RICOCHET_MACOS_NOTARY_PROFILE" "$notary_profile"
echo "Prepared macOS signing keychain."
echo "RICOCHET_MACOS_SIGN_IDENTITY=$sign_identity"
echo "RICOCHET_MACOS_NOTARY_PROFILE=$notary_profile"
trap cleanup EXIT
