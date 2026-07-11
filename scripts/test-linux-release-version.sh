#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
case_number=0

for invalid in dev ../escape 1.2 1.2.3-01 01.2.3 "1.2.3 rc.1"; do
  case_number=$((case_number + 1))
  out="$repo_root/target/invalid-release-version-${case_number}-$$"
  test ! -e "$out"

  if bash "$script_dir/package-release-linux.sh" \
    --version "$invalid" \
    --out-dir "$out" \
    --skip-build \
    --no-deb \
    --signature-mode skip >/dev/null 2>&1; then
    echo "Malformed release version was accepted: $invalid" >&2
    exit 1
  fi

  test ! -e "$out"
done

printf 'Linux release version guard rejected %s malformed values before artifact creation.\n' "$case_number"
