# wacli-cdk

[![Crates.io](https://img.shields.io/crates/v/wacli-cdk.svg)](https://crates.io/crates/wacli-cdk)
[![Documentation](https://docs.rs/wacli-cdk/badge.svg)](https://docs.rs/wacli-cdk)
[![License](https://img.shields.io/crates/l/wacli-cdk.svg)](LICENSE)

Command Development Kit for building [wacli](https://github.com/RAKUDEJI/wacli) plugins in Rust.

## Overview

`wacli-cdk` provides everything you need to create WebAssembly Component Model plugins for the wacli CLI framework:

- **`Command` trait** - Define your command's metadata and execution logic
- **`export!` macro** - Generate required WIT exports automatically
- **`meta()` builder** - Fluent API for command metadata
- **`Context`** - Access arguments and environment variables
- **`args` module** - Lightweight argument parsing helpers
- **`io` module** - stdout/stderr utilities
- **`fs` module** - File read/write/list helpers via the host
- **`pipes` module** - Dynamic pipe loading for data transformation

## Installation

Add to your `Cargo.toml`:

```toml
[package]
name = "my-command"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
wacli-cdk = "0.0.33"
```

## Quick Start

```rust
use wacli_cdk::{Command, CommandMeta, CommandResult, meta};

struct Hello;

impl Command for Hello {
    fn meta() -> CommandMeta {
        meta("hello")
            .summary("Say hello to someone")
            .usage("hello [OPTIONS] [NAME]")
            .description("A friendly greeting command")
            .example("hello World")
            .example("hello --loud Alice")
            .build()
    }

    fn run(argv: Vec<String>) -> CommandResult {
        let name = argv.first().map(|s| s.as_str()).unwrap_or("World");
        wacli_cdk::io::println(format!("Hello, {name}!"));
        Ok(0)
    }
}

wacli_cdk::export!(Hello);
```

## Building

### Step 1: Build the WebAssembly module

```bash
cargo build --target wasm32-unknown-unknown --release
```

This produces a core WebAssembly module at `target/wasm32-unknown-unknown/release/my_command.wasm`.

### Step 2: Convert to a WebAssembly Component

The core module must be converted to a Component using `wasm-tools`:

```bash
wasm-tools component new \
    target/wasm32-unknown-unknown/release/my_command.wasm \
    -o my-command.component.wasm
```

**Important:** Without this step, wacli will reject the module with an error like:
```
Error: found core module (version 0x1), expected component (version 0xd)
Hint: run `wasm-tools component new your.wasm -o your.component.wasm`
```

## API Reference

### Context

Wrap `argv` with `Context` to access environment variables and convenient argument helpers:

`argv` contains only arguments (the command name is not included).
Example: `my-cli greet Alice` -> `argv = ["Alice"]`.

**Note:** If you need positional arguments after boolean flags, use `--` to end flag parsing.

```rust
fn run(argv: Vec<String>) -> CommandResult {
    let ctx = wacli_cdk::Context::new(argv);

    // Positional arguments (flags and their values are skipped)
    let name = ctx.arg(0).unwrap_or("default");
    let rest = ctx.positional_args();

    // Boolean flags
    if ctx.flag(["-v", "--verbose"]) {
        wacli_cdk::io::eprintln("Verbose mode enabled");
    }

    // Required arguments
    let file = ctx.require_arg(0, "FILE")?;

    // Flag values (--key=value or --key value)
    if let Some(output) = ctx.value("--output") {
        wacli_cdk::io::println(format!("Output: {output}"));
    }

    // Environment variables
    for (key, val) in &ctx.env {
        wacli_cdk::io::println(format!("{key}={val}"));
    }

    Ok(0)
}
```

### Argument Helpers

Use `args` module functions directly for more control:

```rust
use wacli_cdk::args;

fn run(argv: Vec<String>) -> CommandResult {
    // Check for flags
    if args::flag(&argv, "--help") {
        print_help();
        return Ok(0);
    }

    // Get flag values
    let count = args::value(&argv, "--count")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    // Get positional argument (flags and their values are skipped; `--` ends flag parsing)
    let target = args::positional(&argv, 0).unwrap_or("default");

    // Get all positional arguments
    let args_only = args::positional_args(&argv);

    // Tip: use `--` to pass positional args that start with `-`

    // Get remaining arguments
    let files = args::rest(&argv, 1);

    Ok(0)
}
```

### I/O Utilities

```rust
use wacli_cdk::io;

// stdout
io::print("no newline");
io::println("with newline");
io::flush();

// stderr
io::eprint("error: ");
io::eprintln("something went wrong");
```

### File System Helpers

#### Reading files

```rust
use wacli_cdk::{fs, CommandError};

// Read entire file as bytes
let bytes = fs::read("config.json")?;

// Convert to string
let text = String::from_utf8(bytes)
    .map_err(|e| CommandError::Failed(e.to_string()))?;
```

#### Writing files

```rust
use wacli_cdk::fs;

// Write string data
fs::write("output.txt", "Hello, World!")?;

// Write binary data
fs::write("data.bin", &[0x00, 0x01, 0x02])?;

// Copy a file
let contents = fs::read("source.txt")?;
fs::write("dest.txt", &contents)?;
```

#### Listing directories

```rust
use wacli_cdk::fs;

let entries = fs::list_dir(".")?;
for name in entries {
    wacli_cdk::io::println(&name);
}
```

**Note:** File paths are relative to the preopened directories provided at runtime.
See [Running with File Access](#running-with-file-access) for details.

### Pipe Helpers

Pipes are dynamically loaded data transformation plugins. Use `pipes` module to load and invoke them at runtime:

```rust
use wacli_cdk::pipes;

fn run(argv: Vec<String>) -> CommandResult {
    // List available pipes
    let available = pipes::list();
    for info in &available {
        wacli_cdk::io::println(format!("{}: {}", info.name, info.summary));
    }

    // Load a pipe by name
    let formatter = pipes::load("format/json")?;

    // Get pipe metadata
    let meta = formatter.meta();
    wacli_cdk::io::println(format!("Loaded: {} v{}", meta.name, meta.version));

    // Process data through the pipe
    let input = b"hello world";
    let output = formatter.process(input, &["--pretty".to_string()])?;

    wacli_cdk::io::print(String::from_utf8_lossy(&output));
    Ok(0)
}
```

**Pipe directory structure:**
```
plugins/
  <command>/
    format/
      json.component.wasm    # pipes::load("format/json")
      table.component.wasm   # pipes::load("format/table")
```

**Note:** Pipes are only available when running with `wacli run`. The host dynamically loads pipe components from `./plugins/<command>/` relative to the current working directory.

### Building a Pipe Plugin (pipe-plugin)

Pipes are separate components that implement the `pipe-plugin` world.

**Cargo.toml**
```toml
[package]
name = "format-table"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.52"
```

**src/lib.rs**
```rust
wit_bindgen::generate!({
    // Point this to the `wit/` directory created by `wacli init`
    path: "../my-cli/wit",
    world: "pipe-plugin",
});

use exports::wacli::cli::pipe::Guest;
use wacli::cli::types::{PipeError, PipeMeta};

struct TablePipe;

impl Guest for TablePipe {
    fn meta() -> PipeMeta {
        PipeMeta {
            name: "format/table".to_string(),
            summary: "Uppercase formatter".to_string(),
            input_types: vec!["text/plain".to_string()],
            output_type: "text/plain".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    fn process(input: Vec<u8>, _options: Vec<String>) -> Result<Vec<u8>, PipeError> {
        let s = String::from_utf8(input).map_err(|e| PipeError::ParseError(e.to_string()))?;
        Ok(s.to_uppercase().into_bytes())
    }
}

export!(TablePipe);
```

**Build & install**
```bash
cargo build --target wasm32-unknown-unknown --release
wasm-tools component new \
  target/wasm32-unknown-unknown/release/format_table.wasm \
  -o table.component.wasm

# Place under the command's plugins directory
mkdir -p plugins/show/format
cp table.component.wasm plugins/show/format/table.component.wasm
```

Now the command can load it:
```rust
let pipe = pipes::load("format/table")?;
```

**Notes**
- `pipes::load("format/table")` maps to `plugins/<command>/format/table.component.wasm`.
- `meta().name` is for display; keep it consistent with the path to avoid confusion.
- Pipes are loaded only by `wacli run`.

### Metadata Builder

```rust
meta("command-name")
    .summary("One-line description")
    .usage("cmd [OPTIONS] <ARGS>")             // usage pattern
    .description("Detailed description...")    // shown in help
    .version("1.0.0")                          // command version
    .alias("cmd")                              // command aliases
    .alias("c")
    .example("cmd --flag value")               // usage examples
    .example("cmd input.txt")
    .hidden()                                  // hide from command list
    .build()
```

### Error Handling

Return errors using `CommandError`:

```rust
use wacli_cdk::{CommandResult, CommandError};

fn run(argv: Vec<String>) -> CommandResult {
    let path = argv.first()
        .ok_or_else(|| CommandError::InvalidArgs("missing file path".into()))?;

    // ... do work ...

    if something_failed {
        return Err(CommandError::Failed("operation failed".into()));
    }

    Ok(0)
}
```

### WASI Capabilities

Plugins do not import WASI directly. All host interactions should go through the
`wacli:cli/host-*` interfaces (`host-env`, `host-io`, `host-fs`, `host-process`, `host-pipes`).

### Prelude

Import common types with a single statement:

```rust
use wacli_cdk::prelude::*;
// Imports: Command, CommandMeta, CommandResult, CommandError, Context, meta, args, io, fs
```

## Integration with wacli

After building your component, integrate it with a wacli project:

```bash
# Initialize a new CLI project (downloads host/core components)
wacli init my-cli --with-components

# Copy your component to the commands directory
cp my-command.component.wasm my-cli/commands/

# Build the final CLI
cd my-cli && wacli build

# Run your command (native host; required for pipes)
wacli run my-cli.component.wasm my-command --help
```

**Note:** Direct `wasmtime run` will fail because the composed CLI imports
`wacli:cli/pipe-runtime@1.0.0`, which is provided by `wacli run`.

**Tip:** The file name (without `.component.wasm`) becomes the command name.
Keep it in sync with `meta("...")` to avoid confusion.

## Running with File Access

If your command uses `fs::read`, `fs::write`, or `fs::list_dir`, you must run from a directory you want to access. `wacli run` preopens the current working directory, and you can add more with `--dir`:

```bash
# Run from the directory you want to access
cd /path/to/data
wacli run my-cli.component.wasm my-command input.txt

# Preopen another directory
wacli run --dir /path/to/data my-cli.component.wasm my-command input.txt

# Map a host dir to a guest path
wacli run --dir /path/to/data::/data my-cli.component.wasm my-command /data/input.txt
```

File paths are resolved relative to the preopened directories.

**Tip:** Use `--` if your command also defines a `--dir` flag.

## Requirements

- Rust 1.92+ (edition 2024)
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [wasm-tools](https://github.com/bytecodealliance/wasm-tools) for componentization

## License

Apache-2.0
