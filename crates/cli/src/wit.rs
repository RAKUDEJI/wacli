pub const TYPES_WIT: &str = r#"package wacli:cli@1.0.0;

interface types {
  type exit-code = u32;

  record command-meta {
    name: string,
    summary: string,
    usage: string,
    aliases: list<string>,
    version: string,
    hidden: bool,
    description: string,
    examples: list<string>,
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

pub const HOST_ENV_WIT: &str = r#"package wacli:cli@1.0.0;

interface host-env {
  args: func() -> list<string>;
  env: func() -> list<tuple<string, string>>;
}
"#;

pub const HOST_IO_WIT: &str = r#"package wacli:cli@1.0.0;

interface host-io {
  stdout-write: func(bytes: list<u8>);
  stderr-write: func(bytes: list<u8>);
  stdout-flush: func();
  stderr-flush: func();
}
"#;

pub const HOST_FS_WIT: &str = r#"package wacli:cli@1.0.0;

interface host-fs {
  read-file: func(path: string) -> result<list<u8>, string>;
  write-file: func(path: string, contents: list<u8>) -> result<_, string>;
  list-dir: func(path: string) -> result<list<string>, string>;
}
"#;

pub const HOST_PROCESS_WIT: &str = r#"package wacli:cli@1.0.0;

interface host-process {
  use types.{exit-code};

  exit: func(code: exit-code);
}
"#;

pub const HOST_PIPES_WIT: &str = r#"package wacli:cli@1.0.0;

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

pub const PIPE_RUNTIME_WIT: &str = r#"package wacli:cli@1.0.0;

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

pub const COMMAND_WIT: &str = r#"package wacli:cli@1.0.0;

interface command {
  use types.{command-meta, command-result};

  meta: func() -> command-meta;
  run: func(argv: list<string>) -> command-result;
}

world plugin {
  /// These are unqualified because they live in the same package.
  /// When embedded into a component, they resolve to:
  ///   wacli:cli/host-<name>@1.0.0
  import host-env;
  import host-io;
  import host-fs;
  import host-process;
  import host-pipes;

  export command;
}
"#;

pub const PIPE_WIT: &str = r#"package wacli:cli@1.0.0;

interface pipe {
  use types.{pipe-meta, pipe-error};

  meta: func() -> pipe-meta;
  process: func(input: list<u8>, options: list<string>) -> result<list<u8>, pipe-error>;
}

world pipe-plugin {
  export pipe;
}
"#;

pub const REGISTRY_WIT: &str = r#"package wacli:cli@1.0.0;

interface registry {
  use types.{command-meta, command-result};

  list-commands: func() -> list<command-meta>;
  run: func(name: string, argv: list<string>) -> command-result;
}

world registry-provider {
  export registry;
}
"#;
