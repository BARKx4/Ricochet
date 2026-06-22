#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/cleanup-macos-signing-keychain.sh

Restores the user keychain list/default keychain after
setup-macos-signing-keychain.sh and deletes the generated temporary signing
keychain. The script is safe to run when no macOS signing keychain was prepared.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -gt 0 ]]; then
  echo "Unknown argument: $1" >&2
  usage >&2
  exit 2
fi

case "$(uname -s)" in
  Darwin) ;;
  *)
    echo "macOS signing keychain cleanup skipped because this is not macOS."
    exit 0
    ;;
esac

keychain_path="${RICOCHET_MACOS_KEYCHAIN_PATH:-}"
previous_default="${RICOCHET_MACOS_PREVIOUS_DEFAULT_KEYCHAIN:-}"
previous_list_file="${RICOCHET_MACOS_PREVIOUS_KEYCHAIN_LIST_FILE:-}"
runner_temp="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
runner_temp="$(cd "$runner_temp" && pwd -P)"

if [[ -z "$keychain_path" ]]; then
  echo "macOS signing keychain cleanup skipped because RICOCHET_MACOS_KEYCHAIN_PATH is not set."
  exit 0
fi

keychain_dir="$(dirname -- "$keychain_path")"
keychain_basename="$(basename -- "$keychain_path")"
keychain_dir_basename="$(basename -- "$keychain_dir")"
keychain_parent="$(cd "$(dirname -- "$keychain_dir")" && pwd -P)"

if [[ "$keychain_basename" != "ricochet-signing.keychain-db" ]] ||
  [[ "$keychain_dir_basename" != ricochet-macos-keychain.* ]] ||
  [[ "$keychain_parent" != "$runner_temp" ]]; then
  echo "Refusing to clean up unexpected macOS keychain path: $keychain_path" >&2
  exit 1
fi

restore_keychains=()
if [[ -n "$previous_list_file" && -f "$previous_list_file" ]]; then
  while IFS= read -r line; do
    if [[ -n "$line" ]]; then
      restore_keychains+=("$line")
    fi
  done < "$previous_list_file"
else
  while IFS= read -r line; do
    cleaned="$(printf '%s' "$line" | sed -e 's/^[[:space:]]*"//' -e 's/"$//')"
    if [[ -n "$cleaned" && "$cleaned" != "$keychain_path" ]]; then
      restore_keychains+=("$cleaned")
    fi
  done < <(security list-keychains -d user)
fi

if [[ "${#restore_keychains[@]}" -gt 0 ]]; then
  security list-keychains -d user -s "${restore_keychains[@]}" || true
fi

if [[ -n "$previous_default" ]]; then
  security default-keychain -d user -s "$previous_default" || true
fi

if [[ -e "$keychain_path" ]]; then
  security delete-keychain "$keychain_path" || rm -f "$keychain_path"
fi

rm -rf "$keychain_dir"

echo "Cleaned up macOS signing keychain."
