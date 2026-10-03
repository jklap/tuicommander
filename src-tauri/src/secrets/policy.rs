//! Argv consent policy. Templates are user-created and never model-created.
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Template(Vec<String>);

impl Template {
    pub(crate) fn new(argv: Vec<String>) -> Result<Self, String> {
        if argv.is_empty()
            || argv[0].contains('{')
            || argv[0].contains(['*', '?', '[', ']'])
            || argv.get(1).is_some_and(|arg| arg.contains('{'))
        {
            return Err("A template must fix the program and its first subcommand".into());
        }
        if argv
            .iter()
            .any(|arg| (arg.contains('{') || arg.contains('}')) && arg != "{arg}")
        {
            return Err("Use a whole {arg} argument as a placeholder".into());
        }
        let program = argv[0].rsplit(['/', '\\']).next().unwrap_or("");
        let program = program.strip_suffix(".exe").unwrap_or(program);
        if argv.iter().any(|arg| arg == "{arg}")
            && !(program == "gh" && argv.get(1).is_some_and(|arg| arg == "api"))
        {
            return Err("This slice supports placeholders only after the fixed gh api command; use exact argv for other programs".into());
        }
        validate_argv(&argv)?;
        Ok(Self(argv))
    }

    pub(crate) fn matches(&self, argv: &[String]) -> bool {
        self.0.len() == argv.len()
            && self.0.iter().zip(argv).all(|(pattern, value)| {
                if pattern == "{arg}" {
                    !value.is_empty()
                        && !value.starts_with('-')
                        && value
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_./:@+=,-".contains(&b))
                } else {
                    pattern == value
                }
            })
    }
}

pub(crate) fn validate_argv(argv: &[String]) -> Result<(), String> {
    let Some(program) = argv.first().filter(|p| !p.is_empty()) else {
        return Err("argv must include a program".into());
    };
    if argv.len() > 128 || argv.iter().any(|a| a.contains('\0') || a.len() > 8192) {
        return Err("Invalid argv".into());
    }
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    if matches!(name, "env" | "printenv") {
        return Err("Environment dumpers are forbidden".into());
    }
    if name.ends_with(".bat") || name.ends_with(".cmd") {
        return Err("Implicit shell batch execution is forbidden".into());
    }
    if argv.iter().skip(1).any(|a| {
        let a = a.to_ascii_lowercase();
        a.starts_with("--eval")
            || a.starts_with("--command")
            || a.starts_with("-command")
            || a.starts_with("-encodedcommand")
            || a.starts_with("/c")
            || a.starts_with("/k")
            || (a.starts_with('-') && !a.starts_with("--") && a[1..].contains(['c', 'e', 'p']))
            || matches!(a.as_str(), "eval" | "-r")
    }) {
        return Err("Shell and interpreter command evaluation is forbidden".into());
    }
    Ok(())
}
