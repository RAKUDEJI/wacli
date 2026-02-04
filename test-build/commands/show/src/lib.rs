use wacli_cdk::{meta, pipes, Command, CommandError, CommandMeta, CommandResult, Context};

struct Show;

impl Command for Show {
    fn meta() -> CommandMeta {
        meta("show")
            .summary("Show text with optional pipe formatting")
            .usage("show [--format <PIPE>] [TEXT]")
            .build()
    }

    fn run(argv: Vec<String>) -> CommandResult {
        let ctx = Context::new(argv);
        let format = ctx.value("--format");
        let input = extract_input(&ctx).unwrap_or("hello world");

        if let Some(pipe_name) = format {
            let pipe = pipes::load(pipe_name)
                .map_err(|e| CommandError::Failed(format!("failed to load pipe '{pipe_name}': {e}")))?;
            let output = pipe
                .process(input.as_bytes(), &[])
                .map_err(|e| CommandError::Failed(format!("pipe error: {e:?}")))?;
            wacli_cdk::io::print(String::from_utf8_lossy(&output));
            Ok(0)
        } else {
            wacli_cdk::io::print(input);
            Ok(0)
        }
    }
}

wacli_cdk::export!(Show);

fn extract_input(ctx: &Context) -> Option<&str> {
    let mut after_separator = false;
    let mut iter = ctx.argv.iter();
    while let Some(arg) = iter.next() {
        if !after_separator {
            if arg == "--" {
                after_separator = true;
                continue;
            }
            if arg == "--format" {
                iter.next();
                continue;
            }
            if arg.starts_with('-') {
                continue;
            }
        }
        return Some(arg.as_str());
    }
    None
}
