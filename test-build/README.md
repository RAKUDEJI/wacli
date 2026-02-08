Playground for trying wacli as a user.

Expected layout:
  wacli.json
  defaults/host.component.wasm
  defaults/core.component.wasm
  defaults/registry.component.wasm (optional; used only with --use-prebuilt-registry)
  commands/*.component.wasm

Build artifacts:
  .wacli/registry.component.wasm (auto-generated)

Example:
  cd test-build
  wacli build

Run (note: pass-through args use `--`):
  wacli run my-cli.component.wasm -- --help
  wacli run my-cli.component.wasm -- --version
