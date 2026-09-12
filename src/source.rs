//! Locate a recipe inside its justfile so the real source text (comments,
//! attributes, indentation) can be shown instead of a reconstruction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Caches justfile contents, keyed by path.
#[derive(Default)]
pub struct SourceCache {
    files: HashMap<PathBuf, Vec<String>>,
}

/// A recipe as it appears in its justfile.
pub struct Snippet {
    /// Exact source text of the recipe, attributes included.
    pub code: String,
    /// The full comment block above the recipe, `#` markers stripped.
    pub doc: Option<String>,
    /// 1-based line number of the first line of `code`.
    pub start_line: usize,
}

impl SourceCache {
    /// Read a file once and keep it, since every recipe in a module needs it.
    pub fn lines(&mut self, path: &Path) -> Option<&Vec<String>> {
        if !self.files.contains_key(path) {
            let text = crate::world::read(path).ok()?;
            let lines = text.lines().map(str::to_owned).collect();
            self.files.insert(path.to_path_buf(), lines);
        }
        self.files.get(path)
    }

    /// Forget every file, so a reload picks up edits.
    pub fn invalidate(&mut self) {
        self.files.clear();
    }

    /// Extract `name`'s definition from `path`.
    pub fn recipe(&mut self, path: &Path, name: &str) -> Option<Snippet> {
        let lines = self.lines(path)?;
        let def = lines.iter().position(|l| is_recipe_line(l, name))?;

        // Walk up over attributes and the comment block.
        let mut start = def;
        let mut doc_lines: Vec<String> = Vec::new();
        while start > 0 {
            let prev = lines[start - 1].trim_end();
            let trimmed = prev.trim_start();
            if trimmed.starts_with('[') {
                start -= 1;
            } else if trimmed.starts_with('#') && !trimmed.starts_with("#!") {
                doc_lines.push(trimmed.trim_start_matches('#').trim().to_owned());
                start -= 1;
            } else {
                break;
            }
        }
        doc_lines.reverse();

        // Walk down over the indented body, allowing blank lines inside it.
        let mut end = def + 1;
        let mut last_content = def;
        while end < lines.len() {
            let line = &lines[end];
            if line.trim().is_empty() {
                end += 1;
                continue;
            }
            if line.starts_with(' ') || line.starts_with('\t') {
                last_content = end;
                end += 1;
            } else {
                break;
            }
        }

        let code = lines[start..=last_content].join("\n");
        let doc = if doc_lines.is_empty() {
            None
        } else {
            Some(doc_lines.join("\n"))
        };
        Some(Snippet {
            code,
            doc,
            start_line: start + 1,
        })
    }
}

/// Does `line` open the recipe `name`? Recipe headers sit at column 0 and
/// carry a `:` that is not the `:=` of an assignment.
fn is_recipe_line(line: &str, name: &str) -> bool {
    if line.starts_with(' ') || line.starts_with('\t') {
        return false;
    }
    let rest = line.strip_prefix('@').unwrap_or(line);
    let Some(rest) = rest.strip_prefix(name) else {
        return false;
    };
    match rest.chars().next() {
        // `name:` — a bare recipe, as long as it is not `name := value`.
        Some(':') => !rest.starts_with(":="),
        // `name arg:` — parameters follow, so a colon must still appear.
        Some(c) if c.is_whitespace() => !rest.trim_start().starts_with(":=") && rest.contains(':'),
        // Anything else means the line only starts with the same letters.
        _ => false,
    }
}
