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
}
"#;

pub const HOST_WIT: &str = r#"package wacli:cli@1.0.0;

interface host {
  use types.{exit-code};

  args: func() -> list<string>;
  env: func() -> list<tuple<string, string>>;

  stdout-write: func(bytes: list<u8>);
  stderr-write: func(bytes: list<u8>);
  stdout-flush: func();
  stderr-flush: func();

  read-file: func(path: string) -> result<list<u8>, string>;
  write-file: func(path: string, contents: list<u8>) -> result<_, string>;
  list-dir: func(path: string) -> result<list<string>, string>;

  exit: func(code: exit-code);
}
"#;

pub const COMMAND_WIT: &str = r#"package wacli:cli@1.0.0;

interface command {
  use types.{command-meta, command-result};

  meta: func() -> command-meta;
  run: func(argv: list<string>) -> command-result;
}

world plugin {
  import host;

  export command;
}
"#;

pub const REGISTRY_WIT: &str = r#"package wacli:cli@1.0.0;

interface registry {
  use types.{command-meta, command-result};

  list-commands: func() -> list<command-meta>;
  run: func(name: string, argv: list<string>) -> command-result;
}

world registry-provider {
  import host;
  export registry;
}
"#;
