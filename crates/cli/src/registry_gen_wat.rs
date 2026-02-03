#![allow(dead_code)]

//! Generate registry.component.wasm from discovered commands using a WAT template.
//!
//! This mirrors registry_gen.rs but emits a core module via WAT, then wraps it
//! as a component with embedded WIT metadata.

use crate::component_scan::CommandInfo;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use wasm_encoder::{CustomSection, Section};
use wit_component::ComponentEncoder;
use wit_parser::{Resolve, UnresolvedPackageGroup};

const REGISTRY_WIT_BASE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../wit/registry.wit"));
const REGISTRY_WAT_TEMPLATE: &str = include_str!("registry_template.wat");

struct NameTable {
    data: Vec<u8>,
    offsets: Vec<(u32, u32)>,
}

/// Check if we should use a pre-built registry instead of generating one.
pub fn should_use_prebuilt_registry(defaults_dir: &Path) -> bool {
    defaults_dir.join("registry.component.wasm").exists()
}

/// Get the path to the pre-built registry if it exists.
pub fn get_prebuilt_registry(defaults_dir: &Path) -> Option<PathBuf> {
    let path = defaults_dir.join("registry.component.wasm");
    if path.exists() { Some(path) } else { None }
}

/// Generate a registry component from discovered commands using WAT template.
pub fn generate_registry_wat(commands: &[CommandInfo]) -> Result<Vec<u8>> {
    let name_table = build_name_table(commands);
    let wat_source = build_wat_module(commands, &name_table)?;

    let core_module = wat::parse_str(&wat_source).context("failed to parse registry WAT")?;

    let dynamic_wit = generate_dynamic_wit(commands);
    let mut resolve = Resolve::default();
    let wit_path = Path::new("registry.wit");
    let pkg_group = UnresolvedPackageGroup::parse(wit_path, &dynamic_wit)
        .context("failed to parse dynamic WIT")?;
    let _pkg_ids = resolve.push_group(pkg_group)?;

    let world_id = resolve
        .packages
        .iter()
        .flat_map(|(_, pkg)| pkg.worlds.values())
        .find(|world_id| resolve.worlds[**world_id].name == "dynamic-registry")
        .copied()
        .context("dynamic-registry world not found in generated WIT")?;

    let encoded_meta = wit_component::metadata::encode(
        &resolve,
        world_id,
        wit_component::StringEncoding::UTF8,
        None,
    )?;

    let module_with_meta =
        add_custom_section(&core_module, "component-type:registry", &encoded_meta)?;

    let component = ComponentEncoder::default()
        .module(&module_with_meta)?
        .validate(false)
        .encode()
        .context("failed to encode component")?;

    Ok(component)
}

/// Generate WIT source dynamically based on discovered commands.
fn generate_dynamic_wit(commands: &[CommandInfo]) -> String {
    let mut wit = String::new();

    wit.push_str(REGISTRY_WIT_BASE.trim_end());
    wit.push_str("\n\n");

    for cmd in commands {
        wit.push_str(&format!("interface {}-command {{\n", cmd.name));
        wit.push_str("  use types.{command-meta, command-result};\n");
        wit.push_str("  meta: func() -> command-meta;\n");
        wit.push_str("  run: func(argv: list<string>) -> command-result;\n");
        wit.push_str("}\n\n");
    }

    wit.push_str("world dynamic-registry {\n");
    wit.push_str("  import host;\n");

    for cmd in commands {
        wit.push_str(&format!("  import {}-command;\n", cmd.name));
    }

    wit.push_str("  export registry;\n");
    wit.push_str("}\n");

    wit
}

fn build_name_table(commands: &[CommandInfo]) -> NameTable {
    let mut data = Vec::new();
    let mut offsets = Vec::with_capacity(commands.len());
    let mut offset = 0u32;

    for cmd in commands {
        let name_bytes = cmd.name.as_bytes();
        offsets.push((offset, name_bytes.len() as u32));
        data.extend_from_slice(name_bytes);
        offset += name_bytes.len() as u32;
    }

    NameTable { data, offsets }
}

fn build_wat_module(commands: &[CommandInfo], name_table: &NameTable) -> Result<String> {
    let imports = build_imports(commands);
    let list_body = build_list_commands_body(commands, &name_table.offsets);
    let run_body = build_run_body(commands, &name_table.offsets);
    let heap_start = compute_heap_start(name_table.data.len());
    let name_data = escape_bytes(&name_table.data);

    apply_template(
        REGISTRY_WAT_TEMPLATE,
        &[
            ("{{IMPORTS}}", imports),
            ("{{HEAP_START}}", heap_start.to_string()),
            ("{{LIST_COMMANDS_BODY}}", list_body),
            ("{{RUN_BODY}}", run_body),
            ("{{NAME_DATA}}", name_data),
        ],
    )
}

fn build_imports(commands: &[CommandInfo]) -> String {
    let mut imports = String::new();

    for cmd in commands {
        let ident = command_ident(&cmd.name);
        imports.push_str(&format!(
            "  (import \"wacli:cli/{name}-command@1.0.0\" \"meta\" (func ${ident}_meta (type $import_meta)))\n",
            name = cmd.name,
            ident = ident
        ));
        imports.push_str(&format!(
            "  (import \"wacli:cli/{name}-command@1.0.0\" \"run\" (func ${ident}_run (type $import_run)))\n",
            name = cmd.name,
            ident = ident
        ));
    }

    imports
}

fn build_list_commands_body(commands: &[CommandInfo], name_offsets: &[(u32, u32)]) -> String {
    const RECORD_SIZE: i32 = 60;
    const ZERO_FIELDS: [i32; 12] = [8, 12, 16, 20, 24, 28, 32, 36, 44, 48, 52, 56];

    let count = commands.len() as i32;
    let list_bytes = count * RECORD_SIZE;

    let mut body = String::new();

    push_line(&mut body, 4, "i32.const 8");
    push_line(&mut body, 4, "call $alloc");
    push_line(&mut body, 4, "local.set $result_ptr");
    push_blank(&mut body);

    push_line(&mut body, 4, &format!("i32.const {}", list_bytes));
    push_line(&mut body, 4, "call $alloc");
    push_line(&mut body, 4, "local.set $list_ptr");
    push_blank(&mut body);

    push_line(&mut body, 4, "local.get $result_ptr");
    push_line(&mut body, 4, "local.get $list_ptr");
    push_line(&mut body, 4, "i32.store offset=0 align=2");
    push_line(&mut body, 4, "local.get $result_ptr");
    push_line(&mut body, 4, &format!("i32.const {}", count));
    push_line(&mut body, 4, "i32.store offset=4 align=2");

    for (i, (name_ptr, name_len)) in name_offsets.iter().enumerate() {
        let record_offset = (i as i32) * RECORD_SIZE;
        push_blank(&mut body);
        push_line(&mut body, 4, "local.get $list_ptr");
        push_line(&mut body, 4, &format!("i32.const {}", record_offset));
        push_line(&mut body, 4, "i32.add");
        push_line(&mut body, 4, "local.set $record_ptr");

        push_line(&mut body, 4, "local.get $record_ptr");
        push_line(&mut body, 4, &format!("i32.const {}", name_ptr));
        push_line(&mut body, 4, "i32.store offset=0 align=2");
        push_line(&mut body, 4, "local.get $record_ptr");
        push_line(&mut body, 4, &format!("i32.const {}", name_len));
        push_line(&mut body, 4, "i32.store offset=4 align=2");

        for offset in ZERO_FIELDS {
            push_line(&mut body, 4, "local.get $record_ptr");
            push_line(&mut body, 4, "i32.const 0");
            push_line(
                &mut body,
                4,
                &format!("i32.store offset={} align=2", offset),
            );
        }

        push_line(&mut body, 4, "local.get $record_ptr");
        push_line(&mut body, 4, "i32.const 0");
        push_line(&mut body, 4, "i32.store8 offset=40");
    }

    push_blank(&mut body);
    push_line(&mut body, 4, "local.get $result_ptr");

    body
}

fn build_run_body(commands: &[CommandInfo], name_offsets: &[(u32, u32)]) -> String {
    let mut body = String::new();

    for (i, cmd) in commands.iter().enumerate() {
        let (name_ptr, name_len) = name_offsets
            .get(i)
            .copied()
            .unwrap_or((0, cmd.name.len() as u32));
        let ident = command_ident(&cmd.name);

        push_line(&mut body, 4, "local.get $name_ptr");
        push_line(&mut body, 4, "local.get $name_len");
        push_line(&mut body, 4, &format!("i32.const {}", name_ptr));
        push_line(&mut body, 4, &format!("i32.const {}", name_len));
        push_line(&mut body, 4, "call $match-name");
        push_line(&mut body, 4, "if");
        push_line(&mut body, 6, "i32.const 16");
        push_line(&mut body, 6, "call $alloc");
        push_line(&mut body, 6, "local.set $ret_ptr");
        push_line(&mut body, 6, "local.get $argv_ptr");
        push_line(&mut body, 6, "local.get $argv_len");
        push_line(&mut body, 6, "local.get $ret_ptr");
        push_line(&mut body, 6, &format!("call ${}_run", ident));
        push_line(&mut body, 6, "local.get $ret_ptr");
        push_line(&mut body, 6, "return");
        push_line(&mut body, 4, "end");
        push_blank(&mut body);
    }

    push_line(&mut body, 4, "i32.const 16");
    push_line(&mut body, 4, "call $alloc");
    push_line(&mut body, 4, "local.set $ret_ptr");
    push_line(&mut body, 4, "local.get $ret_ptr");
    push_line(&mut body, 4, "i32.const 1");
    push_line(&mut body, 4, "i32.store offset=0 align=2");
    push_line(&mut body, 4, "local.get $ret_ptr");
    push_line(&mut body, 4, "i32.const 0");
    push_line(&mut body, 4, "i32.store offset=4 align=2");
    push_line(&mut body, 4, "local.get $ret_ptr");
    push_line(&mut body, 4, "local.get $name_ptr");
    push_line(&mut body, 4, "i32.store offset=8 align=2");
    push_line(&mut body, 4, "local.get $ret_ptr");
    push_line(&mut body, 4, "local.get $name_len");
    push_line(&mut body, 4, "i32.store offset=12 align=2");
    push_line(&mut body, 4, "local.get $ret_ptr");

    body
}

fn command_ident(name: &str) -> String {
    let mut ident = String::from("cmd_");
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            ident.push(ch);
        } else {
            ident.push('_');
        }
    }
    ident
}

fn compute_heap_start(data_len: usize) -> u32 {
    let aligned = align_up(data_len as u32, 4);
    aligned + 1024
}

fn align_up(value: u32, align: u32) -> u32 {
    if align == 0 {
        return value;
    }
    let rem = value % align;
    if rem == 0 {
        value
    } else {
        value + (align - rem)
    }
}

fn escape_bytes(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        out.push_str(&format!("\\{:02x}", b));
    }
    out
}

fn apply_template(template: &str, replacements: &[(&str, String)]) -> Result<String> {
    let mut out = template.to_string();
    for (placeholder, value) in replacements {
        if !out.contains(placeholder) {
            bail!("placeholder not found in WAT template: {}", placeholder);
        }
        out = out.replace(placeholder, value);
    }
    Ok(out)
}

fn push_line(out: &mut String, indent: usize, line: &str) {
    for _ in 0..indent {
        out.push(' ');
    }
    out.push_str(line);
    out.push('\n');
}

fn push_blank(out: &mut String) {
    out.push('\n');
}

fn add_custom_section(module_bytes: &[u8], name: &str, data: &[u8]) -> Result<Vec<u8>> {
    let custom = CustomSection {
        name: std::borrow::Cow::Borrowed(name),
        data: std::borrow::Cow::Borrowed(data),
    };

    let mut result = module_bytes.to_vec();
    custom.append_to(&mut result);
    Ok(result)
}
