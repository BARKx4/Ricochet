#!/usr/bin/env bash
set -euo pipefail

version=""
target="macos-arm64"
out_dir="dist"
configuration="release"
skip_build=0
signing_mode="auto"
sign_identity="${RICOCHET_MACOS_SIGN_IDENTITY:-}"
notarization_mode="auto"
notary_profile="${RICOCHET_MACOS_NOTARY_PROFILE:-}"

usage() {
  cat <<'EOF'
Usage: scripts/package-release-macos.sh [options]

Options:
  --version <version>       Release version. Defaults to workspace.package.
  --target <target>         Package target label. Defaults to macos-arm64.
  --out-dir <path>          Output directory. Defaults to dist.
  --configuration <name>    Cargo profile directory. Defaults to release.
  --skip-build              Reuse existing target/<configuration> binaries.
  --signing-mode <mode>     auto, require, skip, or dry-run. Defaults to auto.
  --sign-identity <name>    codesign identity. Defaults to RICOCHET_MACOS_SIGN_IDENTITY.
  --notarization-mode <mode>
                            auto, require, skip, or dry-run. Defaults to auto.
  --notary-profile <name>   notarytool keychain profile. Defaults to RICOCHET_MACOS_NOTARY_PROFILE.
  -h, --help                Show this help.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      version="${2:?--version requires a value}"
      shift 2
      ;;
    --target)
      target="${2:?--target requires a value}"
      shift 2
      ;;
    --out-dir)
      out_dir="${2:?--out-dir requires a value}"
      shift 2
      ;;
    --configuration)
      configuration="${2:?--configuration requires a value}"
      shift 2
      ;;
    --skip-build)
      skip_build=1
      shift
      ;;
    --signing-mode)
      signing_mode="${2:?--signing-mode requires a value}"
      shift 2
      ;;
    --sign-identity)
      sign_identity="${2:?--sign-identity requires a value}"
      shift 2
      ;;
    --notarization-mode)
      notarization_mode="${2:?--notarization-mode requires a value}"
      shift 2
      ;;
    --notary-profile)
      notary_profile="${2:?--notary-profile requires a value}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"

case "$(uname -s)" in
  Darwin) ;;
  *)
    echo "This package script must run on macOS so it can package macOS executables." >&2
    exit 1
    ;;
esac

workspace_version() {
  awk -F '"' '
    /^\[workspace\.package\]/ { in_workspace_package = 1; next }
    /^\[/ { if (in_workspace_package) exit }
    in_workspace_package && /^[[:space:]]*version[[:space:]]*=/ { print $2; exit }
  ' "$repo_root/Cargo.toml"
}

if [[ -z "$version" ]]; then
  version="$(workspace_version)"
fi
version="${version#v}"
if [[ -z "$version" ]]; then
  echo "Release version must not be empty." >&2
  exit 1
fi

assert_new_path() {
  local path="$1"
  if [[ -e "$path" ]]; then
    echo "$path already exists. Choose a fresh --out-dir or remove the existing artifact first." >&2
    exit 1
  fi
}

validate_mode() {
  local name="$1"
  local value="$2"
  case "$value" in
    auto|require|skip|dry-run) ;;
    *)
      echo "$name must be one of: auto, require, skip, dry-run." >&2
      exit 2
      ;;
  esac
}

copy_release_directory() {
  local source="$1"
  local destination="$2"

  if [[ -d "$source" ]]; then
    mkdir -p "$(dirname -- "$destination")"
    cp -R "$source" "$destination"
  fi
}

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\r'/\\r}"
  value="${value//$'\n'/\\n}"
  value="${value//$'\t'/\\t}"
  printf '%s' "$value"
}

artifact_kind() {
  local path="$1"
  case "$(basename -- "$path")" in
    *.tar.gz) echo "archive" ;;
    SIGNING-*.txt) echo "signing-report" ;;
    SHA256SUMS*.txt) echo "checksums" ;;
    *) echo "artifact" ;;
  esac
}

source_value() {
  local env_value="$1"
  local git_args="$2"
  if [[ -n "$env_value" ]]; then
    printf '%s' "$env_value"
    return
  fi
  git -C "$repo_root" $git_args 2>/dev/null || true
}

write_artifact_manifest() {
  local manifest_path="$1"
  shift
  local artifacts=("$@")
  local source_commit source_ref generated_at
  source_commit="$(source_value "${GITHUB_SHA:-}" "rev-parse HEAD")"
  source_ref="$(source_value "${GITHUB_REF_NAME:-${GITHUB_REF:-}}" "rev-parse --abbrev-ref HEAD")"
  generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

  {
    printf '{\n'
    printf '  "schema": "ricochet.release-artifacts",\n'
    printf '  "schema_version": 1,\n'
    printf '  "target": "%s",\n' "$(json_escape "$target")"
    printf '  "package_version": "%s",\n' "$(json_escape "$version")"
    printf '  "generated_at": "%s",\n' "$(json_escape "$generated_at")"
    printf '  "source": {\n'
    printf '    "commit": %s,\n' "$(if [[ -n "$source_commit" ]]; then printf '"%s"' "$(json_escape "$source_commit")"; else printf 'null'; fi)"
    printf '    "ref": %s\n' "$(if [[ -n "$source_ref" ]]; then printf '"%s"' "$(json_escape "$source_ref")"; else printf 'null'; fi)"
    printf '  },\n'
    printf '  "artifacts": [\n'
    local first=1
    for artifact in "${artifacts[@]}"; do
      local name kind size sha
      name="$(basename -- "$artifact")"
      kind="$(artifact_kind "$artifact")"
      size="$(stat -f '%z' "$artifact")"
      sha="$(shasum -a 256 "$artifact" | awk '{ print $1 }')"

      if [[ "$first" -eq 0 ]]; then
        printf ',\n'
      fi
      first=0
      printf '    {\n'
      printf '      "name": "%s",\n' "$(json_escape "$name")"
      printf '      "path": "%s",\n' "$(json_escape "$name")"
      printf '      "kind": "%s",\n' "$(json_escape "$kind")"
      printf '      "size_bytes": %s,\n' "$size"
      printf '      "sha256": "%s"' "$(json_escape "$sha")"
      if [[ "$name" != "$(basename -- "$signing_report_path")" ]]; then
        printf ',\n      "signing_report": "%s"' "$(json_escape "$(basename -- "$signing_report_path")")"
      fi
      printf '\n    }'
    done
    printf '\n  ]\n'
    printf '}\n'
  } > "$manifest_path"
}

append_signing_report() {
  printf '%s\n' "$@" >> "$signing_report_path"
}

sign_macos_binaries() {
  local paths=("$@")
  append_signing_report "[staged executables]"
  append_signing_report "mode = $signing_mode"

  case "$signing_mode" in
    skip)
      append_signing_report "status = skipped" "reason = signing mode is skip"
      return
      ;;
    dry-run)
      append_signing_report "status = dry-run"
      if [[ -n "$sign_identity" ]]; then
        append_signing_report "identity = $sign_identity"
      fi
      for path in "${paths[@]}"; do
        append_signing_report "would_sign = $path"
      done
      return
      ;;
  esac

  missing=()
  if [[ -z "$sign_identity" ]]; then
    missing+=("RICOCHET_MACOS_SIGN_IDENTITY or --sign-identity is not set")
  fi
  if ! command -v codesign >/dev/null 2>&1; then
    missing+=("codesign is not available")
  fi

  if [[ "${#missing[@]}" -gt 0 ]]; then
    local reason
    reason="$(IFS='; '; echo "${missing[*]}")"
    if [[ "$signing_mode" == "require" ]]; then
      echo "macOS signing prerequisites missing: $reason" >&2
      exit 1
    fi
    echo "Warning: macOS signing skipped: $reason. Continuing unsigned because --signing-mode auto permits beta/nightly fallback." >&2
    append_signing_report "status = unsigned-fallback" "reason = $reason"
    return
  fi

  for path in "${paths[@]}"; do
    codesign --force --timestamp --options runtime --sign "$sign_identity" "$path"
    append_signing_report "signed = $path"
  done
  append_signing_report "status = signed" "identity = $sign_identity"
}

notarize_macos_archive() {
  local archive="$1"
  append_signing_report "[notarization]"
  append_signing_report "mode = $notarization_mode"

  case "$notarization_mode" in
    skip)
      append_signing_report "status = skipped" "reason = notarization mode is skip"
      return
      ;;
    dry-run)
      append_signing_report "status = dry-run" "would_notarize = $archive"
      if [[ -n "$notary_profile" ]]; then
        append_signing_report "notary_profile = $notary_profile"
      fi
      return
      ;;
  esac

  missing=()
  if [[ -z "$notary_profile" ]]; then
    missing+=("RICOCHET_MACOS_NOTARY_PROFILE or --notary-profile is not set")
  fi
  if ! command -v xcrun >/dev/null 2>&1; then
    missing+=("xcrun is not available")
  fi

  if [[ "${#missing[@]}" -gt 0 ]]; then
    local reason
    reason="$(IFS='; '; echo "${missing[*]}")"
    if [[ "$notarization_mode" == "require" ]]; then
      echo "macOS notarization prerequisites missing: $reason" >&2
      exit 1
    fi
    echo "Warning: macOS notarization skipped: $reason. Continuing because --notarization-mode auto permits beta/nightly fallback." >&2
    append_signing_report "status = skipped" "reason = $reason"
    return
  fi

  xcrun notarytool submit "$archive" --keychain-profile "$notary_profile" --wait
  append_signing_report "status = notarized" "archive = $archive" "notary_profile = $notary_profile"
}

package_name="ricochet-v${version}-${target}"
if [[ "$out_dir" = /* ]]; then
  out_dir_path="$out_dir"
else
  out_dir_path="$repo_root/$out_dir"
fi
package_dir="$out_dir_path/$package_name"
archive_path="$out_dir_path/${package_name}.tar.gz"
checksums_path="$out_dir_path/SHA256SUMS-${target}.txt"
signing_report_path="$out_dir_path/SIGNING-${target}.txt"
manifest_path="$out_dir_path/ARTIFACTS-${target}.json"

assert_new_path "$package_dir"
assert_new_path "$archive_path"
assert_new_path "$checksums_path"
assert_new_path "$signing_report_path"
assert_new_path "$manifest_path"

mkdir -p "$out_dir_path"
validate_mode "--signing-mode" "$signing_mode"
validate_mode "--notarization-mode" "$notarization_mode"
{
  echo "Ricochet macOS signing report"
  echo "version = $version"
  echo "target = $target"
} > "$signing_report_path"

if [[ "$skip_build" -eq 0 ]]; then
  pushd "$repo_root" >/dev/null
  cargo build -p ricochet_cli "--$configuration" --locked
  popd >/dev/null
fi

target_dir="$repo_root/target/$configuration"
binaries=(
  "$target_dir/rco"
  "$target_dir/rco-gui"
  "$target_dir/ricochet"
)

for binary in "${binaries[@]}"; do
  if [[ ! -f "$binary" ]]; then
    echo "Expected release binary was not found: $binary" >&2
    exit 1
  fi
done

mkdir -p "$package_dir"
install -m 755 "${binaries[0]}" "$package_dir/rco"
install -m 755 "${binaries[1]}" "$package_dir/rco-gui"
install -m 755 "${binaries[2]}" "$package_dir/ricochet"
sign_macos_binaries "$package_dir/rco" "$package_dir/rco-gui" "$package_dir/ricochet"
cp "$repo_root/README.md" "$package_dir/README.md"
cp "$repo_root/LICENSE" "$package_dir/LICENSE"
copy_release_directory "$repo_root/examples" "$package_dir/examples"
copy_release_directory "$repo_root/packages" "$package_dir/packages"
copy_release_directory "$repo_root/docs/assets" "$package_dir/docs/assets"
copy_release_directory "$repo_root/docs/reference" "$package_dir/docs/reference"
copy_release_directory "$repo_root/editors/vscode" "$package_dir/editors/vscode"

cat > "$package_dir/RELEASE.txt" <<EOF
Ricochet v$version ($target)

Commands:
  rco --help
  rco gui examples/webview_ui.rco
  rco package examples/webview_ui.rco --gui --output webview-ui
  ricochet --help

This is an unsigned developer beta tarball. It is not notarized by Apple.

Install locally:
  ./install.sh

Set PREFIX to install somewhere other than \$HOME/.local:
  PREFIX=/usr/local sudo -E ./install.sh
EOF

cat > "$package_dir/install.sh" <<'EOF'
#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix="${PREFIX:-$HOME/.local}"
bin_dir="$prefix/bin"

mkdir -p "$bin_dir"
cp "$script_dir/rco" "$bin_dir/rco"
cp "$script_dir/rco-gui" "$bin_dir/rco-gui"
cp "$script_dir/ricochet" "$bin_dir/ricochet"
chmod 755 "$bin_dir/rco" "$bin_dir/rco-gui" "$bin_dir/ricochet"

if command -v xattr >/dev/null 2>&1; then
  xattr -d com.apple.quarantine "$bin_dir/rco" "$bin_dir/rco-gui" "$bin_dir/ricochet" 2>/dev/null || true
fi

printf 'Installed Ricochet CLI tools to %s\n' "$bin_dir"
printf 'Make sure %s is on your PATH.\n' "$bin_dir"
EOF
chmod 755 "$package_dir/install.sh"

tar -czf "$archive_path" -C "$out_dir_path" "$package_name"
notarize_macos_archive "$archive_path"

shasum -a 256 "$archive_path" "$signing_report_path" | sed "s#  .*/#  #" > "$checksums_path"
write_artifact_manifest "$manifest_path" "$archive_path" "$signing_report_path" "$checksums_path"

echo "Release assets written to $out_dir_path"
echo " - $archive_path"
echo " - $signing_report_path"
echo " - $checksums_path"
echo " - $manifest_path"
