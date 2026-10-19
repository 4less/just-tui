//! One recipe and everything `just` would run with it, as separate jobs.
//!
//! `just align` runs `align`'s dependencies first, in one process. Submitted
//! to Slurm as one job, they all share one allocation. A [`Chain`] is the same
//! set of recipes as separate jobs, ordered so that each one comes after
//! everything it waits on; the submit path turns the order into
//! `--dependency=afterok` flags.
//!
//! The justfile is the only source: prior dependencies (`a: b c`) and
//! subsequent ones (`a: b && c`) are both edges, and a recipe reached twice is
//! one link, as `just` would run it once.
//!
//! One rule goes beyond the declared edges: a recipe named `all` runs its
//! prior dependencies in sequence, each after the one before it. `just` runs
//! them in the order written anyway, and since a module cannot depend on a
//! sibling module, `all: setup::all qc::all` is the only place the order
//! between modules can be said at all.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::model::Dependency;
use crate::tree::{Kind, RecipeInfo, Tree};

/// One job of a chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The recipe's node in the tree.
    pub id: usize,
    pub namepath: String,
    /// Literal arguments the dependency was declared with: `(fetch "hg38")`
    /// gives `hg38`, quoted for a shell where it needs to be. Empty when it
    /// was named bare.
    pub args: String,
    /// The same values one by one, for handing to `just` directly.
    pub argv: Vec<String>,
    /// Positions in [`Chain::links`] of the jobs this one waits on.
    pub after: Vec<usize>,
}

impl Link {
    /// `fetch hg38`, or just `fetch`, for a list.
    pub fn label(&self) -> String {
        match self.args.is_empty() {
            true => self.namepath.clone(),
            false => format!("{} {}", self.namepath, self.args),
        }
    }
}

/// Every recipe `just <head>` would run, in an order that respects the
/// dependencies. The head is the last link unless it has subsequents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chain {
    pub links: Vec<Link>,
}

impl Chain {
    /// Follow every dependency from `head`. A chain of one link means the
    /// recipe stands alone and nothing here applies.
    pub fn build(tree: &Tree, root: usize, head: usize) -> Result<Self, String> {
        let mut builder = Builder {
            tree,
            root,
            links: Vec::new(),
            index: BTreeMap::new(),
            open: Vec::new(),
            sequence: Vec::new(),
        };
        builder.visit(head, Vec::new())?;
        builder.chain_sequence();
        Ok(Chain {
            links: sort(builder.links)?,
        })
    }

    pub fn len(&self) -> usize {
        self.links.len()
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }
}

struct Builder<'a> {
    tree: &'a Tree,
    root: usize,
    links: Vec<Link>,
    /// `(namepath, args)` already given a link. The same recipe called with
    /// different arguments is two jobs, as it is two runs for `just`.
    index: BTreeMap<(String, String), usize>,
    /// Links on the current path, for cycle detection.
    open: Vec<usize>,
    /// `(earlier, later)` neighbours in an `all` recipe's priors, applied
    /// once the whole graph is known.
    sequence: Vec<(usize, usize)>,
}

impl Builder<'_> {
    fn visit(&mut self, id: usize, argv: Vec<String>) -> Result<usize, String> {
        let node = &self.tree.nodes[id];
        let args = argv.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ");
        let key = (node.namepath.clone(), args.clone());
        if let Some(&at) = self.index.get(&key) {
            if self.open.contains(&at) {
                return Err(format!("{} depends on itself", node.namepath));
            }
            return Ok(at);
        }

        let at = self.links.len();
        self.links.push(Link {
            id,
            namepath: node.namepath.clone(),
            args,
            argv,
            after: Vec::new(),
        });
        self.index.insert(key, at);
        self.open.push(at);

        let info = node
            .info
            .as_ref()
            .ok_or_else(|| format!("{} has no recipe body", node.namepath))?;
        let recipe = info.recipe.clone();
        let written = written_names(info);
        let namepath = node.namepath.clone();
        let (priors, subsequents) = recipe.split_dependencies();

        let in_sequence = node.name == "all";
        let mut previous = None;
        for (i, dependency) in priors.iter().enumerate() {
            let child = self.follow(&namepath, dependency, written.get(i))?;
            self.links[at].after.push(child);
            if in_sequence && let Some(earlier) = previous {
                self.sequence.push((earlier, child));
            }
            previous = Some(child);
        }
        for (i, dependency) in subsequents.iter().enumerate() {
            let child = self.follow(&namepath, dependency, written.get(priors.len() + i))?;
            self.links[child].after.push(at);
        }

        self.open.pop();
        Ok(at)
    }

    /// Make each later dependency of an `all` wait on the one before it. An
    /// edge the other way round already in the graph wins: in `all: a b`
    /// with `a: b`, `just` runs `b` first and skips it the second time, so
    /// `b` is not made to wait on `a`.
    fn chain_sequence(&mut self) {
        for (earlier, later) in std::mem::take(&mut self.sequence) {
            if earlier == later || waits_on(&self.links, earlier, later) {
                continue;
            }
            if !self.links[later].after.contains(&earlier) {
                self.links[later].after.push(earlier);
            }
        }
    }

    /// `written` is the name as the justfile spells it, which keeps the
    /// module path `just --dump` drops.
    fn follow(
        &mut self,
        from: &str,
        dependency: &Dependency,
        written: Option<&String>,
    ) -> Result<usize, String> {
        let name = match written {
            Some(full) if full.rsplit("::").next() == Some(dependency.recipe.as_str()) => full,
            _ => &dependency.recipe,
        };
        let id = resolve(self.tree, self.root, from, name)?;
        let args = literal_args(from, dependency)?;
        self.visit(id, args)
    }
}

/// Whether `link` waits on `upstream`, directly or through other links.
fn waits_on(links: &[Link], link: usize, upstream: usize) -> bool {
    let mut seen = vec![false; links.len()];
    let mut stack = vec![link];
    while let Some(at) = stack.pop() {
        if at == upstream {
            return true;
        }
        if !std::mem::replace(&mut seen[at], true) {
            stack.extend(&links[at].after);
        }
    }
    false
}

/// The dependency names as the recipe's header writes them, `setup::all`
/// where `just --dump` says only `all`. Empty unless the source was found
/// and names as many dependencies as the dump does, so a header this cannot
/// read falls back to the bare names.
fn written_names(info: &RecipeInfo) -> Vec<String> {
    if !info.exact {
        return Vec::new();
    }
    let Some(header) = info.code.lines().find(|line| {
        let line = line.trim_start();
        !line.is_empty() && !line.starts_with('[') && !line.starts_with('#')
    }) else {
        return Vec::new();
    };
    let names = header_dependencies(header);
    match names.len() == info.recipe.dependencies.len() {
        true => names,
        false => Vec::new(),
    }
}

/// `all a="x:y": setup::all (fetch "hg38") && report  # note` gives
/// `setup::all`, `fetch`, `report`: the names after the header's colon, in
/// order, with a call's arguments and any comment left out.
fn header_dependencies(header: &str) -> Vec<String> {
    let chars: Vec<char> = header.chars().collect();
    // The colon that ends the parameters: outside quotes, not `::` or `:=`.
    let mut quote = None;
    let mut colon = None;
    for (i, &c) in chars.iter().enumerate() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'' | '`') => quote = Some(c),
            (None, ':') => {
                let next = chars.get(i + 1);
                let prev = i.checked_sub(1).map(|p| chars[p]);
                if next != Some(&':') && next != Some(&'=') && prev != Some(':') {
                    colon = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(colon) = colon else {
        return Vec::new();
    };

    let mut names = Vec::new();
    let mut depth = 0usize;
    let mut quote = None;
    let mut word = String::new();
    let mut take_word = true;
    for &c in &chars[colon + 1..] {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        let is_name = c.is_alphanumeric() || matches!(c, '_' | '-' | ':');
        if is_name {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            if depth <= 1 && take_word {
                names.push(std::mem::take(&mut word));
                take_word = depth == 0;
            }
            word.clear();
        }
        match c {
            '"' | '\'' | '`' => quote = Some(c),
            '(' => {
                depth += 1;
                take_word = depth == 1;
            }
            ')' => {
                depth = depth.saturating_sub(1);
                take_word = depth == 0;
            }
            '#' if depth == 0 => break,
            _ => {}
        }
    }
    if !word.is_empty() && depth == 0 {
        names.push(word);
    }
    names
}

/// Find the recipe a dependency names, relative to the recipe's own module
/// first. `just --dump` writes the bare name even for `mod::recipe`, and the
/// header may not have been readable, so a name missing from the recipe's own
/// module is looked for anywhere in the tree, as long as only one recipe
/// answers to it.
fn resolve(tree: &Tree, root: usize, from: &str, name: &str) -> Result<usize, String> {
    if let Some((module, _)) = from.rsplit_once("::")
        && let Some(id) = tree.find_namepath(root, &format!("{module}::{name}"))
    {
        return Ok(id);
    }
    if let Some(id) = tree.find_namepath(root, name) {
        return Ok(id);
    }

    let leaf = name.rsplit("::").next().unwrap_or(name);
    let found: Vec<usize> = tree
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == Kind::Recipe && n.root_index == root && n.name == leaf)
        .map(|(id, _)| id)
        .collect();
    match found.as_slice() {
        [id] => Ok(*id),
        [] => Err(format!(
            "{from} depends on {name}, which is not in this justfile"
        )),
        many => Err(format!(
            "{from} depends on {name}, which could be any of {}",
            many.iter()
                .map(|id| tree.nodes[*id].namepath.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The dependency's arguments as values. Only literals can be carried over:
/// a variable or a call would need `just` to evaluate it, and the job is the
/// one that runs `just`.
fn literal_args(from: &str, dependency: &Dependency) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(dependency.arguments.len());
    for value in &dependency.arguments {
        match value {
            Value::String(text) => out.push(text.clone()),
            other => {
                return Err(format!(
                    "{from} calls {} with {}, which is not a literal",
                    dependency.recipe,
                    crate::model::render_expression(other)
                ));
            }
        }
    }
    Ok(out)
}

/// A literal with a space in it has to survive the shell the job runs under.
fn quote(text: &str) -> String {
    match text.chars().any(char::is_whitespace) {
        true => format!("'{}'", text.replace('\'', "'\\''")),
        false => text.to_owned(),
    }
}

/// Order the links so that every one comes after what it waits on, keeping
/// discovery order among those that are free to go. `after` is rewritten to
/// the new positions.
fn sort(links: Vec<Link>) -> Result<Vec<Link>, String> {
    let count = links.len();
    let mut placed: Vec<Option<usize>> = vec![None; count];
    let mut order: Vec<usize> = Vec::with_capacity(count);

    while order.len() < count {
        let ready = (0..count).find(|&i| {
            placed[i].is_none()
                && links[i]
                    .after
                    .iter()
                    .all(|&upstream| placed[upstream].is_some())
        });
        let Some(next) = ready else {
            // Reachable only through a cycle, which `just` itself refuses to
            // load; kept so a bad graph can never loop here.
            return Err("the dependencies form a cycle".to_owned());
        };
        placed[next] = Some(order.len());
        order.push(next);
    }

    Ok(order
        .into_iter()
        .map(|old| {
            let mut link = links[old].clone();
            link.after = link
                .after
                .iter()
                .map(|&upstream| placed[upstream].expect("every link is placed"))
                .collect();
            link.after.sort_unstable();
            link.after.dedup();
            link
        })
        .collect())
}
