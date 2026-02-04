#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

build_component() {
  local name="$1"
  local package="$2"
  local crate="$3"
  local world="$4"
  local comp_dir="${root_dir}/components/${name}"
  local wasm_in="${root_dir}/target/wasm32-unknown-unknown/release/${crate}.wasm"
  local wasm_out="${comp_dir}/${name}.wasm"
  local component_out="${comp_dir}/${name}.component.wasm"
  local root_component_out="${root_dir}/components/${name}.component.wasm"

  cargo build -p "${package}" --target wasm32-unknown-unknown --release

  if [[ ! -f "${wasm_in}" ]]; then
    echo "Missing Rust output: ${wasm_in}" >&2
    exit 1
  fi

  wasm-tools component embed "${root_dir}/wit/cli" "${wasm_in}" -o "${wasm_out}" --encoding utf8 --world "${world}"
  wasm-tools component new "${wasm_out}" -o "${component_out}"
  cp "${component_out}" "${root_component_out}"
}

build_component host wacli-host wacli_host host-provider
build_component core wacli-core wacli_core core
