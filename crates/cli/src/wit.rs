pub const TYPES_WIT: &str = r#"package wacli:cli@2.0.0;

interface types {
  type exit-code = u32;

  record arg-def {
    name: string,
    short: option<string>,
    long: option<string>,
    help: string,
    required: bool,
    default-value: option<string>,
    value-name: option<string>,
    takes-value: bool,
  }

  record command-meta {
    name: string,
    summary: string,
    usage: string,
    aliases: list<string>,
    version: string,
    hidden: bool,
    description: string,
    examples: list<string>,
    args: list<arg-def>,
  }

  variant command-error {
    unknown-command(string),
    invalid-args(string),
    failed(string),
    io(string),
  }

  type command-result = result<exit-code, command-error>;

  record pipe-meta {
    name: string,
    summary: string,
    input-types: list<string>,
    output-type: string,
    version: string,
  }

  variant pipe-error {
    parse-error(string),
    transform-error(string),
    invalid-option(string),
  }

  record pipe-info {
    name: string,
    summary: string,
    path: string,
  }
}
"#;

pub const HOST_ENV_WIT: &str = r#"package wacli:cli@2.0.0;

interface host-env {
  args: func() -> list<string>;
  env: func() -> list<tuple<string, string>>;
}
"#;

pub const HOST_IO_WIT: &str = r#"package wacli:cli@2.0.0;

interface host-io {
  stdout-write: func(bytes: list<u8>);
  stderr-write: func(bytes: list<u8>);
  stdout-flush: func();
  stderr-flush: func();
}
"#;

pub const HOST_FS_WIT: &str = r#"package wacli:cli@2.0.0;

interface host-fs {
  read-file: func(path: string) -> result<list<u8>, string>;
  write-file: func(path: string, contents: list<u8>) -> result<_, string>;
  create-dir: func(path: string) -> result<_, string>;
  list-dir: func(path: string) -> result<list<string>, string>;
}
"#;

pub const HOST_PROCESS_WIT: &str = r#"package wacli:cli@2.0.0;

interface host-process {
  use types.{exit-code};

  exit: func(code: exit-code);
}
"#;

pub const HOST_PIPES_WIT: &str = r#"package wacli:cli@2.0.0;

interface host-pipes {
  use types.{pipe-meta, pipe-error, pipe-info};

  list-pipes: func() -> list<pipe-info>;
  load-pipe: func(name: string) -> result<pipe, string>;

  resource pipe {
    meta: func() -> pipe-meta;
    process: func(input: list<u8>, options: list<string>) -> result<list<u8>, pipe-error>;
  }
}
"#;

pub const PIPE_RUNTIME_WIT: &str = r#"package wacli:cli@2.0.0;

interface pipe-runtime {
  use types.{pipe-meta, pipe-error, pipe-info};

  list-pipes: func() -> list<pipe-info>;
  load-pipe: func(name: string) -> result<pipe, string>;

  resource pipe {
    meta: func() -> pipe-meta;
    process: func(input: list<u8>, options: list<string>) -> result<list<u8>, pipe-error>;
  }
}

world pipe-runtime-host {
  import pipe-runtime;
}
"#;

pub const COMMAND_WIT: &str = r#"package wacli:cli@2.0.0;

interface command {
  use types.{command-meta, command-result};

  meta: func() -> command-meta;
  run: func(argv: list<string>) -> command-result;
}

world plugin {
  /// These are unqualified because they live in the same package.
  /// When embedded into a component, they resolve to:
  ///   wacli:cli/host-<name>@2.0.0
  import host-env;
  import host-io;
  import host-fs;
  import host-process;
  import host-pipes;

  export command;
}
"#;

pub const PIPE_WIT: &str = r#"package wacli:cli@2.0.0;

interface pipe {
  use types.{pipe-meta, pipe-error};

  meta: func() -> pipe-meta;
  process: func(input: list<u8>, options: list<string>) -> result<list<u8>, pipe-error>;
}

world pipe-plugin {
  export pipe;
}
"#;

pub const REGISTRY_WIT: &str = r#"package wacli:cli@2.0.0;

interface registry {
  use types.{command-meta, command-result};

  list-commands: func() -> list<command-meta>;
  run: func(name: string, argv: list<string>) -> command-result;
}

world registry-provider {
  export registry;
}
"#;

pub const SCHEMA_WIT: &str = r#"package wacli:cli@2.0.0;

/// Expressive CLI schema for clap-like behavior.
///
/// This is intentionally more semantic than `types.arg-def`, allowing core-side
/// help/version/validation without executing the plugin.
interface schema {
  record arg-schema {
    name: string,
    short: option<string>,
    long: option<string>,
    help: string,
    required: bool,
    default-value: option<string>,
    env: option<string>,
    value-name: option<string>,
    takes-value: bool,
    multiple: bool,
    value-type: option<string>,
    possible-values: list<string>,
    conflicts-with: list<string>,
    requires: list<string>,
    hidden: bool,
  }

  record command-schema {
    name: string,
    summary: string,
    usage: string,
    aliases: list<string>,
    version: string,
    hidden: bool,
    description: string,
    examples: list<string>,
    args: list<arg-schema>,
  }
}
"#;

pub const REGISTRY_SCHEMA_WIT: &str = r#"package wacli:cli@2.0.0;

interface registry-schema {
  use schema.{command-schema};

  /// App-level metadata, provided by the builder (wacli).
  ///
  /// This is used by core to render global `--help/--version` consistently.
  record app-meta {
    name: string,
    version: string,
    description: string,
  }

  /// Return app-level metadata for the composed CLI.
  get-app-meta: func() -> app-meta;

  /// Return schemas for all commands.
  ///
  /// The schema is pure data and must be available without executing the plugin.
  list-schemas: func() -> list<command-schema>;
}
"#;
