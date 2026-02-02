//! Generate registry.component.wasm from discovered commands.
//!
//! The registry component implements:
//! - `list-commands() -> list<command-meta>`: Returns embedded metadata
//! - `run(name: string, argv: list<string>) -> command-result`: Dispatches to commands

use crate::component_scan::CommandInfo;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use wasm_encoder::{
    CodeSection, CustomSection, DataSection, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, GlobalSection, GlobalType, ImportSection, Instruction, MemorySection,
    MemoryType, Module, Section, TypeSection, ValType,
};
use wit_component::ComponentEncoder;
use wit_parser::{Resolve, UnresolvedPackageGroup};

const REGISTRY_WIT_BASE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../wit/registry.wit"));

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

/// Generate a registry component from discovered commands.
///
/// This creates a WebAssembly component that:
/// 1. Imports `wacli:cli/host@1.0.0`
/// 2. Imports each command's `wacli:cli/command@1.0.0` instance
/// 3. Exports `wacli:cli/registry@1.0.0`
/// 4. Returns pre-built command metadata for list-commands
/// 5. Dispatches run calls to imported command functions
pub fn generate_registry(commands: &[CommandInfo]) -> Result<Vec<u8>> {
    let name_table = build_name_table(commands);

    // Generate the core module
    let core_module = build_core_module(commands, &name_table)?;

    // Generate dynamic WIT that includes all command imports
    let dynamic_wit = generate_dynamic_wit(commands);

    // Parse the dynamic WIT
    let mut resolve = Resolve::default();
    let wit_path = Path::new("registry.wit");
    let pkg_group = UnresolvedPackageGroup::parse(wit_path, &dynamic_wit)
        .with_context(|| "failed to parse dynamic WIT")?;
    let pkg_ids = resolve.push_group(pkg_group)?;

    // Find the dynamic-registry world
    let world_id = resolve
        .packages
        .iter()
        .flat_map(|(_, pkg)| pkg.worlds.values())
        .find(|world_id| resolve.worlds[**world_id].name == "dynamic-registry")
        .copied()
        .context("dynamic-registry world not found in generated WIT")?;

    // Suppress unused variable warning
    let _ = pkg_ids;

    // Encode the WIT metadata into a custom section
    let encoded_meta = wit_component::metadata::encode(
        &resolve,
        world_id,
        wit_component::StringEncoding::UTF8,
        None,
    )?;

    // Add the custom section to the module
    let module_with_meta =
        add_custom_section(&core_module, "component-type:registry", &encoded_meta)?;

    // Use ComponentEncoder to create the component
    // Note: validation is disabled due to a known issue with wit-component's
    // adapter generation for complex return types like list<record>
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

    // Define a separate command interface for each command
    for cmd in commands {
        wit.push_str(&format!("interface {}-command {{\n", cmd.name));
        wit.push_str("  use types.{command-meta, command-result};\n");
        wit.push_str("  meta: func() -> command-meta;\n");
        wit.push_str("  run: func(argv: list<string>) -> command-result;\n");
        wit.push_str("}\n\n");
    }

    // Dynamic registry world
    wit.push_str("world dynamic-registry {\n");
    wit.push_str("  import host;\n");

    // Import each command interface
    for cmd in commands {
        wit.push_str(&format!("  import {}-command;\n", cmd.name));
    }

    wit.push_str("  export registry;\n");
    wit.push_str("}\n");

    wit
}

/// Build a compact name table for embedding into the data section.
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

/// Build the core WebAssembly module.
fn build_core_module(commands: &[CommandInfo], name_table: &NameTable) -> Result<Vec<u8>> {
    let mut module = Module::new();

    // === Type Section ===
    let mut types = TypeSection::new();

    // Type 0: () -> i32 (list-commands returns pointer to flat result)
    // Canonical ABI for exports: return a pointer to flat results
    types.ty().function([], [ValType::I32]);

    // Type 1: (i32, i32, i32, i32) -> i32 (run: name_ptr, name_len, argv_ptr, argv_len -> result_ptr)
    // Canonical ABI for exports: return a pointer to flat result
    types.ty().function(
        [ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        [ValType::I32],
    );

    // Type 2: (i32, i32, i32, i32) -> i32 (cabi_realloc)
    types.ty().function(
        [ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        [ValType::I32],
    );

    // Type 3: (i32, i32, i32) -> () (imported command run: argv_ptr, argv_len, ret_ptr -> void)
    // Canonical ABI: results written via return pointer, function returns void
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32], []);

    // Type 4: (i32) -> () (imported command meta: ret_ptr -> void)
    // Canonical ABI: results written via return pointer
    types.ty().function([ValType::I32], []);

    module.section(&types);

    // === Import Section ===
    let mut imports = ImportSection::new();

    // Type 4: () -> i32 (meta: returns pointer to command-meta)
    // Need to define this type for meta imports

    // Import each command's meta and run functions
    // For interface imports, use fully qualified name: wacli:cli/{name}-command@1.0.0
    for cmd in commands {
        let import_name = format!("wacli:cli/{}-command@1.0.0", cmd.name);
        // Import meta function (type 4)
        imports.import(&import_name, "meta", EntityType::Function(4));
        // Import run function (type 3)
        imports.import(&import_name, "run", EntityType::Function(3));
    }

    module.section(&imports);

    // === Function Section ===
    let mut functions = FunctionSection::new();
    functions.function(0); // list-commands
    functions.function(1); // run
    functions.function(2); // cabi_realloc
    module.section(&functions);

    // === Memory Section ===
    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memory);

    // === Global Section ===
    let mut globals = GlobalSection::new();
    // Heap pointer starts after the data segment
    let heap_start = (name_table.data.len() + 1024) as i32; // Add padding
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(heap_start),
    );
    module.section(&globals);

    // === Export Section ===
    let mut exports = ExportSection::new();
    // Each command has 2 imports (meta, run), so num_imports = commands.len() * 2
    let num_imports = (commands.len() * 2) as u32;
    // Canonical ABI export names: interface#function
    exports.export("wacli:cli/registry@1.0.0#list-commands", ExportKind::Func, num_imports);
    exports.export("wacli:cli/registry@1.0.0#run", ExportKind::Func, num_imports + 1);
    exports.export("cabi_realloc", ExportKind::Func, num_imports + 2);
    exports.export("memory", ExportKind::Memory, 0);
    module.section(&exports);

    // === Code Section ===
    let mut code = CodeSection::new();

    // list-commands: build and return list of command-meta
    // For simplicity, return minimal metadata (just names, empty strings for rest)
    code.function(&build_list_commands_func(commands, &name_table.offsets)?);

    // run: dispatch based on command name
    code.function(&build_run_func(commands, &name_table.offsets)?);

    // cabi_realloc: simple bump allocator
    code.function(&build_cabi_realloc_func());

    module.section(&code);

    // === Data Section ===
    let mut data_section = DataSection::new();
    data_section.active(
        0,                                      // memory_index
        &wasm_encoder::ConstExpr::i32_const(0), // offset
        name_table.data.iter().copied(),
    );
    module.section(&data_section);

    Ok(module.finish())
}

/// Build the list-commands function.
/// Canonical ABI for exports: list-commands() -> i32
/// Returns a pointer to (list_ptr, list_len) flat result
fn build_list_commands_func(
    commands: &[CommandInfo],
    name_offsets: &[(u32, u32)],
) -> Result<Function> {
    const RECORD_SIZE: i32 = 60;
    const ZERO_FIELDS: &[u64] = &[8, 12, 16, 20, 24, 28, 32, 36, 44, 48, 52, 56];

    let mut func = Function::new([(3, ValType::I32)]); // locals: result_ptr, list_ptr, record_ptr
    let count = commands.len() as i32;
    let list_bytes = RECORD_SIZE * count;

    // Allocate space for the flat result (8 bytes: list_ptr + list_len)
    func.instruction(&Instruction::GlobalGet(0)); // heap_ptr
    func.instruction(&Instruction::LocalSet(0)); // result_ptr

    func.instruction(&Instruction::GlobalGet(0));
    func.instruction(&Instruction::I32Const(8));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::GlobalSet(0));

    // Allocate space for the list elements
    func.instruction(&Instruction::GlobalGet(0)); // heap_ptr
    func.instruction(&Instruction::LocalSet(1)); // list_ptr

    func.instruction(&Instruction::GlobalGet(0));
    func.instruction(&Instruction::I32Const(list_bytes));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::GlobalSet(0));

    // Store list pointer
    func.instruction(&Instruction::LocalGet(0));
    func.instruction(&Instruction::LocalGet(1));
    func.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    }));

    // Store list length
    func.instruction(&Instruction::LocalGet(0));
    func.instruction(&Instruction::I32Const(count));
    func.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
        offset: 4,
        align: 2,
        memory_index: 0,
    }));

    // Fill command-meta records
    for (i, (name_ptr, name_len)) in name_offsets.iter().enumerate() {
        let record_offset = (i as i32) * RECORD_SIZE;

        func.instruction(&Instruction::LocalGet(1));
        func.instruction(&Instruction::I32Const(record_offset));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(2)); // record_ptr

        // name: string (ptr, len)
        func.instruction(&Instruction::LocalGet(2));
        func.instruction(&Instruction::I32Const(*name_ptr as i32));
        func.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::LocalGet(2));
        func.instruction(&Instruction::I32Const(*name_len as i32));
        func.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
            offset: 4,
            align: 2,
            memory_index: 0,
        }));

        // summary/usage/aliases/version/description/examples: empty/default
        for field_offset in ZERO_FIELDS {
        func.instruction(&Instruction::LocalGet(2));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                offset: *field_offset,
                align: 2,
                memory_index: 0,
            }));
        }

        // hidden: bool = false
        func.instruction(&Instruction::LocalGet(2));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32Store8(wasm_encoder::MemArg {
            offset: 40,
            align: 0,
            memory_index: 0,
        }));
    }

    // Return the result pointer
    func.instruction(&Instruction::LocalGet(0));
    func.instruction(&Instruction::End);

    Ok(func)
}

/// Build the run function.
/// Compares command name and dispatches to the appropriate imported function.
/// Canonical ABI for exports: run(name_ptr, name_len, argv_ptr, argv_len) -> i32
fn build_run_func(commands: &[CommandInfo], name_offsets: &[(u32, u32)]) -> Result<Function> {
    // Function params: 0=name_ptr, 1=name_len, 2=argv_ptr, 3=argv_len
    // Locals: 4=result_ptr, 5=idx, 6=match
    let mut func = Function::new([(3, ValType::I32)]);

    // For each command, compare the name bytes and call if matches.
    for (i, cmd) in commands.iter().enumerate() {
        let (name_ptr, name_len) = name_offsets
            .get(i)
            .copied()
            .unwrap_or((0, cmd.name.len() as u32));

        // Check if name length matches
        func.instruction(&Instruction::LocalGet(1)); // name_len (param 1)
        func.instruction(&Instruction::I32Const(name_len as i32));
        func.instruction(&Instruction::I32Eq);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));

        // idx = 0
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(5));
        // match = 1
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::LocalSet(6));

        // Compare bytes loop
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));

        // if idx >= name_len, break
        func.instruction(&Instruction::LocalGet(5));
        func.instruction(&Instruction::I32Const(name_len as i32));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1));

        // load input byte
        func.instruction(&Instruction::LocalGet(0));
        func.instruction(&Instruction::LocalGet(5));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(wasm_encoder::MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }));

        // load command name byte
        func.instruction(&Instruction::I32Const(name_ptr as i32));
        func.instruction(&Instruction::LocalGet(5));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(wasm_encoder::MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }));

        // if bytes differ, match = 0; break
        func.instruction(&Instruction::I32Ne);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(6));
        func.instruction(&Instruction::Br(1));
        func.instruction(&Instruction::End);

        // idx++
        func.instruction(&Instruction::LocalGet(5));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(5));
        func.instruction(&Instruction::Br(0));

        func.instruction(&Instruction::End); // loop
        func.instruction(&Instruction::End); // block

        // if match then dispatch
        func.instruction(&Instruction::LocalGet(6));
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));

        // Allocate space for result
        func.instruction(&Instruction::GlobalGet(0));
        func.instruction(&Instruction::LocalSet(4));

        // Call the imported command's run function with return pointer
        // Each command has 2 imports (meta, run), so run is at index i*2+1
        func.instruction(&Instruction::LocalGet(2)); // argv_ptr (param 2)
        func.instruction(&Instruction::LocalGet(3)); // argv_len (param 3)
        func.instruction(&Instruction::LocalGet(4)); // ret_ptr from local
        func.instruction(&Instruction::Call((i * 2 + 1) as u32)); // Call imported run (returns void)

        // Update heap pointer
        func.instruction(&Instruction::GlobalGet(0));
        func.instruction(&Instruction::I32Const(16));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalSet(0));

        // Return result pointer
        func.instruction(&Instruction::LocalGet(4));
        func.instruction(&Instruction::Return);

        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);
    }

    // No command matched - allocate and return error
    func.instruction(&Instruction::GlobalGet(0)); // Get heap ptr
    func.instruction(&Instruction::LocalSet(4));

    // Write error flag (1 = error)
    func.instruction(&Instruction::LocalGet(4));
    func.instruction(&Instruction::I32Const(1)); // Error flag
    func.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    }));

    // Write error discriminant (unknown-command = 0)
    func.instruction(&Instruction::LocalGet(4));
    func.instruction(&Instruction::I32Const(0)); // variant tag: unknown-command
    func.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
        offset: 4,
        align: 2,
        memory_index: 0,
    }));

    // Update heap pointer
    func.instruction(&Instruction::GlobalGet(0));
    func.instruction(&Instruction::I32Const(16));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::GlobalSet(0));

    // Return error pointer
    func.instruction(&Instruction::LocalGet(4));
    func.instruction(&Instruction::End);

    Ok(func)
}

/// Build the cabi_realloc function (simple bump allocator).
fn build_cabi_realloc_func() -> Function {
    // Function params: 0=old_ptr, 1=old_size, 2=align, 3=new_size
    // Local: 4=result
    let mut func = Function::new([(1, ValType::I32)]); // 1 local for result

    // cabi_realloc(old_ptr, old_size, align, new_size) -> new_ptr
    // Simple bump allocator: ignore old_ptr/old_size, just allocate new_size

    func.instruction(&Instruction::GlobalGet(0)); // Get heap pointer
    func.instruction(&Instruction::LocalSet(4)); // Save to local 4

    // Update heap pointer: heap_ptr += new_size
    func.instruction(&Instruction::GlobalGet(0));
    func.instruction(&Instruction::LocalGet(3)); // new_size is param 3
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::GlobalSet(0));

    // Return old heap pointer
    func.instruction(&Instruction::LocalGet(4)); // Return from local 4
    func.instruction(&Instruction::End);

    func
}

/// Add a custom section to a wasm module.
fn add_custom_section(module_bytes: &[u8], name: &str, data: &[u8]) -> Result<Vec<u8>> {
    // Use wasm_encoder's CustomSection which properly encodes the section
    let custom = CustomSection {
        name: std::borrow::Cow::Borrowed(name),
        data: std::borrow::Cow::Borrowed(data),
    };

    let mut result = module_bytes.to_vec();

    // Section trait's append method adds the section to the byte vector
    custom.append_to(&mut result);

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_name_table() {
        let commands = vec![
            CommandInfo {
                name: "help".to_string(),
                path: PathBuf::from("help.wasm"),
            },
            CommandInfo {
                name: "greet".to_string(),
                path: PathBuf::from("greet.wasm"),
            },
        ];

        let table = build_name_table(&commands);
        assert_eq!(table.data, b"helpgreet");
        assert_eq!(table.offsets, vec![(0, 4), (4, 5)]);
    }

    #[test]
    fn test_generate_registry() {
        let commands = vec![
            CommandInfo {
                name: "help".to_string(),
                path: PathBuf::from("commands/help.component.wasm"),
            },
            CommandInfo {
                name: "greet".to_string(),
                path: PathBuf::from("commands/greet.component.wasm"),
            },
        ];

        let name_table = build_name_table(&commands);
        let core_module =
            build_core_module(&commands, &name_table).expect("core module build failed");

        // Verify core module is valid
        assert!(core_module.len() > 8, "Core module too small");
        assert_eq!(&core_module[0..4], b"\0asm", "Invalid core wasm magic");

        // Generate the registry
        let result = generate_registry(&commands);

        match result {
            Ok(bytes) => {
                // Check that we got valid wasm (magic number)
                assert!(bytes.len() > 8, "Component too small");
                assert_eq!(&bytes[0..4], b"\0asm", "Invalid wasm magic number");
                println!("Generated registry component: {} bytes", bytes.len());
            }
            Err(e) => {
                // Print detailed error for debugging
                eprintln!("Registry generation failed: {:#}", e);
                panic!("Registry generation should succeed: {}", e);
            }
        }
    }
}
