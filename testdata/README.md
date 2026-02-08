# Test Data

This directory contains small, checked-in fixtures used by integration tests.

## greet.component.wasm

Built from `test-build/commands/greet`:

```bash
cd test-build/commands/greet
cargo build --release --target wasm32-unknown-unknown

cd ../../..
wasm-tools component new \
  test-build/commands/greet/target/wasm32-unknown-unknown/release/greet.wasm \
  -o testdata/greet.component.wasm
```

