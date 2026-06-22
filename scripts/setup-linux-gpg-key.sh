#!/usr/bin/env bash
set -euo pipefail

required=false
env_file=""

usage() {
  cat <<'USAGE'
Usage: scripts/setup-linux-gpg-key.sh [--required] [--env-file <path>]

Imports the production Linux GPG private key for release artifact signatures.
The script writes GNUPGHOME and RICOCHET_LINUX_GPG_KEY to GITHUB_ENV when
running in GitHub Actions. Pass --env-file locally to write a sourceable shell
file with non-secret environment values needed by the package script. It never
prints private key material.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --required)
      required=true
      shift
      ;;
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
  printf '%s' "$value" | base64 -d > "$output"
}

key_base64="${RICOCHET_LINUX_GPG_PRIVATE_KEY_BASE64:-}"
ownertrust_base64="${RICOCHET_LINUX_GPG_OWNERTRUST_BASE64:-}"
requested_key="${RICOCHET_LINUX_GPG_KEY:-}"

if [[ -z "$key_base64" ]]; then
  message="Linux GPG import skipped because RICOCHET_LINUX_GPG_PRIVATE_KEY_BASE64 is missing."
  if [[ "$required" == "true" ]]; then
    echo "$message" >&2
    exit 1
  fi
  echo "$message"
  exit 0
fi

if ! command -v gpg >/dev/null 2>&1; then
  echo "gpg is required to import the Linux release signing key." >&2
  exit 1
fi

runner_temp="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
secret_dir="$(mktemp -d "$runner_temp/ricochet-linux-gpg-import.XXXXXX")"
gnupg_home="${GNUPGHOME:-}"
if [[ -z "$gnupg_home" ]]; then
  gnupg_home="$(mktemp -d "$runner_temp/ricochet-linux-gnupg.XXXXXX")"
fi
key_file="$secret_dir/private-key.gpg"
ownertrust_file="$secret_dir/ownertrust.txt"

cleanup() {
  rm -rf "$secret_dir"
}
trap cleanup EXIT

mkdir -p "$gnupg_home"
chmod 700 "$gnupg_home"
export GNUPGHOME="$gnupg_home"

decode_base64_value "$key_base64" "$key_file"
gpg --batch --import "$key_file" >/dev/null

if [[ -n "$ownertrust_base64" ]]; then
  decode_base64_value "$ownertrust_base64" "$ownertrust_file"
  gpg --batch --import-ownertrust "$ownertrust_file" >/dev/null
fi

if [[ -n "$requested_key" ]]; then
  if ! gpg --batch --list-secret-keys "$requested_key" >/dev/null 2>&1; then
    echo "RICOCHET_LINUX_GPG_KEY does not match an imported secret key." >&2
    exit 1
  fi
  selected_key="$(gpg --batch --with-colons --list-secret-keys "$requested_key" | awk -F: 'BEGIN { want = 0 } /^sec:/ { want = 1; next } want && /^fpr:/ { print $10; exit }')"
else
  selected_key="$(gpg --batch --with-colons --list-secret-keys | awk -F: 'BEGIN { want = 0 } /^sec:/ { want = 1; next } want && /^fpr:/ { print $10; want = 0 }')"
  key_count="$(printf '%s\n' "$selected_key" | sed '/^$/d' | wc -l | tr -d ' ')"
  if [[ "$key_count" != "1" ]]; then
    echo "Imported $key_count Linux GPG secret keys; set RICOCHET_LINUX_GPG_KEY to choose one." >&2
    exit 1
  fi
fi

if [[ -z "$selected_key" ]]; then
  echo "Could not resolve an imported Linux GPG signing fingerprint." >&2
  exit 1
fi

append_env_output "GNUPGHOME" "$GNUPGHOME"
append_env_output "RICOCHET_LINUX_GPG_KEY" "$selected_key"
echo "Imported Linux GPG release signing key."
echo "RICOCHET_LINUX_GPG_KEY=$selected_key"
