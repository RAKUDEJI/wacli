# wacli

A CLI tool for composing WebAssembly Components using the [WAC](https://github.com/bytecodealliance/wac) composition language.

## Features

- **Single binary** - No external dependencies (wac CLI, wasm-tools, jq not required)
- **Manifest-driven builds** - Define your CLI composition in `wacli.toml`
- **Direct WAC composition** - Use `.wac` files directly
- **Import validation** - Check component imports against an allowlist

## Installation

### From Releases

Download the latest binary for your platform from [Releases](../../releases).

### From Source

```bash
cargo install --path .
```

## Usage

### Initialize a new project

```bash
wacli init my-cli --name "example:my-cli"
```

### Build from manifest

```bash
wacli build -m wacli.toml
```

### Compose using WAC file

```bash
wacli compose app.wac -o app.wasm \
  -d "pkg:name=path/to/component.wasm"
```

### Check imports

```bash
wacli check component.wasm --allowlist allowed-imports.txt
```

## Manifest Format (wacli.toml)

```toml
[package]
name = "example:my-cli"
version = "0.1.0"

[framework]
host = "path/to/host.component.wasm"
core = "path/to/core.component.wasm"
registry = "registry/registry.component.wasm"

[[command]]
name = "greet"
package = "example:greeter"
plugin = "plugins/greeter/greeter.component.wasm"

[output]
path = "dist/my-cli.component.wasm"
```

## License

MIT
