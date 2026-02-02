Playground for trying wacli as a user.

Expected layout:
  defaults/host.component.wasm
  defaults/core.component.wasm
  defaults/registry.component.wasm (optional; generated if missing)
  commands/*.component.wasm

Example:
  cd test-build
  wacli build --name "example:my-cli"
