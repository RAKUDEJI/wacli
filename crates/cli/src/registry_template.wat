(module
  (type $list-commands (func (result i32)))
  (type $list-schemas (func (result i32)))
  (type $app-meta (func (result i32)))
  (type $run (func (param i32 i32 i32 i32) (result i32)))
  (type $cabi_realloc (func (param i32 i32 i32 i32) (result i32)))
  (type $import_run (func (param i32 i32 i32)))
  (type $import_meta (func (param i32)))

{{IMPORTS}}

  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const {{HEAP_START}}))

  (func $alloc-align (param $size i32) (param $align i32) (result i32)
    (local $ptr i32)
    (local $mask i32)
    (local $use_align i32)
    local.get $align
    i32.const 4
    i32.lt_u
    if (result i32)
      i32.const 4
    else
      local.get $align
    end
    local.set $use_align
    local.get $use_align
    i32.const 1
    i32.sub
    local.set $mask
    global.get $heap
    local.set $ptr
    local.get $ptr
    local.get $mask
    i32.add
    local.get $mask
    i32.const -1
    i32.xor
    i32.and
    local.set $ptr
    local.get $ptr
    local.get $size
    i32.add
    global.set $heap
    local.get $ptr)

  (func $alloc (param $size i32) (result i32)
    local.get $size
    i32.const 4
    call $alloc-align)

  (func $match-name (param $name_ptr i32) (param $name_len i32) (param $target_ptr i32) (param $target_len i32) (result i32)
    (local $i i32)
    (local $match i32)
    local.get $name_len
    local.get $target_len
    i32.ne
    if
      i32.const 0
      return
    end
    i32.const 0
    local.set $i
    i32.const 1
    local.set $match
    block $done
      loop $loop
        local.get $i
        local.get $name_len
        i32.ge_u
        br_if $done
        local.get $name_ptr
        local.get $i
        i32.add
        i32.load8_u
        local.get $target_ptr
        local.get $i
        i32.add
        i32.load8_u
        i32.ne
        if
          i32.const 0
          local.set $match
          br $done
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br $loop
      end
    end
    local.get $match)

  (func $list-commands (type $list-commands) (result i32)
    (local $result_ptr i32)
    (local $list_ptr i32)
    (local $record_ptr i32)
    (local $aliases_ptr i32)
    (local $examples_ptr i32)
    (local $args_ptr i32)
    (local $arg_ptr i32)
{{LIST_COMMANDS_BODY}}
  )

  (func $list-schemas (type $list-schemas) (result i32)
    (local $result_ptr i32)
    (local $list_ptr i32)
    (local $record_ptr i32)
    (local $aliases_ptr i32)
    (local $examples_ptr i32)
    (local $args_ptr i32)
    (local $arg_ptr i32)
    (local $values_ptr i32)
    (local $conflicts_ptr i32)
    (local $requires_ptr i32)
{{LIST_SCHEMAS_BODY}}
  )

  (func $app-meta (type $app-meta) (result i32)
    (local $result_ptr i32)
{{APP_META_BODY}}
  )

  (func $run (type $run) (param $name_ptr i32) (param $name_len i32) (param $argv_ptr i32) (param $argv_len i32) (result i32)
    (local $ret_ptr i32)
{{RUN_BODY}}
  )

  (func $cabi_realloc (type $cabi_realloc) (param $old_ptr i32) (param $old_size i32) (param $align i32) (param $new_size i32) (result i32)
    local.get $new_size
    local.get $align
    call $alloc-align)

  (export "wacli:cli/registry@2.0.0#list-commands" (func $list-commands))
  (export "wacli:cli/registry-schema@2.0.0#list-schemas" (func $list-schemas))
  (export "wacli:cli/registry-schema@2.0.0#get-app-meta" (func $app-meta))
  (export "wacli:cli/registry@2.0.0#run" (func $run))
  (export "cabi_realloc" (func $cabi_realloc))

  (data (i32.const 0) "{{STRING_DATA}}")
)
