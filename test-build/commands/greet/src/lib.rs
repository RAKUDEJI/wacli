use wacli_cdk::{Command, CommandMeta, CommandResult};

wacli_cdk::declare_command_metadata!(greet_meta, {
    name: "greet",
    summary: "Greet someone",
    usage: "greet [NAME]",
    aliases: ["hi"],
});

struct Greet;

impl Command for Greet {
    fn meta() -> CommandMeta {
        greet_meta()
    }

    fn run(argv: Vec<String>) -> CommandResult {
        let name = argv.first().map(|s| s.as_str()).unwrap_or("World");
        wacli_cdk::io::println(&format!("Hello, {}!", name));
        Ok(0)
    }
}

wacli_cdk::export!(Greet);
