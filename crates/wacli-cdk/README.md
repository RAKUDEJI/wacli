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
- **`wasi` module** - WASI 0.2.9 filesystem and random access

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
wacli-cdk = "0.0.20"
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

```bash
# Build the WebAssembly module
cargo build --target wasm32-unknown-unknown --release

# Convert to a WebAssembly Component
wasm-tools component new \
    target/wasm32-unknown-unknown/release/my_command.wasm \
    -o my-command.component.wasm
```

## API Reference

### Context

Wrap `argv` with `Context` to access environment variables and convenient argument helpers:

`argv` contains only arguments (the command name is not included).
Example: `my-cli greet Alice` -> `argv = ["Alice"]`.

```rust
fn run(argv: Vec<String>) -> CommandResult {
    let ctx = wacli_cdk::Context::new(argv);

    // Positional arguments
    let name = ctx.arg(0).unwrap_or("default");

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

    // Get positional argument (flags are skipped; `--` ends flag parsing)
    let target = args::positional(&argv, 0).unwrap_or("default");

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

### Metadata Builder

```rust
meta("command-name")
    .summary("One-line description")           // shown in command list
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

Plugins have access to WASI 0.2.9 filesystem and random APIs:

```rust
use wacli_cdk::wasi;

fn run(argv: Vec<String>) -> CommandResult {
    // Filesystem access via preopens
    use wasi::filesystem::preopens::get_directories;
    let dirs = get_directories();

    // Random number generation
    use wasi::random::random::get_random_bytes;
    let bytes = get_random_bytes(16);

    Ok(0)
}
```

Available WASI interfaces:
- `wasi::filesystem::{types, preopens}` - File and directory operations
- `wasi::random::{random, insecure, insecure_seed}` - Random number generation

### Prelude

Import common types with a single statement:

```rust
use wacli_cdk::prelude::*;
// Imports: Command, CommandMeta, CommandResult, CommandError, Context, meta, args, io, wasi
```

## Integration with wacli

After building your component, integrate it with a wacli project:

```bash
# Initialize a new CLI project
wacli init my-cli

# Copy your component to the commands directory
cp my-command.component.wasm my-cli/commands/

# Build the final CLI
cd my-cli && wacli build

# Run your command
wasmtime run my-cli.component.wasm my-command --help
```

## Requirements

- Rust 1.92+ (edition 2024)
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [wasm-tools](https://github.com/bytecodealliance/wasm-tools) for componentization

## License

Apache-2.0
