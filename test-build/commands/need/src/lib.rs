use wacli_cdk::{Command, CommandMeta, CommandResult, Context, parse};

wacli_cdk::declare_command_metadata!(need_meta, {
    name: "need",
    summary: "Demonstrate required args",
    usage: "need [OPTIONS] <TEXT>",
    args: [
        {
            name: "text",
            value_name: "TEXT",
            help: "Text to print",
            required: true
        },
        {
            name: "case",
            long: "--case",
            value_name: "CASE",
            help: "Text casing",
            default_value: "upper",
            possible_values: ["upper", "lower"],
            multiple: false,
            conflicts_with: ["raw"]
        },
        {
            name: "raw",
            long: "--raw",
            help: "Disable transformations",
            conflicts_with: ["case"]
        },
        {
            name: "tag",
            long: "--tag",
            value_name: "TAG",
            help: "Prefix tag",
            multiple: false
        },
        {
            name: "with_tag",
            long: "--with-tag",
            help: "Require --tag and prefix output",
            requires: ["tag"]
        },
        {
            name: "internal",
            long: "--internal",
            help: "Internal flag (hidden)",
            hidden: true
        },
    ],
});

struct Need;

impl Command for Need {
    fn meta() -> CommandMeta {
        need_meta()
    }

    fn run(argv: Vec<String>) -> CommandResult {
        let ctx = Context::new(argv);
        let m = parse(&Self::meta(), &ctx.argv)?;
        let text = m.get("text").unwrap_or_default();
        let raw = m.is_present("raw");
        let case = m.get("case").unwrap_or("upper");
        let tag = m.get("tag");
        let with_tag = m.is_present("with_tag");

        let mut out = text.to_string();
        if !raw {
            match case {
                "upper" => out = out.to_uppercase(),
                "lower" => out = out.to_lowercase(),
                _ => {}
            }
        }

        if with_tag {
            if let Some(t) = tag {
                out = format!("[{t}] {out}");
            }
        } else if let Some(t) = tag {
            out = format!("[{t}] {out}");
        }

        wacli_cdk::io::println(&out);
        Ok(0)
    }
}

wacli_cdk::export!(Need);
