#!/usr/bin/env bash
set -euo pipefail

version=""
target="macos-arm64"
out_dir="dist"
configuration="release"
skip_build=0

usage() {
  cat <<'EOF'
Usage: scripts/package-release-macos.sh [options]

Options:
  --version <version>       Release version. Defaults to workspace.package.
  --target <target>         Package target label. Defaults to macos-arm64.
  --out-dir <path>          Output directory. Defaults to dist.
  --configuration <name>    Cargo profile directory. Defaults to release.
  --skip-build              Reuse existing target/<configuration> binaries.
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

copy_release_directory() {
  local source="$1"
  local destination="$2"

  if [[ -d "$source" ]]; then
    mkdir -p "$(dirname -- "$destination")"
    cp -R "$source" "$destination"
  fi
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

assert_new_path "$package_dir"
assert_new_path "$archive_path"
assert_new_path "$checksums_path"

mkdir -p "$out_dir_path"

if [[ "$skip_build" -eq 0 ]]; then
  pushd "$repo_root" >/dev/null
  cargo build -p ricochet_cli "--$configuration" --locked
  popd >/dev/null
fi

target_dir="$repo_root/target/$configuration"
binaries=(
  "$target_dir/rco"
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
install -m 755 "${binaries[1]}" "$package_dir/ricochet"
cp "$repo_root/README.md" "$package_dir/README.md"
cp "$repo_root/LICENSE" "$package_dir/LICENSE"
copy_release_directory "$repo_root/examples" "$package_dir/examples"
copy_release_directory "$repo_root/docs/assets" "$package_dir/docs/assets"
copy_release_directory "$repo_root/docs/reference" "$package_dir/docs/reference"

cat > "$package_dir/RELEASE.txt" <<EOF
Ricochet v$version ($target)

Commands:
  rco --help
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
cp "$script_dir/ricochet" "$bin_dir/ricochet"
chmod 755 "$bin_dir/rco" "$bin_dir/ricochet"

if command -v xattr >/dev/null 2>&1; then
  xattr -d com.apple.quarantine "$bin_dir/rco" "$bin_dir/ricochet" 2>/dev/null || true
fi

printf 'Installed Ricochet CLI tools to %s\n' "$bin_dir"
printf 'Make sure %s is on your PATH.\n' "$bin_dir"
EOF
chmod 755 "$package_dir/install.sh"

tar -czf "$archive_path" -C "$out_dir_path" "$package_name"

shasum -a 256 "$archive_path" | sed "s#  .*/#  #" > "$checksums_path"

echo "Release assets written to $out_dir_path"
echo " - $archive_path"
echo " - $checksums_path"
