use wacli_cdk::{Command, CommandError, CommandMeta, CommandResult, meta};

struct FileIo;

impl Command for FileIo {
    fn meta() -> CommandMeta {
        meta("fileio")
            .summary("Read/write/list files")
            .usage("fileio <read|write|list> ...")
            .build()
    }

    fn run(argv: Vec<String>) -> CommandResult {
        if argv.is_empty() {
            return Err(CommandError::InvalidArgs(
                "usage: fileio <read|write|list> ...".into(),
            ));
        }

        match argv[0].as_str() {
            "read" => {
                let path = argv.get(1).ok_or_else(|| {
                    CommandError::InvalidArgs("usage: fileio read <path>".into())
                })?;
                let bytes = wacli_cdk::fs::read(path)?;
                let text = String::from_utf8_lossy(&bytes);
                wacli_cdk::io::println(text);
                Ok(0)
            }
            "write" => {
                let path = argv.get(1).ok_or_else(|| {
                    CommandError::InvalidArgs("usage: fileio write <path> <text>".into())
                })?;
                let text = argv.get(2..).unwrap_or(&[]).join(" ");
                if text.is_empty() {
                    return Err(CommandError::InvalidArgs(
                        "usage: fileio write <path> <text>".into(),
                    ));
                }
                wacli_cdk::fs::write(path, text.as_bytes())?;
                wacli_cdk::io::println("ok");
                Ok(0)
            }
            "list" => {
                let path = argv.get(1).map(|s| s.as_str()).unwrap_or(".");
                let entries = wacli_cdk::fs::list_dir(path)?;
                for entry in entries {
                    wacli_cdk::io::println(entry);
                }
                Ok(0)
            }
            _ => Err(CommandError::InvalidArgs(
                "usage: fileio <read|write|list> ...".into(),
            )),
        }
    }
}

wacli_cdk::export!(FileIo);
