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

use std::collections::BTreeMap;

use serde_json::Value;

use crate::model::Dependency;
use crate::tree::{Kind, Tree};

/// One job of a chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The recipe's node in the tree.
    pub id: usize,
    pub namepath: String,
    /// Literal arguments the dependency was declared with: `(fetch "hg38")`
    /// gives `hg38`. Empty when it was named bare.
    pub args: String,
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
        };
        builder.visit(head, String::new())?;
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
}

impl Builder<'_> {
    fn visit(&mut self, id: usize, args: String) -> Result<usize, String> {
        let node = &self.tree.nodes[id];
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
            after: Vec::new(),
        });
        self.index.insert(key, at);
        self.open.push(at);

        let recipe = node
            .info
            .as_ref()
            .map(|info| info.recipe.clone())
            .ok_or_else(|| format!("{} has no recipe body", node.namepath))?;
        let (priors, subsequents) = recipe.split_dependencies();

        for dependency in priors {
            let child = self.follow(&node.namepath.clone(), dependency)?;
            self.links[at].after.push(child);
        }
        for dependency in subsequents {
            let child = self.follow(&node.namepath.clone(), dependency)?;
            self.links[child].after.push(at);
        }

        self.open.pop();
        Ok(at)
    }

    fn follow(&mut self, from: &str, dependency: &Dependency) -> Result<usize, String> {
        let id = resolve(self.tree, self.root, from, &dependency.recipe)?;
        let args = literal_args(from, dependency)?;
        self.visit(id, args)
    }
}

/// Find the recipe a dependency names. `just --dump` writes the bare name
/// even for `mod::recipe`, so a name missing from the recipe's own module is
/// looked for anywhere in the tree, as long as only one recipe answers to it.
fn resolve(tree: &Tree, root: usize, from: &str, name: &str) -> Result<usize, String> {
    let module = from.rsplit_once("::").map(|(module, _)| module);
    let qualified = match (module, name.contains("::")) {
        (Some(module), false) => format!("{module}::{name}"),
        _ => name.to_owned(),
    };
    if let Some(id) = tree.find_namepath(root, &qualified) {
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

/// The dependency's arguments as one string for the `args` field. Only
/// literals can be carried over: a variable or a call would need `just` to
/// evaluate it, and the job is the one that runs `just`.
fn literal_args(from: &str, dependency: &Dependency) -> Result<String, String> {
    let mut out = Vec::with_capacity(dependency.arguments.len());
    for value in &dependency.arguments {
        match value {
            Value::String(text) => out.push(quote(text)),
            other => {
                return Err(format!(
                    "{from} calls {} with {}, which is not a literal",
                    dependency.recipe,
                    crate::model::render_expression(other)
                ));
            }
        }
    }
    Ok(out.join(" "))
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
