use wacli_cdk::{Command, CommandMeta, CommandResult, Context, arg, meta, parse};

struct Need;

impl Command for Need {
    fn meta() -> CommandMeta {
        meta("need")
            .summary("Demonstrate required args")
            .usage("need <TEXT>")
            .arg(arg("text").required(true).value_name("TEXT").help("Text to print"))
            .build()
    }

    fn run(argv: Vec<String>) -> CommandResult {
        let ctx = Context::new(argv);
        let m = parse(&Self::meta(), &ctx.argv)?;
        let text = m.get("text").unwrap();
        wacli_cdk::io::println(text);
        Ok(0)
    }
}

wacli_cdk::export!(Need);

