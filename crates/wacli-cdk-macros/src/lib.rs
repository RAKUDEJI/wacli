use proc_macro::TokenStream;
use quote::quote;
use syn::{
    bracketed, braced,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    token::{Bracket, Brace},
    Ident, LitBool, LitByteStr, LitStr, Result, Token,
};

/// Declare command metadata and embed it into a WASM custom section.
///
/// This generates:
/// - a function `<ident>() -> wacli_cdk::CommandMeta`
/// - a `#[link_section]` static containing JSON metadata (no plugin execution required)
///
/// Syntax (kebab-case JSON keys are derived; this is Rust syntax):
///
/// ```ignore
/// wacli_cdk::declare_command_metadata!(show_meta, {
///   name: "show",
///   summary: "Show text",
///   usage: "show [OPTIONS] [TEXT]",
///   aliases: ["s"],
///   version: "0.1.0",
///   hidden: false,
///   description: "Longer help...",
///   examples: ["show hello"],
///   args: [
///     { name: "format", long: "--format", value_name: "PIPE", help: "Pipe name",
///       env: "SHOW_FORMAT", possible_values: ["plain", "json"], multiple: false,
///       conflicts_with: ["raw"], requires: ["text"] },
///     { name: "text", value_name: "TEXT", help: "Text to show" },
///   ],
/// });
/// ```
#[proc_macro]
pub fn declare_command_metadata(input: TokenStream) -> TokenStream {
    let decl = parse_macro_input!(input as Decl);

    match expand_decl(decl) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

struct Decl {
    func_ident: Ident,
    _comma: Token![,],
    body: Body,
}

impl Parse for Decl {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            func_ident: input.parse()?,
            _comma: input.parse()?,
            body: input.parse()?,
        })
    }
}

struct Body {
    _brace: Brace,
    fields: Punctuated<Field, Token![,]>,
}

impl Parse for Body {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        let brace = braced!(content in input);
        Ok(Self {
            _brace: brace,
            fields: content.parse_terminated(Field::parse, Token![,])?,
        })
    }
}

struct Field {
    key: Ident,
    _colon: Token![:],
    value: Value,
}

impl Parse for Field {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            key: input.parse()?,
            _colon: input.parse()?,
            value: input.parse()?,
        })
    }
}

enum Value {
    Str(LitStr),
    Bool(LitBool),
    StrArray(Vec<LitStr>),
    ArgsArray(Vec<ArgObject>),
}

impl Parse for Value {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(LitStr) {
            return Ok(Self::Str(input.parse()?));
        }
        if input.peek(LitBool) {
            return Ok(Self::Bool(input.parse()?));
        }
        if input.peek(Bracket) {
            let content;
            bracketed!(content in input);

            if content.is_empty() {
                return Ok(Self::StrArray(Vec::new()));
            }

            // If the first element is a `{ ... }`, treat this as `args: [ {..}, .. ]`.
            if content.peek(Brace) {
                let elems: Punctuated<ArgObject, Token![,]> =
                    content.parse_terminated(ArgObject::parse, Token![,])?;
                return Ok(Self::ArgsArray(elems.into_iter().collect()));
            }

            // Otherwise expect `[ "a", "b" ]`.
            let elems: Punctuated<LitStr, Token![,]> = content.parse_terminated(
                |p: ParseStream<'_>| p.parse::<LitStr>(),
                Token![,],
            )?;
            return Ok(Self::StrArray(elems.into_iter().collect()));
        }

        Err(syn::Error::new(
            input.span(),
            "expected string literal, boolean literal, or [ ... ] array",
        ))
    }
}

struct ArgObject {
    _brace: Brace,
    fields: Punctuated<ArgField, Token![,]>,
}

impl Parse for ArgObject {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        let brace = braced!(content in input);
        Ok(Self {
            _brace: brace,
            fields: content.parse_terminated(ArgField::parse, Token![,])?,
        })
    }
}

struct ArgField {
    key: Ident,
    _colon: Token![:],
    value: Value,
}

impl Parse for ArgField {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            key: input.parse()?,
            _colon: input.parse()?,
            value: input.parse()?,
        })
    }
}

#[derive(Default)]
struct CommandSpec {
    name: Option<String>,
    summary: String,
    usage: String,
    aliases: Vec<String>,
    version: Option<String>,
    hidden: bool,
    description: String,
    examples: Vec<String>,
    args: Vec<ArgSpec>,
}

#[derive(Default)]
struct ArgSpec {
    name: Option<String>,
    short: Option<String>,
    long: Option<String>,
    help: String,
    required: bool,
    default_value: Option<String>,
    env: Option<String>,
    value_name: Option<String>,
    takes_value: Option<bool>,
    multiple: Option<bool>,
    value_type: Option<String>,
    possible_values: Vec<String>,
    conflicts_with: Vec<String>,
    requires: Vec<String>,
    hidden: bool,
}

fn expand_decl(decl: Decl) -> Result<proc_macro2::TokenStream> {
    let mut spec = CommandSpec::default();

    for field in &decl.body.fields {
        let key = field.key.to_string();
        match key.as_str() {
            "name" => spec.name = Some(expect_string_value(&field.value)?),
            "summary" => spec.summary = expect_string_value(&field.value)?,
            "usage" => spec.usage = expect_string_value(&field.value)?,
            "aliases" => spec.aliases = expect_string_array_value(&field.value)?,
            "version" => spec.version = Some(expect_string_value(&field.value)?),
            "hidden" => spec.hidden = expect_bool_value(&field.value)?,
            "description" => spec.description = expect_string_value(&field.value)?,
            "examples" => spec.examples = expect_string_array_value(&field.value)?,
            "args" => spec.args = expect_args_array_value(&field.value)?,
            other => {
                return Err(syn::Error::new(
                    field.key.span(),
                    format!("unknown field: {other}"),
                ))
            }
        }
    }

    let Some(name) = spec.name.clone() else {
        return Err(syn::Error::new(
            decl.func_ident.span(),
            "missing required field: name",
        ));
    };

    // Default version to Cargo package version if available.
    let version = spec
        .version
        .clone()
        .or_else(|| std::env::var("CARGO_PKG_VERSION").ok())
        .unwrap_or_default();

    // Build metadata payload for embedding.
    let cmd_meta = wacli_metadata::CommandMeta {
        name: name.clone(),
        summary: spec.summary.clone(),
        usage: spec.usage.clone(),
        aliases: spec.aliases.clone(),
        version: version.clone(),
        hidden: spec.hidden,
        description: spec.description.clone(),
        examples: spec.examples.clone(),
        args: spec
            .args
            .iter()
            .map(|a| wacli_metadata::ArgDef {
                name: a.name.clone().unwrap_or_default(),
                short: a.short.clone(),
                long: a.long.clone(),
                help: a.help.clone(),
                required: a.required,
                default_value: a.default_value.clone(),
                value_name: a.value_name.clone(),
                takes_value: infer_takes_value(a),
            })
            .collect(),
    };

    let cmd_schema = wacli_metadata::CommandSchema {
        name: name.clone(),
        summary: spec.summary.clone(),
        usage: spec.usage.clone(),
        aliases: spec.aliases.clone(),
        version: version.clone(),
        hidden: spec.hidden,
        description: spec.description.clone(),
        examples: spec.examples.clone(),
        args: spec
            .args
            .iter()
            .map(|a| wacli_metadata::ArgSchema {
                name: a.name.clone().unwrap_or_default(),
                short: a.short.clone(),
                long: a.long.clone(),
                help: a.help.clone(),
                required: a.required,
                default_value: a.default_value.clone(),
                env: a.env.clone(),
                value_name: a.value_name.clone(),
                takes_value: infer_takes_value(a),
                multiple: a.multiple.unwrap_or(true),
                value_type: a.value_type.clone(),
                possible_values: a.possible_values.clone(),
                conflicts_with: a.conflicts_with.clone(),
                requires: a.requires.clone(),
                hidden: a.hidden,
            })
            .collect(),
    };

    let payload = wacli_metadata::CommandMetadataV1::new(cmd_meta.clone(), Some(cmd_schema));
    let bytes = payload.to_json_bytes();
    let bytes_len = bytes.len();

    let bytes_lit = LitByteStr::new(&bytes, proc_macro2::Span::call_site());

    let func_ident = decl.func_ident;
    let section_ident = Ident::new(
        &format!("__WACLI_COMMAND_METADATA_{}", func_ident),
        proc_macro2::Span::call_site(),
    );

    // Generate runtime CommandMeta construction as normal Rust allocations.
    let summary_expr = lit_or_empty(&spec.summary);
    let usage_expr = lit_or_empty(&spec.usage);
    let description_expr = lit_or_empty(&spec.description);
    let version_expr = LitStr::new(&version, proc_macro2::Span::call_site());
    let name_expr = LitStr::new(&name, proc_macro2::Span::call_site());

    let aliases_expr = vec_expr(&spec.aliases);
    let examples_expr = vec_expr(&spec.examples);
    let args_expr = meta_args_expr(&spec.args);

    let section_name = LitStr::new(
        wacli_metadata::COMMAND_METADATA_SECTION,
        proc_macro2::Span::call_site(),
    );
    let hidden_tokens = if spec.hidden {
        quote!(true)
    } else {
        quote!(false)
    };

    Ok(quote! {
        #[doc(hidden)]
        #[used]
        #[unsafe(link_section = #section_name)]
        pub static #section_ident: [u8; #bytes_len] = *#bytes_lit;

        pub fn #func_ident() -> ::wacli_cdk::CommandMeta {
            ::wacli_cdk::CommandMeta {
                name: (#name_expr).to_string(),
                summary: (#summary_expr).to_string(),
                usage: (#usage_expr).to_string(),
                aliases: #aliases_expr,
                version: (#version_expr).to_string(),
                hidden: #hidden_tokens,
                description: (#description_expr).to_string(),
                examples: #examples_expr,
                args: #args_expr,
            }
        }
    })
}

fn infer_takes_value(a: &ArgSpec) -> bool {
    if let Some(v) = a.takes_value {
        return v;
    }
    let positional = a.short.is_none() && a.long.is_none();
    if positional {
        return true;
    }
    a.value_name.is_some()
        || a.default_value.is_some()
        || a.env.is_some()
        || a.value_type.is_some()
        || !a.possible_values.is_empty()
}

fn lit_or_empty(s: &str) -> LitStr {
    LitStr::new(s, proc_macro2::Span::call_site())
}

fn vec_expr(items: &[String]) -> proc_macro2::TokenStream {
    let lits: Vec<LitStr> = items
        .iter()
        .map(|s| LitStr::new(s, proc_macro2::Span::call_site()))
        .collect();
    quote! { vec![ #( (#lits).to_string() ),* ] }
}

fn meta_args_expr(args: &[ArgSpec]) -> proc_macro2::TokenStream {
    let entries: Vec<proc_macro2::TokenStream> = args
        .iter()
        .map(|a| {
            let name = LitStr::new(a.name.as_deref().unwrap_or_default(), proc_macro2::Span::call_site());
            let help = LitStr::new(&a.help, proc_macro2::Span::call_site());
            let short = opt_string_expr(a.short.as_deref());
            let long = opt_string_expr(a.long.as_deref());
            let default_value = opt_string_expr(a.default_value.as_deref());
            let value_name = opt_string_expr(a.value_name.as_deref());
            let required = a.required;
            let takes_value = infer_takes_value(a);

            quote! {
                ::wacli_cdk::ArgDef {
                    name: (#name).to_string(),
                    short: #short,
                    long: #long,
                    help: (#help).to_string(),
                    required: #required,
                    default_value: #default_value,
                    value_name: #value_name,
                    takes_value: #takes_value,
                }
            }
        })
        .collect();

    quote! { vec![ #(#entries),* ] }
}

fn opt_string_expr(v: Option<&str>) -> proc_macro2::TokenStream {
    match v {
        Some(s) => {
            let lit = LitStr::new(s, proc_macro2::Span::call_site());
            quote! { Some((#lit).to_string()) }
        }
        None => quote! { None },
    }
}

fn expect_string_value(v: &Value) -> Result<String> {
    match v {
        Value::Str(s) => Ok(s.value()),
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected string literal",
        )),
    }
}

fn expect_bool_value(v: &Value) -> Result<bool> {
    match v {
        Value::Bool(b) => Ok(b.value()),
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected boolean literal",
        )),
    }
}

fn expect_string_array_value(v: &Value) -> Result<Vec<String>> {
    match v {
        Value::StrArray(items) => Ok(items.iter().map(|s| s.value()).collect()),
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected [\"a\", \"b\"]",
        )),
    }
}

fn expect_args_array_value(v: &Value) -> Result<Vec<ArgSpec>> {
    match v {
        Value::ArgsArray(items) => items.iter().map(arg_from_object).collect(),
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected args: [ { ... }, { ... } ]",
        )),
    }
}

fn arg_from_object(obj: &ArgObject) -> Result<ArgSpec> {
    let mut a = ArgSpec::default();

    for field in &obj.fields {
        let key = field.key.to_string();
        match key.as_str() {
            "name" => a.name = Some(expect_string_value(&field.value)?),
            "short" => a.short = Some(expect_string_value(&field.value)?),
            "long" => a.long = Some(expect_string_value(&field.value)?),
            "help" => a.help = expect_string_value(&field.value)?,
            "required" => a.required = expect_bool_value(&field.value)?,
            "default_value" => a.default_value = Some(expect_string_value(&field.value)?),
            "env" => a.env = Some(expect_string_value(&field.value)?),
            "value_name" => a.value_name = Some(expect_string_value(&field.value)?),
            "takes_value" => a.takes_value = Some(expect_bool_value(&field.value)?),
            "multiple" => a.multiple = Some(expect_bool_value(&field.value)?),
            "value_type" => a.value_type = Some(expect_string_value(&field.value)?),
            "possible_values" => a.possible_values = expect_string_array_value(&field.value)?,
            "conflicts_with" => a.conflicts_with = expect_string_array_value(&field.value)?,
            "requires" => a.requires = expect_string_array_value(&field.value)?,
            "hidden" => a.hidden = expect_bool_value(&field.value)?,
            other => {
                return Err(syn::Error::new(
                    field.key.span(),
                    format!("unknown arg field: {other}"),
                ))
            }
        }
    }

    if a.name.is_none() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "arg is missing required field: name",
        ));
    }

    // Normalize short/long like the runtime builder.
    if let Some(s) = a.short.clone() {
        let trimmed = s.trim().to_string();
        a.short = Some(if trimmed.starts_with('-') {
            trimmed
        } else {
            format!("-{trimmed}")
        });
    }
    if let Some(s) = a.long.clone() {
        let trimmed = s.trim().to_string();
        a.long = Some(if trimmed.starts_with("--") {
            trimmed
        } else if trimmed.starts_with('-') {
            trimmed
        } else {
            format!("--{trimmed}")
        });
    }

    Ok(a)
}
