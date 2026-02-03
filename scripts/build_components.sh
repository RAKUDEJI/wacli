#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

build_component() {
  local name="$1"
  local comp_dir="${root_dir}/components/${name}"
  local gen_dir="${comp_dir}/gen"
  local wasm_in="${gen_dir}/_build/wasm/release/build/gen/gen.wasm"
  local wasm_out="${comp_dir}/${name}.wasm"
  local component_out="${comp_dir}/${name}.component.wasm"
  local root_component_out="${root_dir}/components/${name}.component.wasm"

  if [[ -d "${gen_dir}/gen/interface" ]]; then
    mkdir -p "${gen_dir}/interface"
    for entry in "${gen_dir}/gen/interface"/*; do
      [[ -e "${entry}" ]] || continue
      local base
      base="$(basename "${entry}")"
      if [[ ! -e "${gen_dir}/interface/${base}" ]]; then
        cp -R "${entry}" "${gen_dir}/interface/${base}"
      fi
    done
  fi

  if ! (cd "${gen_dir}" && moon build --target wasm); then
    if [[ -f "${component_out}" ]]; then
      echo "MoonBit build failed for ${name}; using prebuilt ${component_out}" >&2
      return 0
    fi
    echo "MoonBit build failed for ${name} and no prebuilt component found" >&2
    exit 1
  fi

  if [[ ! -f "${wasm_in}" ]]; then
    echo "Missing MoonBit output: ${wasm_in}" >&2
    exit 1
  fi

  wasm-tools component embed "${comp_dir}/wit" "${wasm_in}" -o "${wasm_out}" --encoding utf16
  wasm-tools component new "${wasm_out}" -o "${component_out}"
  cp "${component_out}" "${root_component_out}"
}

build_component host
build_component core
