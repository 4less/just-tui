//! Serde model for `just --dump --dump-format json`.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

/// One justfile (the root, or a `mod`).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Justfile {
    #[serde(default)]
    pub aliases: BTreeMap<String, Alias>,
    #[serde(default)]
    pub assignments: BTreeMap<String, Assignment>,
    #[serde(default)]
    pub modules: BTreeMap<String, Justfile>,
    #[serde(default)]
    pub recipes: BTreeMap<String, Recipe>,
    #[serde(default)]
    pub doc: Option<String>,
    /// Name of the recipe `just` runs with no arguments.
    #[serde(default)]
    pub first: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub settings: Settings,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub dotenv_load: bool,
    #[serde(default)]
    pub export: bool,
    #[serde(default)]
    pub positional_arguments: bool,
    #[serde(default)]
    pub shell: Option<Value>,
    #[serde(default)]
    pub working_directory: Option<String>,
}

impl Settings {
    /// Only the settings that differ from just's defaults, ready to display.
    pub fn summary(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if self.dotenv_load {
            out.push(("dotenv-load".into(), "true".into()));
        }
        if self.export {
            out.push(("export".into(), "true".into()));
        }
        if self.positional_arguments {
            out.push(("positional-arguments".into(), "true".into()));
        }
        if let Some(shell) = &self.shell {
            let rendered = render_expression(shell);
            if !rendered.trim().is_empty() {
                out.push(("shell".into(), rendered));
            }
        }
        if let Some(dir) = &self.working_directory {
            out.push(("working-directory".into(), dir.clone()));
        }
        out
    }
}

/// A `name := value` binding.
#[derive(Debug, Clone, Deserialize)]
pub struct Assignment {
    #[serde(default)]
    pub export: bool,
    #[serde(default)]
    pub private: bool,
    pub value: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Alias {
    pub name: String,
    pub target: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Recipe {
    #[serde(default)]
    pub attributes: Vec<Value>,
    /// Lines, each a list of fragments: a string, or an interpolation node.
    #[serde(default)]
    pub body: Vec<Vec<Value>>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub doc: Option<String>,
    pub name: String,
    pub namepath: String,
    #[serde(default)]
    pub parameters: Vec<Parameter>,
    #[serde(default)]
    pub priors: usize,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub shebang: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    #[serde(default)]
    pub arguments: Vec<Value>,
    pub recipe: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Parameter {
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub export: bool,
    /// `singular`, `plus`, `star`.
    #[serde(default)]
    pub kind: String,
    pub name: String,
}

impl Parameter {
    /// `name`, `*name`, `+name`, rendered the way it appears in a justfile.
    pub fn sigil(&self) -> &'static str {
        match self.kind.as_str() {
            "plus" => "+",
            "star" => "*",
            _ => "",
        }
    }

    /// The default as it appears in the justfile, quotes included.
    pub fn default_text(&self) -> Option<String> {
        self.default.as_ref().map(render_literal)
    }
}

impl Recipe {
    /// The `[group('x')]` this recipe belongs to, if any.
    pub fn group(&self) -> Option<String> {
        self.attributes.iter().find_map(|a| match a {
            Value::Object(map) => map.get("group").and_then(Value::as_str).map(str::to_owned),
            _ => None,
        })
    }

    /// `[doc("...")]` overrides the comment above the recipe.
    pub fn doc_attribute(&self) -> Option<String> {
        self.attributes.iter().find_map(|a| match a {
            Value::Object(map) => map.get("doc").and_then(Value::as_str).map(str::to_owned),
            _ => None,
        })
    }

    /// Human-readable attribute names, e.g. `group('greet')`, `no-cd`.
    pub fn attribute_labels(&self) -> Vec<String> {
        self.attributes.iter().map(render_attribute).collect()
    }

    /// `name arg="default" *rest`
    pub fn signature(&self) -> String {
        let mut out = self.name.clone();
        for p in &self.parameters {
            out.push(' ');
            out.push_str(p.sigil());
            out.push_str(&p.name);
            if let Some(d) = p.default_text() {
                out.push('=');
                out.push_str(&d);
            }
        }
        out
    }

    /// Dependencies that run before the body, and those that run after
    /// (just's `recipe: before && after` form).
    pub fn split_dependencies(&self) -> (&[Dependency], &[Dependency]) {
        let split = self.priors.min(self.dependencies.len());
        self.dependencies.split_at(split)
    }

    /// Reconstruct the body from the JSON fragments. Used when the recipe
    /// cannot be located in the source file.
    pub fn body_text(&self) -> String {
        let mut out = String::new();
        for line in &self.body {
            for fragment in line {
                match fragment {
                    Value::String(s) => out.push_str(s),
                    other => {
                        out.push_str("{{ ");
                        out.push_str(&render_expression(other));
                        out.push_str(" }}");
                    }
                }
            }
            out.push('\n');
        }
        out
    }
}

fn render_attribute(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| match v {
                Value::String(s) => format!("{k}('{s}')"),
                Value::Array(items) => {
                    let inner = items.iter().map(render_expression).collect::<Vec<_>>();
                    format!("{k}({})", inner.join(", "))
                }
                other => format!("{k}({})", render_expression(other)),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    }
}

/// Like [`render_expression`], but a bare JSON string is a string *literal*
/// and gets its quotes back.
pub fn render_literal(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{s}\""),
        other => render_expression(other),
    }
}

/// Best-effort rendering of just's expression trees back to justfile syntax.
pub fn render_expression(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(items) => match items.first().and_then(Value::as_str) {
            Some("variable") => items
                .get(1)
                .map(render_expression)
                .unwrap_or_else(|| "?".into()),
            Some("string") => {
                let text = items.get(2).or(items.get(1)).map(render_expression);
                format!("\"{}\"", text.unwrap_or_default())
            }
            Some("call") => {
                let name = items.get(1).map(render_expression).unwrap_or_default();
                let args = items[2..].iter().map(render_expression).collect::<Vec<_>>();
                format!("{name}({})", args.join(", "))
            }
            Some("concatenate") => items[1..]
                .iter()
                .map(render_literal)
                .collect::<Vec<_>>()
                .join(" + "),
            _ => items
                .iter()
                .map(render_expression)
                .collect::<Vec<_>>()
                .join(" "),
        },
        Value::Object(map) => map
            .values()
            .map(render_expression)
            .collect::<Vec<_>>()
            .join(" "),
    }
}
