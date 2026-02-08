//! Minimal clap-like argument parsing and help rendering.
//!
//! This crate is intentionally small and dependency-free so it can be reused by:
//! - `wacli-core` (to provide consistent `--help/--version` + validation even if plugins don't)
//! - `wacli-cdk` (to provide the same behavior for plugin-side parsing)

pub mod args {
    use std::borrow::Cow;
    use std::collections::{HashMap, HashSet};

    /// Parsed arguments and values.
    ///
    /// This is intentionally minimal (clap-like features are built on top).
    #[derive(Debug, Clone, Default)]
    pub struct Matches<'a> {
        values: HashMap<String, Vec<Cow<'a, str>>>,
        present: HashSet<String>,
        explicit: HashSet<String>,
        rest: Vec<&'a str>,
    }

    impl<'a> Matches<'a> {
        /// Get the last value for an argument (positional or value-taking flag).
        pub fn get(&self, name: &str) -> Option<&str> {
            self.values
                .get(name)
                .and_then(|v| v.last().map(|s| s.as_ref()))
        }

        /// Get all values for an argument (if it occurs multiple times).
        pub fn get_all(&self, name: &str) -> Option<&[Cow<'a, str>]> {
            self.values.get(name).map(|v| v.as_slice())
        }

        /// Whether an argument was present (boolean flag) or has a value.
        pub fn is_present(&self, name: &str) -> bool {
            self.present.contains(name) || self.values.contains_key(name)
        }

        /// Whether an argument was explicitly provided in argv.
        ///
        /// This does not include values sourced from env/default.
        pub fn is_explicit(&self, name: &str) -> bool {
            self.explicit.contains(name)
        }

        /// Extra positional arguments not covered by declared positional arg defs.
        pub fn rest(&self) -> &[&'a str] {
            self.rest.as_slice()
        }
    }

    impl<'a> Matches<'a> {
        pub(crate) fn push_present(&mut self, name: String) {
            self.present.insert(name);
        }

        pub(crate) fn push_value(&mut self, name: String, value: Cow<'a, str>) {
            self.values.entry(name).or_default().push(value);
        }

        pub(crate) fn push_explicit(&mut self, name: String) {
            self.explicit.insert(name);
        }

        pub(crate) fn push_rest(&mut self, value: &'a str) {
            self.rest.push(value);
        }

        pub(crate) fn has_value_key(&self, name: &str) -> bool {
            self.values.contains_key(name)
        }
    }

    /// Declare which flags take a value in the *next* argument (e.g. `--output out.txt`).
    ///
    /// Without a schema, parsing cannot reliably distinguish between:
    /// - a boolean flag followed by a positional (`--verbose file.txt`)
    /// - a value flag followed by its value (`--output out.txt`)
    ///
    /// `Schema` lets you declare value-taking flags so helpers like
    /// `positional_args_with_schema` can skip those values correctly.
    #[derive(Debug, Clone, Default)]
    pub struct Schema {
        value_flags: Vec<String>,
    }

    impl Schema {
        /// Create an empty schema.
        pub fn new() -> Self {
            Self::default()
        }

        /// Declare a flag that takes a value (e.g. `--output`, `-o`).
        pub fn value_flag(mut self, name: impl Into<String>) -> Self {
            let name = name.into();
            if !self.value_flags.iter().any(|s| s == &name) {
                self.value_flags.push(name);
            }
            self
        }

        fn takes_value(&self, flag: &str) -> bool {
            self.value_flags.iter().any(|s| s == flag)
        }
    }

    /// Argument name collection for flag matching.
    pub trait FlagNames<'a> {
        type Iter: Iterator<Item = &'a str>;
        fn iter(self) -> Self::Iter;
    }

    impl<'a> FlagNames<'a> for &'a str {
        type Iter = std::iter::Once<&'a str>;

        fn iter(self) -> Self::Iter {
            std::iter::once(self)
        }
    }

    impl<'a> FlagNames<'a> for &'a [&'a str] {
        type Iter = std::iter::Copied<std::slice::Iter<'a, &'a str>>;

        fn iter(self) -> Self::Iter {
            self.iter().copied()
        }
    }

    impl<'a, const N: usize> FlagNames<'a> for [&'a str; N] {
        type Iter = std::array::IntoIter<&'a str, N>;

        fn iter(self) -> Self::Iter {
            self.into_iter()
        }
    }

    /// Check if a flag like `--help` exists.
    ///
    /// Accepts a single name or multiple names via array/slice.
    /// Parsing stops at `--`.
    pub fn flag<'a, N>(argv: &[String], names: N) -> bool
    where
        N: FlagNames<'a>,
    {
        let names: Vec<&str> = names.iter().collect();
        for arg in argv {
            if arg == "--" {
                break;
            }
            if names.iter().any(|name| arg == name) {
                return true;
            }
        }
        false
    }

    /// Get a flag value like `--name=value` or `--name value`.
    ///
    /// Parsing stops at `--`.
    pub fn value<'a>(argv: &'a [String], name: &str) -> Option<&'a str> {
        let needle = format!("{name}=");
        for (idx, arg) in argv.iter().enumerate() {
            if arg == "--" {
                break;
            }
            if let Some(rest) = arg.strip_prefix(&needle) {
                return Some(rest);
            }
            if arg == name {
                return argv.get(idx + 1).map(|s| s.as_str());
            }
        }
        None
    }

    /// Get all positional arguments.
    ///
    /// Flags (arguments starting with `-`) are skipped. `--key=value` is treated
    /// as a single flag token and skipped.
    ///
    /// This function does *not* guess whether `--key value` is a value-taking
    /// flag or a boolean flag followed by a positional argument.
    ///
    /// If you want `--key value` to skip the value, use
    /// `positional_args_with_schema` and declare value-taking flags.
    ///
    /// Use `--` to stop flag parsing and treat everything after as positional.
    pub fn positional_args<'a>(argv: &'a [String]) -> Vec<&'a str> {
        positional_args_with_schema(argv, &Schema::default())
    }

    /// Get all positional arguments using a schema to skip values of declared flags.
    ///
    /// Any flag listed in `schema` is treated as taking a value in the next
    /// argument (e.g. `--output out.txt`), and that value is skipped.
    pub fn positional_args_with_schema<'a>(argv: &'a [String], schema: &Schema) -> Vec<&'a str> {
        let mut positionals = Vec::new();
        let mut i = 0;
        let mut after_separator = false;

        while i < argv.len() {
            let arg = &argv[i];
            if !after_separator {
                if arg == "--" {
                    after_separator = true;
                    i += 1;
                    continue;
                }
                if arg != "-" && arg.starts_with('-') {
                    if arg.contains('=') {
                        i += 1;
                        continue;
                    }
                    if schema.takes_value(arg) {
                        // Skip the flag itself.
                        i += 1;
                        // Skip the value if present. `--` remains a separator.
                        if i < argv.len() && argv[i] != "--" {
                            i += 1;
                        }
                        continue;
                    }
                    i += 1;
                    continue;
                }
            }

            positionals.push(arg.as_str());
            i += 1;
        }

        positionals
    }

    /// Get a positional argument by index.
    pub fn positional<'a>(argv: &'a [String], index: usize) -> Option<&'a str> {
        positional_args(argv).get(index).copied()
    }

    /// Get a positional argument by index using a schema.
    pub fn positional_with_schema<'a>(
        argv: &'a [String],
        index: usize,
        schema: &Schema,
    ) -> Option<&'a str> {
        positional_args_with_schema(argv, schema)
            .get(index)
            .copied()
    }

    /// Get the remaining arguments from a start index.
    pub fn rest<'a>(argv: &'a [String], start: usize) -> &'a [String] {
        if start >= argv.len() {
            &argv[argv.len()..]
        } else {
            &argv[start..]
        }
    }
}

pub mod claplike {
    use super::args::Matches;
    use std::borrow::Cow;
    use std::collections::{HashMap, HashSet};

    const BUILTIN_HELP_NAME: &str = "__wacli_help";
    const BUILTIN_VERSION_NAME: &str = "__wacli_version";

    pub trait ArgDefLike {
        fn name(&self) -> &str;
        fn short(&self) -> Option<&str>;
        fn long(&self) -> Option<&str>;
        fn help(&self) -> &str;
        fn required(&self) -> bool;
        fn default_value(&self) -> Option<&str>;
        fn env(&self) -> Option<&str> {
            None
        }
        fn value_name(&self) -> Option<&str>;
        fn takes_value(&self) -> bool;
        /// Whether the argument may be specified multiple times.
        ///
        /// Default is `true` to preserve existing behavior (the last value wins for `get()`).
        fn multiple(&self) -> bool {
            true
        }
        fn value_type(&self) -> Option<&str> {
            None
        }
        fn possible_values(&self) -> &[String] {
            &[]
        }
        fn conflicts_with(&self) -> &[String] {
            &[]
        }
        fn requires(&self) -> &[String] {
            &[]
        }
        fn hidden(&self) -> bool {
            false
        }
    }

    pub trait CommandMetaLike {
        type ArgDef: ArgDefLike;

        fn name(&self) -> &str;
        fn summary(&self) -> &str;
        fn usage(&self) -> &str;
        fn aliases(&self) -> &[String];
        fn version(&self) -> &str;
        fn hidden(&self) -> bool;
        fn description(&self) -> &str;
        fn examples(&self) -> &[String];
        fn args(&self) -> &[Self::ArgDef];
    }

    #[derive(Debug, Clone)]
    pub enum ParseError {
        InvalidArgs(String),
        Failed(String),
    }

    impl ParseError {
        pub fn message(&self) -> &str {
            match self {
                Self::InvalidArgs(msg) | Self::Failed(msg) => msg.as_str(),
            }
        }
    }

    impl From<String> for ParseError {
        fn from(msg: String) -> Self {
            Self::InvalidArgs(msg)
        }
    }

    pub type ParseResult<T> = Result<T, ParseError>;

    #[derive(Debug, Clone)]
    pub enum ParseOutcome<'a> {
        Matches(Matches<'a>),
        Help(String),
        Version(String),
    }

    #[derive(Debug, Clone)]
    struct ArgInfo {
        name: String,
        short: Option<String>,
        long: Option<String>,
        takes_value: bool,
        default_value: Option<String>,
    }

    #[derive(Debug, Clone)]
    struct BuiltinArgDef {
        name: &'static str,
        short: &'static str,
        long: &'static str,
        help: &'static str,
    }

    impl ArgDefLike for BuiltinArgDef {
        fn name(&self) -> &str {
            self.name
        }

        fn short(&self) -> Option<&str> {
            Some(self.short)
        }

        fn long(&self) -> Option<&str> {
            Some(self.long)
        }

        fn help(&self) -> &str {
            self.help
        }

        fn required(&self) -> bool {
            false
        }

        fn default_value(&self) -> Option<&str> {
            None
        }

        fn env(&self) -> Option<&str> {
            None
        }

        fn value_name(&self) -> Option<&str> {
            None
        }

        fn takes_value(&self) -> bool {
            false
        }

        fn multiple(&self) -> bool {
            true
        }

        fn value_type(&self) -> Option<&str> {
            None
        }

        fn possible_values(&self) -> &[String] {
            &[]
        }

        fn conflicts_with(&self) -> &[String] {
            &[]
        }

        fn requires(&self) -> &[String] {
            &[]
        }

        fn hidden(&self) -> bool {
            false
        }
    }

    #[derive(Debug, Clone)]
    enum ArgDefRef<'a, A> {
        User(&'a A),
        Builtin(BuiltinArgDef),
    }

    impl<'a, A: ArgDefLike> ArgDefLike for ArgDefRef<'a, A> {
        fn name(&self) -> &str {
            match self {
                Self::User(a) => a.name(),
                Self::Builtin(a) => a.name(),
            }
        }

        fn short(&self) -> Option<&str> {
            match self {
                Self::User(a) => a.short(),
                Self::Builtin(a) => a.short(),
            }
        }

        fn long(&self) -> Option<&str> {
            match self {
                Self::User(a) => a.long(),
                Self::Builtin(a) => a.long(),
            }
        }

        fn help(&self) -> &str {
            match self {
                Self::User(a) => a.help(),
                Self::Builtin(a) => a.help(),
            }
        }

        fn required(&self) -> bool {
            match self {
                Self::User(a) => a.required(),
                Self::Builtin(a) => a.required(),
            }
        }

        fn default_value(&self) -> Option<&str> {
            match self {
                Self::User(a) => a.default_value(),
                Self::Builtin(a) => a.default_value(),
            }
        }

        fn env(&self) -> Option<&str> {
            match self {
                Self::User(a) => a.env(),
                Self::Builtin(a) => a.env(),
            }
        }

        fn value_name(&self) -> Option<&str> {
            match self {
                Self::User(a) => a.value_name(),
                Self::Builtin(a) => a.value_name(),
            }
        }

        fn takes_value(&self) -> bool {
            match self {
                Self::User(a) => a.takes_value(),
                Self::Builtin(a) => a.takes_value(),
            }
        }

        fn multiple(&self) -> bool {
            match self {
                Self::User(a) => a.multiple(),
                Self::Builtin(a) => a.multiple(),
            }
        }

        fn value_type(&self) -> Option<&str> {
            match self {
                Self::User(a) => a.value_type(),
                Self::Builtin(a) => a.value_type(),
            }
        }

        fn possible_values(&self) -> &[String] {
            match self {
                Self::User(a) => a.possible_values(),
                Self::Builtin(a) => a.possible_values(),
            }
        }

        fn conflicts_with(&self) -> &[String] {
            match self {
                Self::User(a) => a.conflicts_with(),
                Self::Builtin(a) => a.conflicts_with(),
            }
        }

        fn requires(&self) -> &[String] {
            match self {
                Self::User(a) => a.requires(),
                Self::Builtin(a) => a.requires(),
            }
        }

        fn hidden(&self) -> bool {
            match self {
                Self::User(a) => a.hidden(),
                Self::Builtin(a) => a.hidden(),
            }
        }
    }

    fn normalize_short(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.starts_with('-') {
            trimmed.to_string()
        } else {
            format!("-{trimmed}")
        }
    }

    fn normalize_long(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.starts_with("--") {
            trimmed.to_string()
        } else if trimmed.starts_with('-') {
            trimmed.to_string()
        } else {
            format!("--{trimmed}")
        }
    }

    fn build_arg_info(def: &dyn ArgDefLike) -> ArgInfo {
        let short = def.short().map(normalize_short);
        let long = def.long().map(normalize_long);
        ArgInfo {
            name: def.name().to_string(),
            short,
            long,
            takes_value: def.takes_value(),
            default_value: def.default_value().map(|s| s.to_string()),
        }
    }

    fn has_flag<A: ArgDefLike>(defs: &[A], short: &str, long: &str) -> bool {
        let short = normalize_short(short);
        let long = normalize_long(long);
        defs.iter().any(|d| {
            d.short()
                .map(normalize_short)
                .is_some_and(|s| s == short)
                || d.long()
                    .map(normalize_long)
                    .is_some_and(|l| l == long)
        })
    }

    fn builtin_help_def() -> BuiltinArgDef {
        BuiltinArgDef {
            name: BUILTIN_HELP_NAME,
            short: "-h",
            long: "--help",
            help: "Show help information",
        }
    }

    fn builtin_version_def() -> BuiltinArgDef {
        BuiltinArgDef {
            name: BUILTIN_VERSION_NAME,
            short: "-V",
            long: "--version",
            help: "Show version information",
        }
    }

    fn schema_defs<'a, M: CommandMetaLike>(
        meta: &'a M,
    ) -> Vec<ArgDefRef<'a, M::ArgDef>> {
        let defs = meta.args();
        let mut out: Vec<ArgDefRef<'a, M::ArgDef>> = defs.iter().map(ArgDefRef::User).collect();

        if !has_flag(defs, "-h", "--help") {
            out.push(ArgDefRef::Builtin(builtin_help_def()));
        }
        if !has_flag(defs, "-V", "--version") {
            out.push(ArgDefRef::Builtin(builtin_version_def()));
        }

        out
    }

    fn format_value_name(def: &dyn ArgDefLike) -> String {
        def.value_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| def.name().to_ascii_uppercase())
    }

    fn format_arg_left(def: &dyn ArgDefLike) -> String {
        if def.short().is_none() && def.long().is_none() {
            let n = format_value_name(def);
            if def.required() {
                format!("<{n}>")
            } else {
                format!("[{n}]")
            }
        } else {
            let mut names: Vec<String> = Vec::new();
            if let Some(s) = def.short() {
                names.push(normalize_short(s));
            }
            if let Some(l) = def.long() {
                names.push(normalize_long(l));
            }
            let mut out = names.join(", ");
            if def.takes_value() {
                let n = format_value_name(def);
                out.push_str(&format!(" <{n}>"));
            }
            out
        }
    }

    fn format_arg_help(def: &dyn ArgDefLike) -> String {
        let mut out = def.help().trim().to_string();
        if def.required() && !(def.short().is_none() && def.long().is_none()) {
            if out.is_empty() {
                out.push_str("required");
            } else {
                out.push_str(" (required)");
            }
        }
        if let Some(default_value) = def.default_value() {
            if out.is_empty() {
                out.push_str(&format!("[default: {default_value}]"));
            } else {
                out.push_str(&format!(" [default: {default_value}]"));
            }
        }
        out
    }

    /// Render a help message based on `CommandMeta`.
    pub fn help<M: CommandMetaLike>(meta: &M) -> String {
        let defs = schema_defs(meta);

        let mut out = String::new();
        if meta.summary().trim().is_empty() {
            out.push_str(meta.name());
            out.push('\n');
        } else {
            out.push_str(&format!("{} - {}\n", meta.name(), meta.summary().trim()));
        }

        if meta.usage().trim().is_empty() {
            out.push_str(&format!("\nUsage: {}\n", meta.name()));
        } else {
            out.push_str(&format!("\nUsage: {}\n", meta.usage().trim()));
        }

        if !meta.description().trim().is_empty() {
            out.push('\n');
            out.push_str(meta.description().trim_end());
            out.push('\n');
        }

        let mut options: Vec<&dyn ArgDefLike> = Vec::new();
        let mut positionals: Vec<&dyn ArgDefLike> = Vec::new();
        for def in &defs {
            let as_dyn: &dyn ArgDefLike = def;
            if def.hidden() {
                continue;
            }
            if def.short().is_none() && def.long().is_none() {
                positionals.push(as_dyn);
            } else {
                options.push(as_dyn);
            }
        }

        if !positionals.is_empty() {
            out.push_str("\nArguments:\n");
            let rows: Vec<(String, String)> = positionals
                .iter()
                .map(|d| (format_arg_left(*d), format_arg_help(*d)))
                .collect();
            let width = rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
            for (left, help) in rows {
                if help.is_empty() {
                    out.push_str(&format!("  {}\n", left));
                } else {
                    out.push_str(&format!("  {:width$}  {}\n", left, help, width = width));
                }
            }
        }

        if !options.is_empty() {
            out.push_str("\nOptions:\n");
            let rows: Vec<(String, String)> = options
                .iter()
                .map(|d| (format_arg_left(*d), format_arg_help(*d)))
                .collect();
            let width = rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
            for (left, help) in rows {
                if help.is_empty() {
                    out.push_str(&format!("  {}\n", left));
                } else {
                    out.push_str(&format!("  {:width$}  {}\n", left, help, width = width));
                }
            }
        }

        if !meta.examples().is_empty() {
            out.push_str("\nExamples:\n");
            for ex in meta.examples() {
                if ex.trim().is_empty() {
                    continue;
                }
                out.push_str(&format!("  {}\n", ex.trim_end()));
            }
        }

        out
    }

    /// Render a version message based on `CommandMeta`.
    pub fn version<M: CommandMetaLike>(meta: &M) -> String {
        if meta.version().trim().is_empty() {
            format!("{}\n", meta.name())
        } else {
            format!("{} {}\n", meta.name(), meta.version().trim())
        }
    }

    fn env_lookup<'e>(env: &'e [(String, String)], key: &str) -> Option<&'e str> {
        env.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn arg_display_name(def: &dyn ArgDefLike) -> String {
        def.long()
            .map(normalize_long)
            .or_else(|| def.short().map(normalize_short))
            .unwrap_or_else(|| def.name().to_string())
    }

    fn validate_relations(defs: &[&dyn ArgDefLike]) -> ParseResult<()> {
        let names: HashSet<&str> = defs.iter().map(|d| d.name()).collect();
        for def in defs {
            for other in def.conflicts_with() {
                if other.trim().is_empty() {
                    continue;
                }
                if !names.contains(other.as_str()) {
                    return Err(ParseError::Failed(format!(
                        "schema error: '{}' conflicts-with unknown arg '{}'",
                        def.name(),
                        other
                    )));
                }
            }
            for other in def.requires() {
                if other.trim().is_empty() {
                    continue;
                }
                if !names.contains(other.as_str()) {
                    return Err(ParseError::Failed(format!(
                        "schema error: '{}' requires unknown arg '{}'",
                        def.name(),
                        other
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_matches(
        defs: &[&dyn ArgDefLike],
        m: &Matches<'_>,
    ) -> ParseResult<()> {
        let by_name: HashMap<&str, &dyn ArgDefLike> =
            defs.iter().copied().map(|d| (d.name(), d)).collect();

        for &def in defs {
            let name = def.name();

            if !def.multiple() {
                if let Some(values) = m.get_all(name) {
                    if values.len() > 1 {
                        return Err(ParseError::InvalidArgs(format!(
                            "argument '{}' cannot be used multiple times",
                            arg_display_name(def)
                        )));
                    }
                }
            }

            if !def.possible_values().is_empty() {
                if let Some(values) = m.get_all(name) {
                    for v in values {
                        let v = v.as_ref();
                        if !def.possible_values().iter().any(|p| p == v) {
                            return Err(ParseError::InvalidArgs(format!(
                                "invalid value '{}' for '{}'. possible values: {}",
                                v,
                                arg_display_name(def),
                                def.possible_values().join(", ")
                            )));
                        }
                    }
                }
            }

            if m.is_explicit(name) {
                for other in def.conflicts_with() {
                    if other.trim().is_empty() {
                        continue;
                    }
                    if m.is_explicit(other) {
                        let other_display = by_name
                            .get(other.as_str())
                            .map(|d| arg_display_name(*d))
                            .unwrap_or_else(|| other.to_string());
                        return Err(ParseError::InvalidArgs(format!(
                            "argument '{}' cannot be used with '{}'",
                            arg_display_name(def),
                            other_display
                        )));
                    }
                }
                for other in def.requires() {
                    if other.trim().is_empty() {
                        continue;
                    }
                    if !m.is_explicit(other) {
                        let other_display = by_name
                            .get(other.as_str())
                            .map(|d| arg_display_name(*d))
                            .unwrap_or_else(|| other.to_string());
                        return Err(ParseError::InvalidArgs(format!(
                            "argument '{}' requires '{}'",
                            arg_display_name(def),
                            other_display
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Parse `argv` based on the `meta.args` schema.
    ///
    /// This implements a minimal clap-like behavior:
    /// - `-h/--help` => `ParseOutcome::Help`
    /// - `-V/--version` => `ParseOutcome::Version`
    /// - required argument checks
    /// - unknown flag detection
    pub fn parse<'a, M: CommandMetaLike>(
        meta: &M,
        argv: &'a [String],
    ) -> ParseResult<ParseOutcome<'a>> {
        parse_with_env(meta, argv, &[])
    }

    /// Parse `argv` using `env` as a value source for args that declare `env`.
    ///
    /// Value precedence is:
    /// 1) CLI argv
    /// 2) env
    /// 3) default-value
    pub fn parse_with_env<'a, M: CommandMetaLike>(
        meta: &M,
        argv: &'a [String],
        env: &[(String, String)],
    ) -> ParseResult<ParseOutcome<'a>> {
        let defs = schema_defs(meta);
        let defs_dyn: Vec<&dyn ArgDefLike> = defs.iter().map(|d| d as &dyn ArgDefLike).collect();
        validate_relations(&defs_dyn)?;

        let infos: Vec<ArgInfo> = defs_dyn.iter().map(|d| build_arg_info(*d)).collect();
        let mut long_map: HashMap<String, usize> = HashMap::new();
        let mut short_map: HashMap<String, usize> = HashMap::new();
        let mut positional_defs: Vec<usize> = Vec::new();

        for (idx, info) in infos.iter().enumerate() {
            if info.short.is_none() && info.long.is_none() {
                positional_defs.push(idx);
                continue;
            }

            if let Some(short) = &info.short {
                if let Some(prev) = short_map.insert(short.clone(), idx) {
                    if infos[prev].name != info.name {
                        return Err(ParseError::Failed(format!(
                            "arg definition conflict: {short} maps to both '{}' and '{}'",
                            infos[prev].name, info.name
                        )));
                    }
                }
            }
            if let Some(long) = &info.long {
                if let Some(prev) = long_map.insert(long.clone(), idx) {
                    if infos[prev].name != info.name {
                        return Err(ParseError::Failed(format!(
                            "arg definition conflict: {long} maps to both '{}' and '{}'",
                            infos[prev].name, info.name
                        )));
                    }
                }
            }
        }

        let mut m = Matches::default();
        let mut positionals: Vec<&'a str> = Vec::new();
        let mut parse_error: Option<ParseError> = None;

        let mut i = 0usize;
        let mut after_separator = false;
        while i < argv.len() {
            let arg = argv[i].as_str();

            if !after_separator && arg == "--" {
                after_separator = true;
                i += 1;
                continue;
            }

            if !after_separator && arg.starts_with("--") && arg != "--" {
                // --key=value
                if let Some((flag, value)) = arg.split_once('=') {
                    if let Some(&idx) = long_map.get(flag) {
                        let info = &infos[idx];
                        if !info.takes_value {
                            if parse_error.is_none() {
                                parse_error = Some(ParseError::InvalidArgs(format!(
                                    "flag does not take a value: {flag}"
                                )));
                            }
                            i += 1;
                            continue;
                        }
                        m.push_explicit(info.name.clone());
                        m.push_value(info.name.clone(), Cow::Borrowed(value));
                        i += 1;
                        continue;
                    }
                    if parse_error.is_none() {
                        parse_error =
                            Some(ParseError::InvalidArgs(format!("unknown flag: {flag}")));
                    }
                    i += 1;
                    continue;
                }

                // --key value? (only if declared)
                if let Some(&idx) = long_map.get(arg) {
                    let info = &infos[idx];
                    if info.takes_value {
                        let Some(value) = argv.get(i + 1) else {
                            if parse_error.is_none() {
                                parse_error = Some(ParseError::InvalidArgs(format!(
                                    "missing value for {arg}"
                                )));
                            }
                            break;
                        };
                        m.push_explicit(info.name.clone());
                        m.push_value(info.name.clone(), Cow::Borrowed(value.as_str()));
                        i += 2;
                    } else {
                        m.push_explicit(info.name.clone());
                        m.push_present(info.name.clone());
                        i += 1;
                    }
                    continue;
                }

                if parse_error.is_none() {
                    parse_error =
                        Some(ParseError::InvalidArgs(format!("unknown flag: {arg}")));
                }
                i += 1;
                continue;
            }

            if !after_separator && arg.starts_with('-') && arg != "-" {
                // Short flags: -v, -o value, -abc, -ofile
                if arg.len() == 2 {
                    if let Some(&idx) = short_map.get(arg) {
                        let info = &infos[idx];
                        if info.takes_value {
                            let Some(value) = argv.get(i + 1) else {
                                if parse_error.is_none() {
                                    parse_error = Some(ParseError::InvalidArgs(format!(
                                        "missing value for {arg}"
                                    )));
                                }
                                break;
                            };
                            m.push_explicit(info.name.clone());
                            m.push_value(info.name.clone(), Cow::Borrowed(value.as_str()));
                            i += 2;
                        } else {
                            m.push_explicit(info.name.clone());
                            m.push_present(info.name.clone());
                            i += 1;
                        }
                        continue;
                    }
                    if parse_error.is_none() {
                        parse_error =
                            Some(ParseError::InvalidArgs(format!("unknown flag: {arg}")));
                    }
                    i += 1;
                    continue;
                }

                // Combined short flags.
                let bytes = arg.as_bytes();
                if !bytes.is_ascii() {
                    if parse_error.is_none() {
                        parse_error = Some(ParseError::InvalidArgs(format!(
                            "invalid short flags: {arg}"
                        )));
                    }
                    i += 1;
                    continue;
                }

                let mut k = 1usize;
                let mut consumed_next = false;
                while k < bytes.len() {
                    let c = bytes[k] as char;
                    let flag = format!("-{c}");
                    let Some(&idx) = short_map.get(&flag) else {
                        if parse_error.is_none() {
                            parse_error = Some(ParseError::InvalidArgs(format!(
                                "unknown flag: {flag}"
                            )));
                        }
                        k += 1;
                        continue;
                    };
                    let info = &infos[idx];
                    if info.takes_value {
                        let rest = &arg[k + 1..];
                        if !rest.is_empty() {
                            m.push_explicit(info.name.clone());
                            m.push_value(info.name.clone(), Cow::Borrowed(rest));
                        } else {
                            let Some(value) = argv.get(i + 1) else {
                                if parse_error.is_none() {
                                    parse_error = Some(ParseError::InvalidArgs(format!(
                                        "missing value for {flag}"
                                    )));
                                }
                                break;
                            };
                            m.push_explicit(info.name.clone());
                            m.push_value(info.name.clone(), Cow::Borrowed(value.as_str()));
                            consumed_next = true;
                        }
                        break;
                    } else {
                        m.push_explicit(info.name.clone());
                        m.push_present(info.name.clone());
                    }
                    k += 1;
                }

                i += if consumed_next { 2 } else { 1 };
                continue;
            }

            positionals.push(arg);
            i += 1;
        }

        // Assign positional args by declaration order.
        let mut pos_iter = positionals.into_iter();
        for &idx in &positional_defs {
            let info = &infos[idx];
            if let Some(v) = pos_iter.next() {
                m.push_explicit(info.name.clone());
                m.push_value(info.name.clone(), Cow::Borrowed(v));
            }
        }
        for v in pos_iter {
            m.push_rest(v);
        }

        // Apply env/defaults for missing value-taking args.
        for (idx, info) in infos.iter().enumerate() {
            if !info.takes_value || m.has_value_key(&info.name) {
                continue;
            }

            let def = defs_dyn[idx];
            if let Some(key) = def.env() {
                if let Some(v) = env_lookup(env, key) {
                    m.push_value(info.name.clone(), Cow::Owned(v.to_string()));
                    continue;
                }
            }

            if let Some(default_value) = info.default_value.clone() {
                m.push_value(info.name.clone(), Cow::Owned(default_value));
            }
        }

        // Built-in flags.
        if m.is_present(BUILTIN_HELP_NAME) {
            return Ok(ParseOutcome::Help(help(meta)));
        }
        if m.is_present(BUILTIN_VERSION_NAME) {
            return Ok(ParseOutcome::Version(version(meta)));
        }

        if let Some(err) = parse_error {
            return Err(err);
        }

        // Required checks.
        let mut missing: Vec<String> = Vec::new();
        for def in &defs {
            if !def.required() {
                continue;
            }
            if m.is_present(def.name()) {
                continue;
            }

            if def.short().is_none() && def.long().is_none() {
                missing.push(format!("<{}>", format_value_name(def)));
                continue;
            }

            let mut s = def
                .long()
                .map(normalize_long)
                .or_else(|| def.short().map(normalize_short))
                .unwrap_or_else(|| def.name().to_string());
            if def.takes_value() {
                s.push(' ');
                s.push('<');
                s.push_str(&format_value_name(def));
                s.push('>');
            }
            missing.push(s);
        }

        if !missing.is_empty() {
            if missing.len() == 1 {
                return Err(ParseError::InvalidArgs(format!(
                    "missing required argument: {}",
                    missing[0]
                )));
            }
            return Err(ParseError::InvalidArgs(format!(
                "missing required arguments: {}",
                missing.join(", ")
            )));
        }

        validate_matches(&defs_dyn, &m)?;

        Ok(ParseOutcome::Matches(m))
    }

    /// Validate `argv` based on the `meta.args` schema.
    ///
    /// This is equivalent to parsing and ignoring the results. `--help`/`--version`
    /// are treated as valid inputs.
    pub fn validate<M: CommandMetaLike>(meta: &M, argv: &[String]) -> ParseResult<()> {
        match parse(meta, argv)? {
            ParseOutcome::Matches(_) | ParseOutcome::Help(_) | ParseOutcome::Version(_) => Ok(()),
        }
    }

    /// Validate `argv` using `env` as a value source for args that declare `env`.
    pub fn validate_with_env<M: CommandMetaLike>(
        meta: &M,
        argv: &[String],
        env: &[(String, String)],
    ) -> ParseResult<()> {
        match parse_with_env(meta, argv, env)? {
            ParseOutcome::Matches(_) | ParseOutcome::Help(_) | ParseOutcome::Version(_) => Ok(()),
        }
    }

    /// Return the canonical command name for `raw`, matching either the command
    /// name itself or an alias.
    pub fn resolve_command_name<'a, M: CommandMetaLike>(
        metas: &'a [M],
        raw: &str,
    ) -> Option<&'a str> {
        if let Some(m) = metas.iter().find(|m| m.name() == raw) {
            return Some(m.name());
        }
        metas.iter()
            .find(|m| m.aliases().iter().any(|a| a == raw))
            .map(|m| m.name())
    }

    /// Detect invalid alias configuration (duplicate aliases or alias colliding
    /// with a command name).
    pub fn validate_aliases<M: CommandMetaLike>(metas: &[M]) -> ParseResult<()> {
        let mut names: HashSet<&str> = HashSet::new();
        for m in metas {
            if !m.name().trim().is_empty() {
                names.insert(m.name());
            }
        }

        let mut alias_map: HashMap<&str, &str> = HashMap::new();
        for m in metas {
            for alias in m.aliases() {
                let alias = alias.trim();
                if alias.is_empty() || alias == m.name() {
                    continue;
                }
                if names.contains(alias) {
                    return Err(ParseError::Failed(format!(
                        "alias conflict: '{alias}' is both a command name and an alias (command: {})",
                        m.name()
                    )));
                }
                if let Some(prev) = alias_map.insert(alias, m.name()) {
                    if prev != m.name() {
                        return Err(ParseError::Failed(format!(
                            "alias conflict: '{alias}' refers to both '{prev}' and '{}'",
                            m.name()
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::claplike;

    #[derive(Debug, Clone)]
    struct ArgDef {
        name: String,
        short: Option<String>,
        long: Option<String>,
        help: String,
        required: bool,
        default_value: Option<String>,
        env: Option<String>,
        value_name: Option<String>,
        takes_value: bool,
        multiple: bool,
        possible_values: Vec<String>,
        conflicts_with: Vec<String>,
        requires: Vec<String>,
        hidden: bool,
    }

    impl Default for ArgDef {
        fn default() -> Self {
            Self {
                name: String::new(),
                short: None,
                long: None,
                help: String::new(),
                required: false,
                default_value: None,
                env: None,
                value_name: None,
                takes_value: false,
                multiple: true,
                possible_values: Vec::new(),
                conflicts_with: Vec::new(),
                requires: Vec::new(),
                hidden: false,
            }
        }
    }

    impl claplike::ArgDefLike for ArgDef {
        fn name(&self) -> &str {
            &self.name
        }
        fn short(&self) -> Option<&str> {
            self.short.as_deref()
        }
        fn long(&self) -> Option<&str> {
            self.long.as_deref()
        }
        fn help(&self) -> &str {
            &self.help
        }
        fn required(&self) -> bool {
            self.required
        }
        fn default_value(&self) -> Option<&str> {
            self.default_value.as_deref()
        }
        fn env(&self) -> Option<&str> {
            self.env.as_deref()
        }
        fn value_name(&self) -> Option<&str> {
            self.value_name.as_deref()
        }
        fn takes_value(&self) -> bool {
            self.takes_value
        }
        fn multiple(&self) -> bool {
            self.multiple
        }
        fn possible_values(&self) -> &[String] {
            self.possible_values.as_slice()
        }
        fn conflicts_with(&self) -> &[String] {
            self.conflicts_with.as_slice()
        }
        fn requires(&self) -> &[String] {
            self.requires.as_slice()
        }
        fn hidden(&self) -> bool {
            self.hidden
        }
    }

    #[derive(Debug, Clone, Default)]
    struct Meta {
        name: String,
        summary: String,
        usage: String,
        aliases: Vec<String>,
        version: String,
        hidden: bool,
        description: String,
        examples: Vec<String>,
        args: Vec<ArgDef>,
    }

    impl claplike::CommandMetaLike for Meta {
        type ArgDef = ArgDef;

        fn name(&self) -> &str {
            &self.name
        }
        fn summary(&self) -> &str {
            &self.summary
        }
        fn usage(&self) -> &str {
            &self.usage
        }
        fn aliases(&self) -> &[String] {
            self.aliases.as_slice()
        }
        fn version(&self) -> &str {
            &self.version
        }
        fn hidden(&self) -> bool {
            self.hidden
        }
        fn description(&self) -> &str {
            &self.description
        }
        fn examples(&self) -> &[String] {
            self.examples.as_slice()
        }
        fn args(&self) -> &[Self::ArgDef] {
            self.args.as_slice()
        }
    }

    #[test]
    fn help_includes_builtins() {
        let meta = Meta {
            name: "show".to_string(),
            summary: "Show a file".to_string(),
            usage: "show [OPTIONS] <FILE>".to_string(),
            description: "Display a file to stdout.".to_string(),
            examples: vec!["show hello.txt".to_string()],
            args: vec![
                ArgDef {
                    name: "file".to_string(),
                    help: "File to show".to_string(),
                    required: true,
                    value_name: Some("FILE".to_string()),
                    takes_value: true,
                    ..Default::default()
                },
                ArgDef {
                    name: "verbose".to_string(),
                    short: Some("-v".to_string()),
                    long: Some("--verbose".to_string()),
                    help: "Verbose output".to_string(),
                    takes_value: false,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let text = claplike::help(&meta);
        assert!(text.contains("Usage: show [OPTIONS] <FILE>"));
        assert!(text.contains("Options:"));
        assert!(text.contains("--help"));
        assert!(text.contains("--version"));
        assert!(text.contains("--verbose"));
    }

    #[test]
    fn parse_supports_combined_short_flags_and_attached_value() {
        let meta = Meta {
            name: "show".to_string(),
            args: vec![
                ArgDef {
                    name: "verbose".to_string(),
                    short: Some("-v".to_string()),
                    help: "Verbose output".to_string(),
                    takes_value: false,
                    ..Default::default()
                },
                ArgDef {
                    name: "output".to_string(),
                    short: Some("-o".to_string()),
                    value_name: Some("FILE".to_string()),
                    help: "Output file".to_string(),
                    takes_value: true,
                    ..Default::default()
                },
                ArgDef {
                    name: "file".to_string(),
                    required: true,
                    value_name: Some("FILE".to_string()),
                    takes_value: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let argv = vec!["-voout.txt".to_string(), "in.txt".to_string()];
        let outcome = claplike::parse(&meta, &argv).unwrap();
        let claplike::ParseOutcome::Matches(m) = outcome else {
            panic!("expected Matches");
        };
        assert!(m.is_present("verbose"));
        assert_eq!(m.get("output"), Some("out.txt"));
        assert_eq!(m.get("file"), Some("in.txt"));
    }

    #[test]
    fn validate_aliases_rejects_conflicts() {
        let a = Meta {
            name: "alpha".to_string(),
            aliases: vec!["beta".to_string()],
            ..Default::default()
        };
        let b = Meta {
            name: "beta".to_string(),
            ..Default::default()
        };

        let err = claplike::validate_aliases(&[a, b]).unwrap_err();
        match err {
            claplike::ParseError::Failed(msg) => assert!(msg.contains("alias conflict")),
            other => panic!("expected Failed, got: {other:?}"),
        }
    }

    #[test]
    fn parse_with_env_respects_precedence() {
        let meta = Meta {
            name: "cmd".to_string(),
            args: vec![ArgDef {
                name: "format".to_string(),
                long: Some("--format".to_string()),
                value_name: Some("FMT".to_string()),
                takes_value: true,
                default_value: Some("plain".to_string()),
                env: Some("FORMAT".to_string()),
                possible_values: vec!["plain".to_string(), "json".to_string(), "xml".to_string()],
                multiple: true,
                ..Default::default()
            }],
            ..Default::default()
        };

        // env beats default
        let argv: Vec<String> = vec![];
        let env = vec![("FORMAT".to_string(), "json".to_string())];
        let claplike::ParseOutcome::Matches(m) = claplike::parse_with_env(&meta, &argv, &env).unwrap() else {
            panic!("expected Matches");
        };
        assert_eq!(m.get("format"), Some("json"));

        // default used when env absent
        let env: Vec<(String, String)> = vec![];
        let claplike::ParseOutcome::Matches(m) = claplike::parse_with_env(&meta, &argv, &env).unwrap() else {
            panic!("expected Matches");
        };
        assert_eq!(m.get("format"), Some("plain"));

        // argv beats env
        let argv = vec!["--format".to_string(), "xml".to_string()];
        let env = vec![("FORMAT".to_string(), "json".to_string())];
        let claplike::ParseOutcome::Matches(m) = claplike::parse_with_env(&meta, &argv, &env).unwrap() else {
            panic!("expected Matches");
        };
        assert_eq!(m.get("format"), Some("xml"));
    }

    #[test]
    fn validate_rejects_invalid_value() {
        let meta = Meta {
            name: "cmd".to_string(),
            args: vec![ArgDef {
                name: "format".to_string(),
                long: Some("--format".to_string()),
                value_name: Some("FMT".to_string()),
                takes_value: true,
                possible_values: vec!["plain".to_string(), "json".to_string()],
                multiple: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let argv = vec!["--format".to_string(), "xml".to_string()];
        let err = claplike::validate(&meta, &argv).unwrap_err();
        match err {
            claplike::ParseError::InvalidArgs(msg) => assert!(msg.contains("invalid value")),
            other => panic!("expected InvalidArgs, got: {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_conflicts_with() {
        let meta = Meta {
            name: "cmd".to_string(),
            args: vec![
                ArgDef {
                    name: "a".to_string(),
                    long: Some("--a".to_string()),
                    takes_value: false,
                    conflicts_with: vec!["b".to_string()],
                    ..Default::default()
                },
                ArgDef {
                    name: "b".to_string(),
                    long: Some("--b".to_string()),
                    takes_value: false,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let argv = vec!["--a".to_string(), "--b".to_string()];
        let err = claplike::validate(&meta, &argv).unwrap_err();
        match err {
            claplike::ParseError::InvalidArgs(msg) => assert!(msg.contains("cannot be used with")),
            other => panic!("expected InvalidArgs, got: {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_requires() {
        let meta = Meta {
            name: "cmd".to_string(),
            args: vec![
                ArgDef {
                    name: "a".to_string(),
                    long: Some("--a".to_string()),
                    takes_value: false,
                    requires: vec!["b".to_string()],
                    ..Default::default()
                },
                ArgDef {
                    name: "b".to_string(),
                    long: Some("--b".to_string()),
                    takes_value: false,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let argv = vec!["--a".to_string()];
        let err = claplike::validate(&meta, &argv).unwrap_err();
        match err {
            claplike::ParseError::InvalidArgs(msg) => assert!(msg.contains("requires")),
            other => panic!("expected InvalidArgs, got: {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_multiple_when_disabled() {
        let meta = Meta {
            name: "cmd".to_string(),
            args: vec![ArgDef {
                name: "out".to_string(),
                long: Some("--out".to_string()),
                value_name: Some("FILE".to_string()),
                takes_value: true,
                multiple: false,
                ..Default::default()
            }],
            ..Default::default()
        };
        let argv = vec![
            "--out".to_string(),
            "a.txt".to_string(),
            "--out".to_string(),
            "b.txt".to_string(),
        ];
        let err = claplike::validate(&meta, &argv).unwrap_err();
        match err {
            claplike::ParseError::InvalidArgs(msg) => assert!(msg.contains("multiple times")),
            other => panic!("expected InvalidArgs, got: {other:?}"),
        }
    }

    #[test]
    fn help_omits_hidden_args() {
        let meta = Meta {
            name: "cmd".to_string(),
            args: vec![
                ArgDef {
                    name: "visible".to_string(),
                    long: Some("--visible".to_string()),
                    help: "Visible".to_string(),
                    takes_value: false,
                    ..Default::default()
                },
                ArgDef {
                    name: "secret".to_string(),
                    long: Some("--secret".to_string()),
                    help: "Secret".to_string(),
                    takes_value: false,
                    hidden: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let text = claplike::help(&meta);
        assert!(text.contains("--visible"));
        assert!(!text.contains("--secret"));
    }
}
