#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../../.." && pwd -P)"

cd "$repo_root"

tmp_root="$repo_root/.tmp/tutorials/rust-run"
mkdir -p "$tmp_root"
run_dir="$(mktemp -d "$tmp_root/run.XXXXXX")"
process_tmp="$run_dir/process"
data_tmp="$run_dir/data"
mkdir -p "$process_tmp" "$data_tmp"
cleanup() {
  local status=$?
  rm -rf -- "$run_dir"
  exit "$status"
}
trap cleanup EXIT INT TERM
export TMPDIR="$process_tmp"
export TEMP="$process_tmp"
export TMP="$process_tmp"
export TIO_TUTORIAL_TMPDIR="$data_tmp"

crate_manifest="crates/arcadia-tio-rs/Cargo.toml"
source_glob="crates/arcadia-tio-rs/examples/tutorials/[0-9][0-9]_*.rs"
if [[ ! -f "$crate_manifest" ]]; then
  echo "Could not find an exported arcadia-tio-rs workspace" >&2
  exit 1
fi

if [[ -n "${ARCADIA_TIO_CAPI_LIB_DIR:-}" ]]; then
  lib_dir="$ARCADIA_TIO_CAPI_LIB_DIR"
else
  rustc_version_verbose="$(rustc -vV)"
  host="$(awk '/^host:/ { print $2; exit }' <<<"$rustc_version_verbose")"
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

# shellcheck disable=SC2206 # Intentional glob expansion into an array.
sources=($source_glob)
if [[ ! -e "${sources[0]}" ]]; then
  echo "No Rust tutorial sources found" >&2
  exit 1
fi

for source in "${sources[@]}"; do
  stem="$(basename "${source%.rs}")"
  example="tutorial_${stem}"
  case "$stem" in
    09_tensor_ops_conversions)
      echo "==> cargo run --manifest-path $crate_manifest --features arrow,ndarray,csv,parquet --example $example"
      cargo run --manifest-path "$crate_manifest" --features arrow,ndarray,csv,parquet --example "$example"
      ;;
    10_ocb_roundtrip_parallel)
      echo "==> cargo run --manifest-path $crate_manifest --features format-ocb --example $example"
      cargo run --manifest-path "$crate_manifest" --features format-ocb --example "$example"
      ;;
    *)
      echo "==> cargo run --manifest-path $crate_manifest --example $example"
      cargo run --manifest-path "$crate_manifest" --example "$example"
      ;;
  esac
done

echo "Rust tutorial runner passed (${#sources[@]} examples)."
