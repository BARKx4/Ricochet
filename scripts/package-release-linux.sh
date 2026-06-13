#!/usr/bin/env bash
set -euo pipefail

version=""
target="linux-x64"
out_dir="dist"
configuration="release"
skip_build=0
build_deb=1

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
deb_path="$out_dir_path/ricochet_${version}_amd64.deb"
checksums_path="$out_dir_path/SHA256SUMS-${target}.txt"

assert_new_path "$package_dir"
assert_new_path "$archive_path"
assert_new_path "$checksums_path"
if [[ "$build_deb" -eq 1 ]]; then
  assert_new_path "$deb_path"
fi

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
  install -m 755 "${binaries[1]}" "$deb_root/usr/bin/ricochet"
  cp "$repo_root/README.md" "$deb_root/usr/share/doc/ricochet/README.md"
  cp "$repo_root/LICENSE" "$deb_root/usr/share/doc/ricochet/LICENSE"
  copy_release_directory "$repo_root/examples" "$deb_root/usr/share/ricochet/examples"
  copy_release_directory "$repo_root/docs/reference" "$deb_root/usr/share/doc/ricochet/reference"

  installed_size="$(du -sk "$deb_root/usr" | awk '{ print $1 }')"
  cat > "$deb_root/DEBIAN/control" <<EOF
Package: ricochet
Version: $version
Section: devel
Priority: optional
Architecture: amd64
Maintainer: Ricochet <noreply@ricochet.today>
Installed-Size: $installed_size
Description: Ricochet stack-based web language CLI
 Ricochet is a pure-postfix, stack-based programming language with a Rust
 bytecode VM, CLI scripting, MVC web scaffolding, and beta Active Record
 support.
EOF

  dpkg-deb --build "$deb_root" "$deb_path"
  assets+=("$deb_path")
fi

{
  for asset in "${assets[@]}"; do
    sha256sum "$asset" | sed "s#  .*/#  #"
  done
} > "$checksums_path"

echo "Release assets written to $out_dir_path"
for asset in "${assets[@]}"; do
  echo " - $asset"
done
echo " - $checksums_path"
