#![allow(dead_code)]

//! Generate registry.component.wasm from discovered commands using a WAT template.
//!
//! This mirrors registry_gen.rs but emits a core module via WAT, then wraps it
//! as a component with embedded WIT metadata.

use crate::component_scan::CommandInfo;
use crate::wit;
use anyhow::{Context, Result, bail};
use semver::Version;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use wasm_encoder::{CustomSection, Section};
use wit_component::ComponentEncoder;
use wit_parser::{PackageName, Resolve, UnresolvedPackageGroup};

const REGISTRY_WIT_BASE: &str = wit::REGISTRY_WIT;
const REGISTRY_WAT_TEMPLATE: &str = include_str!("registry_template.wat");

#[derive(Debug, Clone, Default)]
pub struct AppMeta {
    pub name: String,
    pub version: String,
    pub description: String,
}

#[derive(Debug)]
struct StringTable {
    data: Vec<u8>,
    offsets: HashMap<String, (u32, u32)>,
}

impl Default for StringTable {
    fn default() -> Self {
        // Reserve offset 0 as a sentinel for "no string" (ptr=0,len=0).
        // Some canonical ABI adapters treat ptr=0 as "null" even when len>0,
        // so we must ensure no non-empty string is ever stored at offset 0.
        Self {
            data: vec![0],
            offsets: HashMap::new(),
        }
    }
}

impl StringTable {
    fn intern(&mut self, s: &str) -> (u32, u32) {
        if s.is_empty() {
            return (0, 0);
        }
        if let Some(v) = self.offsets.get(s) {
            return *v;
        }
        let offset = self.data.len() as u32;
        let bytes = s.as_bytes();
        let len = bytes.len() as u32;
        self.data.extend_from_slice(bytes);
        self.offsets.insert(s.to_string(), (offset, len));
        (offset, len)
    }

    fn get(&self, s: &str) -> (u32, u32) {
        if s.is_empty() {
            return (0, 0);
        }
        self.offsets.get(s).copied().unwrap_or((0, 0))
    }
}

/// Get the path to a pre-built registry in `defaults/` if it exists.
pub fn get_prebuilt_registry(defaults_dir: &Path) -> Option<PathBuf> {
    let path = defaults_dir.join("registry.component.wasm");
    if path.exists() { Some(path) } else { None }
}

/// Generate a registry component from discovered commands using WAT template.
pub fn generate_registry_wat(commands: &[CommandInfo], app: &AppMeta) -> Result<Vec<u8>> {
    let string_table = build_string_table(commands, app);
    let wat_source = build_wat_module(commands, app, &string_table)?;

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
    append_wit_base(&mut wit);

    for cmd in commands {
        wit.push_str(&format!("interface {}-command {{\n", cmd.name));
        wit.push_str("  use types.{command-meta, command-result};\n");
        wit.push_str("  meta: func() -> command-meta;\n");
        wit.push_str("  run: func(argv: list<string>) -> command-result;\n");
        wit.push_str("}\n\n");
    }

    wit.push_str("world dynamic-registry {\n");

    for cmd in commands {
        wit.push_str(&format!("  import {}-command;\n", cmd.name));
    }

    wit.push_str("  export registry;\n");
    wit.push_str("  export registry-schema;\n");
    wit.push_str("}\n");

    wit
}

fn append_wit_base(dst: &mut String) {
    dst.push_str(wit::TYPES_WIT.trim_end());
    dst.push_str("\n\n");
    append_without_package(dst, wit::HOST_ENV_WIT);
    dst.push_str("\n\n");
    append_without_package(dst, wit::HOST_IO_WIT);
    dst.push_str("\n\n");
    append_without_package(dst, wit::HOST_FS_WIT);
    dst.push_str("\n\n");
    append_without_package(dst, wit::HOST_PROCESS_WIT);
    dst.push_str("\n\n");
    append_without_package(dst, wit::SCHEMA_WIT);
    dst.push_str("\n\n");
    append_without_package(dst, wit::REGISTRY_SCHEMA_WIT);
    dst.push_str("\n\n");
    append_without_package(dst, REGISTRY_WIT_BASE);
    dst.push_str("\n\n");
}

fn append_without_package(dst: &mut String, wit: &str) {
    let lines = wit.lines();
    let mut saw_package = false;
    let mut started = false;

    for line in lines {
        if !started {
            if !saw_package && line.trim_start().starts_with("package ") {
                saw_package = true;
                continue;
            }
            if saw_package && line.trim().is_empty() {
                continue;
            }
            started = true;
        }
        dst.push_str(line);
        dst.push('\n');
    }
}

fn build_string_table(commands: &[CommandInfo], app: &AppMeta) -> StringTable {
    let mut t = StringTable::default();

    // App metadata (used by core for global help/version).
    t.intern(&app.name);
    t.intern(&app.version);
    t.intern(&app.description);

    for cmd in commands {
        let meta = &cmd.metadata.command_meta;
        t.intern(&meta.name);
        t.intern(&meta.summary);
        t.intern(&meta.usage);
        t.intern(&meta.version);
        t.intern(&meta.description);

        for a in &meta.aliases {
            t.intern(a);
        }
        for e in &meta.examples {
            t.intern(e);
        }
        for arg in &meta.args {
            t.intern(&arg.name);
            if let Some(s) = arg.short.as_deref() {
                t.intern(s);
            }
            if let Some(s) = arg.long.as_deref() {
                t.intern(s);
            }
            t.intern(&arg.help);
            if let Some(s) = arg.default_value.as_deref() {
                t.intern(s);
            }
            if let Some(s) = arg.value_name.as_deref() {
                t.intern(s);
            }
        }

        if let Some(schema) = cmd.metadata.command_schema.as_ref() {
            // The schema may include additional strings beyond `command_meta`.
            for a in &schema.aliases {
                t.intern(a);
            }
            for e in &schema.examples {
                t.intern(e);
            }
            for arg in &schema.args {
                t.intern(&arg.name);
                if let Some(s) = arg.short.as_deref() {
                    t.intern(s);
                }
                if let Some(s) = arg.long.as_deref() {
                    t.intern(s);
                }
                t.intern(&arg.help);
                if let Some(s) = arg.default_value.as_deref() {
                    t.intern(s);
                }
                if let Some(s) = arg.env.as_deref() {
                    t.intern(s);
                }
                if let Some(s) = arg.value_name.as_deref() {
                    t.intern(s);
                }
                if let Some(s) = arg.value_type.as_deref() {
                    t.intern(s);
                }
                for v in &arg.possible_values {
                    t.intern(v);
                }
                for v in &arg.conflicts_with {
                    t.intern(v);
                }
                for v in &arg.requires {
                    t.intern(v);
                }
            }
        }
    }

    t
}

fn build_wat_module(commands: &[CommandInfo], app: &AppMeta, strings: &StringTable) -> Result<String> {
    let imports = build_imports(commands)?;
    let list_body = build_list_commands_body(commands, strings);
    let list_schemas_body = build_list_schemas_body(commands, strings);
    let app_meta_body = build_app_meta_body(app, strings);
    let run_body = build_run_body(commands, strings);
    let heap_start = compute_heap_start(strings.data.len());
    let string_data = escape_bytes(&strings.data);

    apply_template(
        REGISTRY_WAT_TEMPLATE,
        &[
            ("{{IMPORTS}}", imports),
            ("{{HEAP_START}}", heap_start.to_string()),
            ("{{LIST_COMMANDS_BODY}}", list_body),
            ("{{LIST_SCHEMAS_BODY}}", list_schemas_body),
            ("{{APP_META_BODY}}", app_meta_body),
            ("{{RUN_BODY}}", run_body),
            ("{{STRING_DATA}}", string_data),
        ],
    )
}

fn build_imports(commands: &[CommandInfo]) -> Result<String> {
    let mut imports = String::new();
    let pkg = registry_package_name()?;

    for cmd in commands {
        let ident = command_ident(&cmd.name);
        let iface = pkg.interface_id(&format!("{}-command", cmd.name));
        imports.push_str(&format!(
            "  (import \"{iface}\" \"meta\" (func ${ident}_meta (type $import_meta)))\n",
            iface = iface,
            ident = ident
        ));
        imports.push_str(&format!(
            "  (import \"{iface}\" \"run\" (func ${ident}_run (type $import_run)))\n",
            iface = iface,
            ident = ident
        ));
    }

    Ok(imports)
}

fn registry_package_name() -> Result<PackageName> {
    let decl = REGISTRY_WIT_BASE
        .lines()
        .find(|line| line.trim_start().starts_with("package "))
        .context("registry WIT missing package declaration")?;
    let mut name = decl.trim();
    name = name.strip_prefix("package ").unwrap_or(name).trim();
    name = name.strip_suffix(';').unwrap_or(name).trim();
    parse_package_name(name).context("failed to parse registry package name")
}

fn parse_package_name(name: &str) -> Result<PackageName> {
    let (namespace, rest) = name
        .split_once(':')
        .context("registry package name missing namespace")?;
    let (pkg_name, version) = match rest.split_once('@') {
        Some((pkg, version)) => (pkg, Some(version)),
        None => (rest, None),
    };
    if namespace.is_empty() {
        bail!("registry package namespace is empty");
    }
    if pkg_name.is_empty() {
        bail!("registry package name is empty");
    }
    let version = match version {
        Some(raw) if !raw.trim().is_empty() => {
            Some(Version::parse(raw.trim()).context("invalid registry package version")?)
        }
        Some(_) => bail!("registry package version is empty"),
        None => None,
    };
    Ok(PackageName {
        namespace: namespace.trim().to_string(),
        name: pkg_name.trim().to_string(),
        version,
    })
}

fn build_list_commands_body(commands: &[CommandInfo], strings: &StringTable) -> String {
    const CMD_RECORD_SIZE: i32 = 68;
    const STR_ELEM_SIZE: i32 = 8;
    const ARG_RECORD_SIZE: i32 = 72;

    let count = commands.len() as i32;
    let list_bytes = count * CMD_RECORD_SIZE;

    let mut body = String::new();

    // Allocate the returned `(ptr, len)` pair.
    push_line(&mut body, 4, "i32.const 8");
    push_line(&mut body, 4, "call $alloc");
    push_line(&mut body, 4, "local.set $result_ptr");
    push_blank(&mut body);

    // Allocate list storage.
    push_line(&mut body, 4, &format!("i32.const {}", list_bytes));
    push_line(&mut body, 4, "call $alloc");
    push_line(&mut body, 4, "local.set $list_ptr");
    push_blank(&mut body);

    // Store list ptr/len in result.
    push_line(&mut body, 4, "local.get $result_ptr");
    push_line(&mut body, 4, "local.get $list_ptr");
    push_line(&mut body, 4, "i32.store offset=0 align=2");
    push_line(&mut body, 4, "local.get $result_ptr");
    push_line(&mut body, 4, &format!("i32.const {}", count));
    push_line(&mut body, 4, "i32.store offset=4 align=2");

    for (i, cmd) in commands.iter().enumerate() {
        let meta = &cmd.metadata.command_meta;
        let record_offset = (i as i32) * CMD_RECORD_SIZE;

        let (name_ptr, name_len) = strings.get(&meta.name);
        let (summary_ptr, summary_len) = strings.get(&meta.summary);
        let (usage_ptr, usage_len) = strings.get(&meta.usage);
        let (version_ptr, version_len) = strings.get(&meta.version);
        let (desc_ptr, desc_len) = strings.get(&meta.description);

        push_blank(&mut body);
        push_line(&mut body, 4, "local.get $list_ptr");
        push_line(&mut body, 4, &format!("i32.const {}", record_offset));
        push_line(&mut body, 4, "i32.add");
        push_line(&mut body, 4, "local.set $record_ptr");

        // name
        emit_store_i32_const(&mut body, "$record_ptr", 0, name_ptr);
        emit_store_i32_const(&mut body, "$record_ptr", 4, name_len);

        // summary
        emit_store_i32_const(&mut body, "$record_ptr", 8, summary_ptr);
        emit_store_i32_const(&mut body, "$record_ptr", 12, summary_len);

        // usage
        emit_store_i32_const(&mut body, "$record_ptr", 16, usage_ptr);
        emit_store_i32_const(&mut body, "$record_ptr", 20, usage_len);

        // aliases list<string>
        if meta.aliases.is_empty() {
            emit_store_i32_const(&mut body, "$record_ptr", 24, 0);
            emit_store_i32_const(&mut body, "$record_ptr", 28, 0);
        } else {
            let bytes = (meta.aliases.len() as i32) * STR_ELEM_SIZE;
            push_line(&mut body, 4, &format!("i32.const {}", bytes));
            push_line(&mut body, 4, "call $alloc");
            push_line(&mut body, 4, "local.set $aliases_ptr");

            emit_store_i32_local(&mut body, "$record_ptr", 24, "$aliases_ptr");
            emit_store_i32_const(&mut body, "$record_ptr", 28, meta.aliases.len() as u32);

            for (j, alias) in meta.aliases.iter().enumerate() {
                let (ap, al) = strings.get(alias);
                let entry_off = (j as i32) * STR_ELEM_SIZE;
                // ptr
                push_line(&mut body, 4, "local.get $aliases_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", entry_off));
                push_line(&mut body, 4, "i32.add");
                push_line(&mut body, 4, &format!("i32.const {}", ap));
                push_line(&mut body, 4, "i32.store offset=0 align=2");
                // len
                push_line(&mut body, 4, "local.get $aliases_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", entry_off));
                push_line(&mut body, 4, "i32.add");
                push_line(&mut body, 4, &format!("i32.const {}", al));
                push_line(&mut body, 4, "i32.store offset=4 align=2");
            }
        }

        // version
        emit_store_i32_const(&mut body, "$record_ptr", 32, version_ptr);
        emit_store_i32_const(&mut body, "$record_ptr", 36, version_len);

        // hidden (bool)
        push_line(&mut body, 4, "local.get $record_ptr");
        push_line(&mut body, 4, &format!("i32.const {}", if meta.hidden { 1 } else { 0 }));
        push_line(&mut body, 4, "i32.store8 offset=40");

        // description
        emit_store_i32_const(&mut body, "$record_ptr", 44, desc_ptr);
        emit_store_i32_const(&mut body, "$record_ptr", 48, desc_len);

        // examples list<string>
        if meta.examples.is_empty() {
            emit_store_i32_const(&mut body, "$record_ptr", 52, 0);
            emit_store_i32_const(&mut body, "$record_ptr", 56, 0);
        } else {
            let bytes = (meta.examples.len() as i32) * STR_ELEM_SIZE;
            push_line(&mut body, 4, &format!("i32.const {}", bytes));
            push_line(&mut body, 4, "call $alloc");
            push_line(&mut body, 4, "local.set $examples_ptr");

            emit_store_i32_local(&mut body, "$record_ptr", 52, "$examples_ptr");
            emit_store_i32_const(&mut body, "$record_ptr", 56, meta.examples.len() as u32);

            for (j, ex) in meta.examples.iter().enumerate() {
                let (ep, el) = strings.get(ex);
                let entry_off = (j as i32) * STR_ELEM_SIZE;
                // ptr
                push_line(&mut body, 4, "local.get $examples_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", entry_off));
                push_line(&mut body, 4, "i32.add");
                push_line(&mut body, 4, &format!("i32.const {}", ep));
                push_line(&mut body, 4, "i32.store offset=0 align=2");
                // len
                push_line(&mut body, 4, "local.get $examples_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", entry_off));
                push_line(&mut body, 4, "i32.add");
                push_line(&mut body, 4, &format!("i32.const {}", el));
                push_line(&mut body, 4, "i32.store offset=4 align=2");
            }
        }

        // args list<arg-def>
        if meta.args.is_empty() {
            emit_store_i32_const(&mut body, "$record_ptr", 60, 0);
            emit_store_i32_const(&mut body, "$record_ptr", 64, 0);
        } else {
            let bytes = (meta.args.len() as i32) * ARG_RECORD_SIZE;
            push_line(&mut body, 4, &format!("i32.const {}", bytes));
            push_line(&mut body, 4, "call $alloc");
            push_line(&mut body, 4, "local.set $args_ptr");

            emit_store_i32_local(&mut body, "$record_ptr", 60, "$args_ptr");
            emit_store_i32_const(&mut body, "$record_ptr", 64, meta.args.len() as u32);

            for (j, arg) in meta.args.iter().enumerate() {
                let arg_off = (j as i32) * ARG_RECORD_SIZE;
                push_blank(&mut body);
                push_line(&mut body, 4, "local.get $args_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", arg_off));
                push_line(&mut body, 4, "i32.add");
                push_line(&mut body, 4, "local.set $arg_ptr");

                let (anp, anl) = strings.get(&arg.name);
                emit_store_i32_const(&mut body, "$arg_ptr", 0, anp);
                emit_store_i32_const(&mut body, "$arg_ptr", 4, anl);

                emit_store_opt_str(&mut body, "$arg_ptr", 8, 12, 16, arg.short.as_deref(), strings);
                emit_store_opt_str(&mut body, "$arg_ptr", 20, 24, 28, arg.long.as_deref(), strings);

                let (hp, hl) = strings.get(&arg.help);
                emit_store_i32_const(&mut body, "$arg_ptr", 32, hp);
                emit_store_i32_const(&mut body, "$arg_ptr", 36, hl);

                // required bool @40
                push_line(&mut body, 4, "local.get $arg_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", if arg.required { 1 } else { 0 }));
                push_line(&mut body, 4, "i32.store8 offset=40");

                emit_store_opt_str(
                    &mut body,
                    "$arg_ptr",
                    44,
                    48,
                    52,
                    arg.default_value.as_deref(),
                    strings,
                );
                emit_store_opt_str(
                    &mut body,
                    "$arg_ptr",
                    56,
                    60,
                    64,
                    arg.value_name.as_deref(),
                    strings,
                );

                // takes_value bool @68
                push_line(&mut body, 4, "local.get $arg_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", if arg.takes_value { 1 } else { 0 }));
                push_line(&mut body, 4, "i32.store8 offset=68");
            }
        }
    }

    push_blank(&mut body);
    push_line(&mut body, 4, "local.get $result_ptr");

    body
}

fn build_list_schemas_body(commands: &[CommandInfo], strings: &StringTable) -> String {
    const CMD_RECORD_SIZE: i32 = 68;
    const STR_ELEM_SIZE: i32 = 8;
    // arg-schema lowers to 31 * ptrsize bytes on wasm32 (124 bytes).
    const ARG_RECORD_SIZE: i32 = 124;

    let count = commands.len() as i32;
    let list_bytes = count * CMD_RECORD_SIZE;

    let mut body = String::new();

    // Allocate the returned `(ptr, len)` pair.
    push_line(&mut body, 4, "i32.const 8");
    push_line(&mut body, 4, "call $alloc");
    push_line(&mut body, 4, "local.set $result_ptr");
    push_blank(&mut body);

    // Allocate list storage.
    push_line(&mut body, 4, &format!("i32.const {}", list_bytes));
    push_line(&mut body, 4, "call $alloc");
    push_line(&mut body, 4, "local.set $list_ptr");
    push_blank(&mut body);

    // Store list ptr/len in result.
    push_line(&mut body, 4, "local.get $result_ptr");
    push_line(&mut body, 4, "local.get $list_ptr");
    push_line(&mut body, 4, "i32.store offset=0 align=2");
    push_line(&mut body, 4, "local.get $result_ptr");
    push_line(&mut body, 4, &format!("i32.const {}", count));
    push_line(&mut body, 4, "i32.store offset=4 align=2");

    for (i, cmd) in commands.iter().enumerate() {
        // If schema is absent, synthesize it from `command_meta` for backwards-compat.
        let schema_owned;
        let schema: &wacli_metadata::CommandSchema = if let Some(s) = cmd.metadata.command_schema.as_ref() {
            s
        } else {
            schema_owned = wacli_metadata::CommandSchema::from_meta(&cmd.metadata.command_meta);
            &schema_owned
        };

        let record_offset = (i as i32) * CMD_RECORD_SIZE;

        let (name_ptr, name_len) = strings.get(&schema.name);
        let (summary_ptr, summary_len) = strings.get(&schema.summary);
        let (usage_ptr, usage_len) = strings.get(&schema.usage);
        let (version_ptr, version_len) = strings.get(&schema.version);
        let (desc_ptr, desc_len) = strings.get(&schema.description);

        push_blank(&mut body);
        push_line(&mut body, 4, "local.get $list_ptr");
        push_line(&mut body, 4, &format!("i32.const {}", record_offset));
        push_line(&mut body, 4, "i32.add");
        push_line(&mut body, 4, "local.set $record_ptr");

        // name
        emit_store_i32_const(&mut body, "$record_ptr", 0, name_ptr);
        emit_store_i32_const(&mut body, "$record_ptr", 4, name_len);

        // summary
        emit_store_i32_const(&mut body, "$record_ptr", 8, summary_ptr);
        emit_store_i32_const(&mut body, "$record_ptr", 12, summary_len);

        // usage
        emit_store_i32_const(&mut body, "$record_ptr", 16, usage_ptr);
        emit_store_i32_const(&mut body, "$record_ptr", 20, usage_len);

        // aliases list<string>
        if schema.aliases.is_empty() {
            emit_store_i32_const(&mut body, "$record_ptr", 24, 0);
            emit_store_i32_const(&mut body, "$record_ptr", 28, 0);
        } else {
            let bytes = (schema.aliases.len() as i32) * STR_ELEM_SIZE;
            push_line(&mut body, 4, &format!("i32.const {}", bytes));
            push_line(&mut body, 4, "call $alloc");
            push_line(&mut body, 4, "local.set $aliases_ptr");

            emit_store_i32_local(&mut body, "$record_ptr", 24, "$aliases_ptr");
            emit_store_i32_const(&mut body, "$record_ptr", 28, schema.aliases.len() as u32);

            for (j, alias) in schema.aliases.iter().enumerate() {
                let (ap, al) = strings.get(alias);
                let entry_off = (j as i32) * STR_ELEM_SIZE;
                // ptr
                push_line(&mut body, 4, "local.get $aliases_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", entry_off));
                push_line(&mut body, 4, "i32.add");
                push_line(&mut body, 4, &format!("i32.const {}", ap));
                push_line(&mut body, 4, "i32.store offset=0 align=2");
                // len
                push_line(&mut body, 4, "local.get $aliases_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", entry_off));
                push_line(&mut body, 4, "i32.add");
                push_line(&mut body, 4, &format!("i32.const {}", al));
                push_line(&mut body, 4, "i32.store offset=4 align=2");
            }
        }

        // version
        emit_store_i32_const(&mut body, "$record_ptr", 32, version_ptr);
        emit_store_i32_const(&mut body, "$record_ptr", 36, version_len);

        // hidden (bool)
        push_line(&mut body, 4, "local.get $record_ptr");
        push_line(
            &mut body,
            4,
            &format!("i32.const {}", if schema.hidden { 1 } else { 0 }),
        );
        push_line(&mut body, 4, "i32.store8 offset=40");

        // description
        emit_store_i32_const(&mut body, "$record_ptr", 44, desc_ptr);
        emit_store_i32_const(&mut body, "$record_ptr", 48, desc_len);

        // examples list<string>
        if schema.examples.is_empty() {
            emit_store_i32_const(&mut body, "$record_ptr", 52, 0);
            emit_store_i32_const(&mut body, "$record_ptr", 56, 0);
        } else {
            let bytes = (schema.examples.len() as i32) * STR_ELEM_SIZE;
            push_line(&mut body, 4, &format!("i32.const {}", bytes));
            push_line(&mut body, 4, "call $alloc");
            push_line(&mut body, 4, "local.set $examples_ptr");

            emit_store_i32_local(&mut body, "$record_ptr", 52, "$examples_ptr");
            emit_store_i32_const(&mut body, "$record_ptr", 56, schema.examples.len() as u32);

            for (j, ex) in schema.examples.iter().enumerate() {
                let (ep, el) = strings.get(ex);
                let entry_off = (j as i32) * STR_ELEM_SIZE;
                // ptr
                push_line(&mut body, 4, "local.get $examples_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", entry_off));
                push_line(&mut body, 4, "i32.add");
                push_line(&mut body, 4, &format!("i32.const {}", ep));
                push_line(&mut body, 4, "i32.store offset=0 align=2");
                // len
                push_line(&mut body, 4, "local.get $examples_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", entry_off));
                push_line(&mut body, 4, "i32.add");
                push_line(&mut body, 4, &format!("i32.const {}", el));
                push_line(&mut body, 4, "i32.store offset=4 align=2");
            }
        }

        // args list<arg-schema>
        if schema.args.is_empty() {
            emit_store_i32_const(&mut body, "$record_ptr", 60, 0);
            emit_store_i32_const(&mut body, "$record_ptr", 64, 0);
        } else {
            let bytes = (schema.args.len() as i32) * ARG_RECORD_SIZE;
            push_line(&mut body, 4, &format!("i32.const {}", bytes));
            push_line(&mut body, 4, "call $alloc");
            push_line(&mut body, 4, "local.set $args_ptr");

            emit_store_i32_local(&mut body, "$record_ptr", 60, "$args_ptr");
            emit_store_i32_const(&mut body, "$record_ptr", 64, schema.args.len() as u32);

            for (j, arg) in schema.args.iter().enumerate() {
                let arg_off = (j as i32) * ARG_RECORD_SIZE;
                push_blank(&mut body);
                push_line(&mut body, 4, "local.get $args_ptr");
                push_line(&mut body, 4, &format!("i32.const {}", arg_off));
                push_line(&mut body, 4, "i32.add");
                push_line(&mut body, 4, "local.set $arg_ptr");

                let (anp, anl) = strings.get(&arg.name);
                emit_store_i32_const(&mut body, "$arg_ptr", 0, anp);
                emit_store_i32_const(&mut body, "$arg_ptr", 4, anl);

                emit_store_opt_str(
                    &mut body,
                    "$arg_ptr",
                    8,
                    12,
                    16,
                    arg.short.as_deref(),
                    strings,
                );
                emit_store_opt_str(
                    &mut body,
                    "$arg_ptr",
                    20,
                    24,
                    28,
                    arg.long.as_deref(),
                    strings,
                );

                let (hp, hl) = strings.get(&arg.help);
                emit_store_i32_const(&mut body, "$arg_ptr", 32, hp);
                emit_store_i32_const(&mut body, "$arg_ptr", 36, hl);

                // required bool @40
                push_line(&mut body, 4, "local.get $arg_ptr");
                push_line(
                    &mut body,
                    4,
                    &format!("i32.const {}", if arg.required { 1 } else { 0 }),
                );
                push_line(&mut body, 4, "i32.store8 offset=40");

                emit_store_opt_str(
                    &mut body,
                    "$arg_ptr",
                    44,
                    48,
                    52,
                    arg.default_value.as_deref(),
                    strings,
                );
                emit_store_opt_str(
                    &mut body,
                    "$arg_ptr",
                    56,
                    60,
                    64,
                    arg.env.as_deref(),
                    strings,
                );
                emit_store_opt_str(
                    &mut body,
                    "$arg_ptr",
                    68,
                    72,
                    76,
                    arg.value_name.as_deref(),
                    strings,
                );

                // takes_value bool @80
                push_line(&mut body, 4, "local.get $arg_ptr");
                push_line(
                    &mut body,
                    4,
                    &format!("i32.const {}", if arg.takes_value { 1 } else { 0 }),
                );
                push_line(&mut body, 4, "i32.store8 offset=80");

                // multiple bool @81
                push_line(&mut body, 4, "local.get $arg_ptr");
                push_line(
                    &mut body,
                    4,
                    &format!("i32.const {}", if arg.multiple { 1 } else { 0 }),
                );
                push_line(&mut body, 4, "i32.store8 offset=81");

                emit_store_opt_str(
                    &mut body,
                    "$arg_ptr",
                    84,
                    88,
                    92,
                    arg.value_type.as_deref(),
                    strings,
                );

                // possible-values list<string> @96/@100
                emit_list_str(
                    &mut body,
                    "$arg_ptr",
                    96,
                    100,
                    "$values_ptr",
                    &arg.possible_values,
                    strings,
                );

                // conflicts-with list<string> @104/@108
                emit_list_str(
                    &mut body,
                    "$arg_ptr",
                    104,
                    108,
                    "$conflicts_ptr",
                    &arg.conflicts_with,
                    strings,
                );

                // requires list<string> @112/@116
                emit_list_str(
                    &mut body,
                    "$arg_ptr",
                    112,
                    116,
                    "$requires_ptr",
                    &arg.requires,
                    strings,
                );

                // hidden bool @120
                push_line(&mut body, 4, "local.get $arg_ptr");
                push_line(
                    &mut body,
                    4,
                    &format!("i32.const {}", if arg.hidden { 1 } else { 0 }),
                );
                push_line(&mut body, 4, "i32.store8 offset=120");
            }
        }
    }

    push_blank(&mut body);
    push_line(&mut body, 4, "local.get $result_ptr");

    body
}

fn build_app_meta_body(app: &AppMeta, strings: &StringTable) -> String {
    // `app-meta` record lowers to 3 strings => 6 * ptrsize bytes on wasm32 (24 bytes).
    const APP_META_RECORD_SIZE: i32 = 24;

    let (name_ptr, name_len) = strings.get(&app.name);
    let (version_ptr, version_len) = strings.get(&app.version);
    let (desc_ptr, desc_len) = strings.get(&app.description);

    let mut body = String::new();

    // Allocate record storage.
    push_line(&mut body, 4, &format!("i32.const {}", APP_META_RECORD_SIZE));
    push_line(&mut body, 4, "call $alloc");
    push_line(&mut body, 4, "local.set $result_ptr");
    push_blank(&mut body);

    // name
    emit_store_i32_const(&mut body, "$result_ptr", 0, name_ptr);
    emit_store_i32_const(&mut body, "$result_ptr", 4, name_len);

    // version
    emit_store_i32_const(&mut body, "$result_ptr", 8, version_ptr);
    emit_store_i32_const(&mut body, "$result_ptr", 12, version_len);

    // description
    emit_store_i32_const(&mut body, "$result_ptr", 16, desc_ptr);
    emit_store_i32_const(&mut body, "$result_ptr", 20, desc_len);

    push_blank(&mut body);
    push_line(&mut body, 4, "local.get $result_ptr");

    body
}

fn build_run_body(commands: &[CommandInfo], strings: &StringTable) -> String {
    let mut body = String::new();

    for cmd in commands {
        let meta = &cmd.metadata.command_meta;
        let (name_ptr, name_len) = strings.get(&meta.name);
        let ident = command_ident(&cmd.name);

        // Canonical name match.
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

        // Alias matches (static, no `meta()` call).
        for alias in &meta.aliases {
            let (ap, al) = strings.get(alias);
            push_line(&mut body, 4, "local.get $name_ptr");
            push_line(&mut body, 4, "local.get $name_len");
            push_line(&mut body, 4, &format!("i32.const {}", ap));
            push_line(&mut body, 4, &format!("i32.const {}", al));
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
    }

    // Err(command-error::unknown-command(name))
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

fn emit_list_str(
    out: &mut String,
    base_local: &str,
    ptr_offset: i32,
    len_offset: i32,
    tmp_local: &str,
    values: &[String],
    strings: &StringTable,
) {
    const STR_ELEM_SIZE: i32 = 8;

    if values.is_empty() {
        emit_store_i32_const(out, base_local, ptr_offset, 0);
        emit_store_i32_const(out, base_local, len_offset, 0);
        return;
    }

    let bytes = (values.len() as i32) * STR_ELEM_SIZE;
    push_line(out, 4, &format!("i32.const {}", bytes));
    push_line(out, 4, "call $alloc");
    push_line(out, 4, &format!("local.set {}", tmp_local));

    emit_store_i32_local(out, base_local, ptr_offset, tmp_local);
    emit_store_i32_const(out, base_local, len_offset, values.len() as u32);

    for (j, v) in values.iter().enumerate() {
        let (vp, vl) = strings.get(v);
        let entry_off = (j as i32) * STR_ELEM_SIZE;
        // ptr
        push_line(out, 4, &format!("local.get {}", tmp_local));
        push_line(out, 4, &format!("i32.const {}", entry_off));
        push_line(out, 4, "i32.add");
        push_line(out, 4, &format!("i32.const {}", vp));
        push_line(out, 4, "i32.store offset=0 align=2");
        // len
        push_line(out, 4, &format!("local.get {}", tmp_local));
        push_line(out, 4, &format!("i32.const {}", entry_off));
        push_line(out, 4, "i32.add");
        push_line(out, 4, &format!("i32.const {}", vl));
        push_line(out, 4, "i32.store offset=4 align=2");
    }
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

fn emit_store_i32_const(out: &mut String, base_local: &str, offset: i32, value: u32) {
    push_line(out, 4, &format!("local.get {base_local}"));
    push_line(out, 4, &format!("i32.const {value}"));
    push_line(out, 4, &format!("i32.store offset={offset} align=2"));
}

fn emit_store_i32_local(out: &mut String, base_local: &str, offset: i32, value_local: &str) {
    push_line(out, 4, &format!("local.get {base_local}"));
    push_line(out, 4, &format!("local.get {value_local}"));
    push_line(out, 4, &format!("i32.store offset={offset} align=2"));
}

fn emit_store_opt_str(
    out: &mut String,
    base_local: &str,
    tag_offset: i32,
    ptr_offset: i32,
    len_offset: i32,
    value: Option<&str>,
    strings: &StringTable,
) {
    let (ptr, len) = match value {
        Some(s) => strings.get(s),
        None => (0, 0),
    };
    let tag = if value.is_some() { 1 } else { 0 };

    push_line(out, 4, &format!("local.get {base_local}"));
    push_line(out, 4, &format!("i32.const {tag}"));
    push_line(out, 4, &format!("i32.store8 offset={tag_offset}"));

    emit_store_i32_const(out, base_local, ptr_offset, ptr);
    emit_store_i32_const(out, base_local, len_offset, len);
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
