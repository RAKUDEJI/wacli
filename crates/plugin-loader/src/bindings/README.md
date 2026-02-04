# Generated bindings

These files are generated from the repo-level `wit/cli` package.

Worlds:
- `pipe-plugin`
- `pipe-runtime-host`

Regenerate:
```
scripts/gen_plugin_loader_bindings.sh
```

Notes:
- The generator uses `WASMTIME_DEBUG_BINDGEN=1` and the `regen-bindings` feature to capture
  `wasmtime::component::bindgen!` output.
- Do not edit the `*.rs` files by hand.
