use wacli_cdk::{Command, CommandMeta, CommandResult, meta};

struct Greet;

impl Command for Greet {
    fn meta() -> CommandMeta {
        meta("greet")
            .summary("Greet someone")
            .usage("greet [NAME]")
            .build()
    }

    fn run(argv: Vec<String>) -> CommandResult {
        let name = argv.first().map(|s| s.as_str()).unwrap_or("World");
        wacli_cdk::io::println(&format!("Hello, {}!", name));
        Ok(0)
    }
}

wacli_cdk::export!(Greet);
