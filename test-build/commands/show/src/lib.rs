use wacli_cdk::{parse, pipes, Command, CommandError, CommandMeta, CommandResult, Context};

wacli_cdk::declare_command_metadata!(show_meta, {
    name: "show",
    summary: "Show text with optional pipe formatting",
    usage: "show [--format <PIPE>] [TEXT]",
    args: [
        {
            name: "format",
            long: "--format",
            value_name: "PIPE",
            help: "Pipe to apply to the input"
        },
        { name: "text", value_name: "TEXT", help: "Text to show" }
    ],
});

struct Show;

impl Command for Show {
    fn meta() -> CommandMeta {
        show_meta()
    }

    fn run(argv: Vec<String>) -> CommandResult {
        let ctx = Context::new(argv);
        let matches = parse(&Self::meta(), &ctx.argv)?;
        let format = matches.get("format");
        let input = matches.get("text").unwrap_or("hello world");

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
