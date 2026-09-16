//! A recipe's parameters as a form: one row per parameter, and the
//! argument list the filled-in rows turn into. Shared by the run-with-
//! arguments overlay and the Slurm submit form.

use crate::model::{Parameter, Recipe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Single,
    /// `*name`: zero or more values.
    Star,
    /// `+name`: one or more values.
    Plus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub kind: ParamKind,
    /// The default as plain text, when it is a string literal. Expression
    /// defaults cannot be reproduced here, so they stay `None`.
    pub default: Option<String>,
    /// The default as written in the justfile, for display.
    pub default_text: Option<String>,
}

impl Param {
    fn from(parameter: &Parameter) -> Self {
        let kind = match parameter.kind.as_str() {
            "star" => ParamKind::Star,
            "plus" => ParamKind::Plus,
            _ => ParamKind::Single,
        };
        Self {
            name: parameter.name.clone(),
            kind,
            default: parameter
                .default
                .as_ref()
                .and_then(|d| d.as_str().map(str::to_owned)),
            default_text: parameter.default_text(),
        }
    }

    pub fn sigil(&self) -> &'static str {
        match self.kind {
            ParamKind::Single => "",
            ParamKind::Star => "*",
            ParamKind::Plus => "+",
        }
    }

    pub fn is_variadic(&self) -> bool {
        self.kind != ParamKind::Single
    }

    /// What an empty row means, shown beside it.
    pub fn hint(&self) -> String {
        let mut parts = Vec::new();
        match &self.default_text {
            Some(default) => parts.push(format!("default {default}")),
            None if self.kind == ParamKind::Star => parts.push("optional".to_owned()),
            None => parts.push("required".to_owned()),
        }
        match self.kind {
            ParamKind::Single => {}
            ParamKind::Star => parts.push("zero or more, space-separated".to_owned()),
            ParamKind::Plus => parts.push("one or more, space-separated".to_owned()),
        }
        parts.join(" · ")
    }
}

/// The values typed for one recipe's parameters, and which row the cursor
/// is on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParamForm {
    pub params: Vec<Param>,
    pub values: Vec<String>,
    pub cursor: usize,
}

impl ParamForm {
    pub fn from_recipe(recipe: &Recipe) -> Self {
        let params: Vec<Param> = recipe.parameters.iter().map(Param::from).collect();
        Self {
            values: vec![String::new(); params.len()],
            params,
            cursor: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }

    pub fn len(&self) -> usize {
        self.params.len()
    }

    /// Fill the rows from a positional argument list, the way `just` would
    /// assign them: one per parameter, the rest into a trailing variadic.
    pub fn prefill(&mut self, args: &[String]) {
        for value in &mut self.values {
            value.clear();
        }
        let mut rest = args.iter();
        for (param, value) in self.params.iter().zip(&mut self.values) {
            if param.is_variadic() {
                *value = rest.map(|s| quote(s)).collect::<Vec<_>>().join(" ");
                break;
            }
            if let Some(arg) = rest.next() {
                *value = arg.clone();
            }
        }
    }

    /// [`Self::prefill`] from a shell-style line such as a config's `args`.
    pub fn prefill_line(&mut self, line: &str) {
        self.prefill(&split_words(line));
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let count = self.params.len() as isize;
        if count > 0 {
            self.cursor = (self.cursor as isize + delta).rem_euclid(count) as usize;
        }
    }

    pub fn push_char(&mut self, c: char) {
        if let Some(value) = self.values.get_mut(self.cursor) {
            value.push(c);
        }
    }

    pub fn pop_char(&mut self) {
        if let Some(value) = self.values.get_mut(self.cursor) {
            value.pop();
        }
    }

    pub fn clear(&mut self) {
        if let Some(value) = self.values.get_mut(self.cursor) {
            value.clear();
        }
    }

    /// The positional arguments to pass after the recipe name. Empty rows
    /// fall back to their default; a required one left empty is an error.
    pub fn args(&self) -> Result<Vec<String>, String> {
        self.collect(true)
    }

    /// Like [`Self::args`], but skips what cannot be resolved instead of
    /// failing, for previews while the form is still being filled in.
    pub fn args_lenient(&self) -> Vec<String> {
        self.collect(false).unwrap_or_default()
    }

    /// The arguments as one shell line, for `sbatch --wrap`.
    pub fn shell_line(&self) -> String {
        self.args_lenient()
            .iter()
            .map(|a| quote(a))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn collect(&self, strict: bool) -> Result<Vec<String>, String> {
        let last_filled = self.values.iter().rposition(|v| !v.trim().is_empty());
        let mut out = Vec::new();

        for (index, (param, value)) in self.params.iter().zip(&self.values).enumerate() {
            let value = value.trim();
            if param.is_variadic() {
                let words = split_words(value);
                if words.is_empty() && param.kind == ParamKind::Plus && param.default.is_none() {
                    if strict {
                        return Err(format!("{} needs at least one value", param.name));
                    }
                    continue;
                }
                out.extend(words);
                continue;
            }
            if !value.is_empty() {
                out.push(value.to_owned());
                continue;
            }
            // Empty: just applies the default itself, unless a later row is
            // filled, in which case this position has to be occupied.
            let needed = last_filled.is_some_and(|last| index < last);
            match (&param.default, &param.default_text) {
                (Some(default), _) if needed => out.push(default.clone()),
                (Some(_), _) => {}
                (None, Some(_)) if needed && strict => {
                    return Err(format!(
                        "{} must be given: its default is an expression",
                        param.name
                    ));
                }
                (None, Some(_)) => {}
                (None, None) if strict => return Err(format!("{} is required", param.name)),
                (None, None) => {}
            }
        }
        Ok(out)
    }
}

/// Split a line into words the way a POSIX shell would: whitespace
/// separates, single and double quotes group, backslash escapes.
pub fn split_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    word.push(c);
                }
            }
            '"' => {
                in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some(escaped @ ('"' | '\\' | '$' | '`')) => word.push(escaped),
                            Some(other) => {
                                word.push('\\');
                                word.push(other);
                            }
                            None => word.push('\\'),
                        },
                        other => word.push(other),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            other => {
                in_word = true;
                word.push(other);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words
}

/// Quote a word so a shell reads it back as one argument.
pub fn quote(word: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_-+=:,./@%".contains(c);
    if !word.is_empty() && word.chars().all(safe) {
        return word.to_owned();
    }
    format!("'{}'", word.replace('\'', "'\\''"))
}
