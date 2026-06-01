#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../../.." && pwd -P)"

cd "$repo_root"

private_manifest="wrappers/rust/arcadia-tio-rs/Cargo.toml"
public_manifest="crates/arcadia-tio-rs/Cargo.toml"
if [[ -f "$private_manifest" ]]; then
  crate_manifest="$private_manifest"
  source_glob="wrappers/rust/arcadia-tio-rs/examples/tutorials/[0-9][0-9]_*.rs"
  private_source=1
elif [[ -f "$public_manifest" ]]; then
  crate_manifest="$public_manifest"
  source_glob="crates/arcadia-tio-rs/examples/tutorials/[0-9][0-9]_*.rs"
  private_source=0
else
  echo "Could not find arcadia-tio-rs Cargo.toml in private or public layout" >&2
  exit 1
fi

if [[ "$private_source" == "1" && "${TIO_TUTORIAL_RUST_SKIP_NATIVE_BUILD:-0}" != "1" ]]; then
  cargo build --package arcadia-tio-capi --release
fi

if [[ -n "${ARCADIA_TIO_CAPI_LIB_DIR:-}" ]]; then
  lib_dir="$ARCADIA_TIO_CAPI_LIB_DIR"
elif [[ "$private_source" == "1" ]]; then
  target_root="${CARGO_TARGET_DIR:-$repo_root/target}"
  lib_dir="$target_root/release"
else
  host="$(rustc -vV | awk '/^host:/ { print $2; exit }')"
  lib_dir="$repo_root/native/$host/lib"
fi
export ARCADIA_TIO_CAPI_LIB_DIR="$lib_dir"

case "$(uname -s 2>/dev/null || echo unknown)" in
  Darwin*)
    export DYLD_LIBRARY_PATH="$lib_dir${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
    ;;
  MINGW*|MSYS*|CYGWIN*)
    export PATH="$lib_dir${PATH:+:$PATH}"
    ;;
  *)
    export LD_LIBRARY_PATH="$lib_dir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    ;;
esac

# Expand after selecting the private or public source layout.
# shellcheck disable=SC2206 # Intentional glob expansion into an array.
sources=($source_glob)
if [[ ! -e "${sources[0]}" ]]; then
  echo "No Rust tutorial sources found" >&2
  exit 1
fi

for source in "${sources[@]}"; do
  stem="$(basename "${source%.rs}")"
  example="tutorial_${stem}"
  echo "==> cargo run --manifest-path $crate_manifest --example $example"
  cargo run --manifest-path "$crate_manifest" --example "$example"
done

echo "Rust tutorial runner passed (${#sources[@]} examples)."
