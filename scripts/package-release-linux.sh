#!/usr/bin/env bash
set -euo pipefail

version=""
target="linux-x64"
out_dir="dist"
configuration="release"
skip_build=0
build_deb=1
signature_mode="auto"
linux_gpg_key="${RICOCHET_LINUX_GPG_KEY:-}"

usage() {
  cat <<'EOF'
Usage: scripts/package-release-linux.sh [options]

Options:
  --version <version>       Release version. Defaults to workspace.package.
  --target <target>         Package target label. Defaults to linux-x64.
  --out-dir <path>          Output directory. Defaults to dist.
  --configuration <name>    Cargo profile directory. Defaults to release.
  --skip-build              Reuse existing target/<configuration> binaries.
  --no-deb                  Skip Debian package creation.
  --signature-mode <mode>   auto, require, skip, or dry-run. Defaults to auto.
  --gpg-key <key-id>        GPG signing key. Defaults to RICOCHET_LINUX_GPG_KEY.
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
    --no-deb)
      build_deb=0
      shift
      ;;
    --signature-mode)
      signature_mode="${2:?--signature-mode requires a value}"
      shift 2
      ;;
    --gpg-key)
      linux_gpg_key="${2:?--gpg-key requires a value}"
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
  Linux) ;;
  *)
    echo "This package script must run on Linux so it can package Linux executables." >&2
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
    *.deb) echo "debian-package" ;;
    *.asc) echo "detached-signature" ;;
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
      local name kind size sha signed_artifact signature
      name="$(basename -- "$artifact")"
      kind="$(artifact_kind "$artifact")"
      size="$(stat -c '%s' "$artifact")"
      sha="$(sha256sum "$artifact" | awk '{ print $1 }')"
      signed_artifact=""
      signature=""
      if [[ "$kind" == "detached-signature" ]]; then
        signed_artifact="$(basename -- "${artifact%.asc}")"
      elif [[ -f "${artifact}.asc" ]]; then
        signature="$(basename -- "${artifact}.asc")"
      fi

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
      if [[ -n "$signed_artifact" ]]; then
        printf ',\n      "signed_artifact": "%s"' "$(json_escape "$signed_artifact")"
      fi
      if [[ -n "$signature" ]]; then
        printf ',\n      "signature": "%s"' "$(json_escape "$signature")"
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

sign_linux_assets() {
  local paths=("$@")
  signature_assets=()
  append_signing_report "[detached signatures]"
  append_signing_report "mode = $signature_mode"

  case "$signature_mode" in
    skip)
      append_signing_report "status = skipped" "reason = signature mode is skip"
      return
      ;;
    dry-run)
      append_signing_report "status = dry-run"
      if [[ -n "$linux_gpg_key" ]]; then
        append_signing_report "gpg_key = $linux_gpg_key"
      fi
      for path in "${paths[@]}"; do
        append_signing_report "would_sign = $path"
      done
      return
      ;;
  esac

  missing=()
  if [[ -z "$linux_gpg_key" ]]; then
    missing+=("RICOCHET_LINUX_GPG_KEY or --gpg-key is not set")
  fi
  if ! command -v gpg >/dev/null 2>&1; then
    missing+=("gpg is not available")
  fi

  if [[ "${#missing[@]}" -gt 0 ]]; then
    local reason
    reason="$(IFS='; '; echo "${missing[*]}")"
    if [[ "$signature_mode" == "require" ]]; then
      echo "Linux signing prerequisites missing: $reason" >&2
      exit 1
    fi
    echo "Warning: Linux detached signatures skipped: $reason. Continuing unsigned because --signature-mode auto permits beta/nightly fallback." >&2
    append_signing_report "status = unsigned-fallback" "reason = $reason"
    return
  fi

  for path in "${paths[@]}"; do
    local signature_path="${path}.asc"
    assert_new_path "$signature_path"
    gpg --batch --yes --armor --detach-sign --local-user "$linux_gpg_key" --output "$signature_path" "$path"
    append_signing_report "signed = $path" "signature = $signature_path"
    signature_assets+=("$signature_path")
  done
  append_signing_report "status = signed" "gpg_key = $linux_gpg_key"
}

write_linux_metadata() {
  local root="$1"
  local version="$2"
  local share_dir="$root/share"

  mkdir -p \
    "$share_dir/applications" \
    "$share_dir/icons/hicolor/scalable/apps" \
    "$share_dir/metainfo"

  cat > "$share_dir/applications/ricochet-repl.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Ricochet REPL
Comment=Open the Ricochet interactive language shell
Exec=rco repl
Icon=ricochet
Terminal=true
Categories=Development;Utility;
StartupNotify=false
EOF

  cat > "$share_dir/icons/hicolor/scalable/apps/ricochet.svg" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
  <rect width="128" height="128" rx="24" fill="#1f2937"/>
  <path d="M28 36h72v16H48v18h42v16H48v30H28z" fill="#f8fafc"/>
  <text x="64" y="112" text-anchor="middle" font-family="Arial, sans-serif" font-size="22" font-weight="700" fill="#38bdf8">Rc</text>
</svg>
EOF

  cat > "$share_dir/metainfo/today.ricochet.rco.metainfo.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<component type="console-application">
  <id>today.ricochet.rco</id>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>MIT</project_license>
  <name>Ricochet</name>
  <summary>Pure-postfix language, MVC web framework, and desktop app toolkit</summary>
  <description>
    <p>Ricochet packages the rco command-line tools, local reference documentation, examples, and first-party package catalog.</p>
  </description>
  <provides>
    <binary>rco</binary>
    <binary>ricochet</binary>
    <binary>rco-gui</binary>
    <binary>rco-app</binary>
  </provides>
  <releases>
    <release version="$version" />
  </releases>
</component>
EOF
}

package_name="ricochet-v${version}-${target}"
if [[ "$out_dir" = /* ]]; then
  out_dir_path="$out_dir"
else
  out_dir_path="$repo_root/$out_dir"
fi
package_dir="$out_dir_path/$package_name"
archive_path="$out_dir_path/${package_name}.tar.gz"
deb_path="$out_dir_path/ricochet_${version}_amd64.deb"
checksums_path="$out_dir_path/SHA256SUMS-${target}.txt"
signing_report_path="$out_dir_path/SIGNING-${target}.txt"
manifest_path="$out_dir_path/ARTIFACTS-${target}.json"

assert_new_path "$package_dir"
assert_new_path "$archive_path"
assert_new_path "$checksums_path"
assert_new_path "$signing_report_path"
assert_new_path "$manifest_path"
if [[ "$build_deb" -eq 1 ]]; then
  assert_new_path "$deb_path"
fi

mkdir -p "$out_dir_path"
validate_mode "--signature-mode" "$signature_mode"
{
  echo "Ricochet Linux signing report"
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
  "$target_dir/rco-app"
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
install -m 755 "${binaries[2]}" "$package_dir/rco-app"
install -m 755 "${binaries[3]}" "$package_dir/ricochet"
cp "$repo_root/README.md" "$package_dir/README.md"
cp "$repo_root/LICENSE" "$package_dir/LICENSE"
copy_release_directory "$repo_root/examples" "$package_dir/examples"
copy_release_directory "$repo_root/packages" "$package_dir/packages"
copy_release_directory "$repo_root/docs/assets" "$package_dir/docs/assets"
copy_release_directory "$repo_root/docs/reference" "$package_dir/docs/reference"
copy_release_directory "$repo_root/editors/vscode" "$package_dir/editors/vscode"
write_linux_metadata "$package_dir" "$version"

cat > "$package_dir/RELEASE.txt" <<EOF
Ricochet v$version ($target)

Commands:
  rco --help
  rco gui examples/webview_ui.rco
  rco package examples/webview_ui.rco --gui --output webview-ui
  rco package examples/native_showcase_app.rco --app --backend slint --output native-showcase
  ricochet --help

Install locally:
  ./install.sh

Set PREFIX to install somewhere other than \$HOME/.local:
  PREFIX=/usr/local sudo -E ./install.sh
EOF
cat > "$package_dir/CHANGELOG.txt" <<EOF
ricochet ($version)

  * Ricochet release package for $target.
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
cp "$script_dir/rco-app" "$bin_dir/rco-app"
cp "$script_dir/ricochet" "$bin_dir/ricochet"
chmod 755 "$bin_dir/rco" "$bin_dir/rco-gui" "$bin_dir/rco-app" "$bin_dir/ricochet"

share_dir="$prefix/share"
if [ -d "$script_dir/share/applications" ]; then
  mkdir -p "$share_dir/applications"
  cp "$script_dir/share/applications/"*.desktop "$share_dir/applications/"
fi
if [ -d "$script_dir/share/metainfo" ]; then
  mkdir -p "$share_dir/metainfo"
  cp "$script_dir/share/metainfo/"*.metainfo.xml "$share_dir/metainfo/"
fi
if [ -d "$script_dir/share/icons/hicolor/scalable/apps" ]; then
  mkdir -p "$share_dir/icons/hicolor/scalable/apps"
  cp "$script_dir/share/icons/hicolor/scalable/apps/"*.svg "$share_dir/icons/hicolor/scalable/apps/"
fi

printf 'Installed Ricochet CLI tools to %s\n' "$bin_dir"
printf 'Make sure %s is on your PATH.\n' "$bin_dir"
EOF
chmod 755 "$package_dir/install.sh"

tar -czf "$archive_path" -C "$out_dir_path" "$package_name"

assets=("$archive_path")

if [[ "$build_deb" -eq 1 ]]; then
  deb_root="$out_dir_path/deb-root"
  assert_new_path "$deb_root"

  mkdir -p \
    "$deb_root/DEBIAN" \
    "$deb_root/usr/bin" \
    "$deb_root/usr/share/doc/ricochet" \
    "$deb_root/usr/share/ricochet"

  install -m 755 "${binaries[0]}" "$deb_root/usr/bin/rco"
  install -m 755 "${binaries[1]}" "$deb_root/usr/bin/rco-gui"
  install -m 755 "${binaries[2]}" "$deb_root/usr/bin/rco-app"
  install -m 755 "${binaries[3]}" "$deb_root/usr/bin/ricochet"
  cp "$repo_root/README.md" "$deb_root/usr/share/doc/ricochet/README.md"
  cp "$repo_root/LICENSE" "$deb_root/usr/share/doc/ricochet/LICENSE"
  cat > "$deb_root/usr/share/doc/ricochet/changelog" <<EOF
ricochet ($version)

  * Ricochet release package for $target.
EOF
  copy_release_directory "$repo_root/examples" "$deb_root/usr/share/ricochet/examples"
  copy_release_directory "$repo_root/packages" "$deb_root/usr/share/ricochet/packages"
  copy_release_directory "$repo_root/docs/reference" "$deb_root/usr/share/doc/ricochet/reference"
  copy_release_directory "$repo_root/editors/vscode" "$deb_root/usr/share/ricochet/editors/vscode"
  write_linux_metadata "$deb_root/usr" "$version"

  installed_size="$(du -sk "$deb_root/usr" | awk '{ print $1 }')"
  cat > "$deb_root/DEBIAN/control" <<EOF
Package: ricochet
Version: $version
Section: devel
Priority: optional
Architecture: amd64
Depends: xdg-utils
Maintainer: Ricochet <noreply@ricochet.today>
Installed-Size: $installed_size
Description: Ricochet stack-based web language CLI
 Ricochet is a pure-postfix, stack-based programming language with a Rust
 bytecode VM, CLI scripting, MVC web scaffolding, beta Active Record support,
 and a desktop GUI launcher that opens Linux GUI apps in the system browser.
EOF

  dpkg-deb --build "$deb_root" "$deb_path"
  assets+=("$deb_path")
fi

sign_linux_assets "${assets[@]}"
assets+=("${signature_assets[@]}" "$signing_report_path")

{
  for asset in "${assets[@]}"; do
    sha256sum "$asset" | sed "s#  .*/#  #"
  done
} > "$checksums_path"
write_artifact_manifest "$manifest_path" "${assets[@]}" "$checksums_path"

echo "Release assets written to $out_dir_path"
for asset in "${assets[@]}"; do
  echo " - $asset"
done
echo " - $checksums_path"
echo " - $manifest_path"
