//! The explorer tree: modules as directories, recipes as files.

use std::path::PathBuf;

use crate::just::Loaded;
use crate::model::{Justfile, Recipe};
use crate::source::SourceCache;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Module,
    /// A `[group('x')]` layer inside a module. Display only: it holds recipes
    /// but is not part of any namepath.
    Group,
    Recipe,
    Alias,
}

#[derive(Debug, Clone)]
pub struct RecipeInfo {
    pub recipe: Recipe,
    /// Exact source text when it could be located, otherwise reconstructed.
    pub code: String,
    pub exact: bool,
    /// Full comment block above the recipe, or just's one-line doc.
    pub doc: Option<String>,
    pub source: Option<PathBuf>,
    pub start_line: usize,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    pub name: String,
    pub namepath: String,
    pub depth: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub expanded: bool,
    pub private: bool,
    pub is_default: bool,
    pub group: Option<String>,
    /// Short summary shown next to the name in the tree.
    pub summary: Option<String>,
    pub info: Option<RecipeInfo>,
    pub alias_target: Option<String>,
    pub source: Option<PathBuf>,
    /// What a search matches against. Usually the namepath, but recipes in a
    /// global file get its name in front, so `git` finds `~/.justx/git.just`.
    pub search_path: String,
    /// Which loaded source this node belongs to.
    pub root_index: usize,
    /// Part of the global recipe library rather than the current project.
    pub global: bool,
    /// Non-default settings, for module nodes.
    pub settings: Vec<(String, String)>,
    /// `name := value` assignments, for module nodes.
    pub variables: Vec<(String, String)>,
    pub is_root: bool,
}

impl Node {
    /// Modules and groups hold other entries; recipes and aliases do not.
    pub fn is_container(&self) -> bool {
        matches!(self.kind, Kind::Module | Kind::Group)
    }
}

/// A group folder, taking its context from the module it sits in.
fn group_node(parent: &Node, name: &str, depth: usize, members: usize) -> Node {
    Node {
        kind: Kind::Group,
        name: name.to_owned(),
        // Groups are not part of any namepath; this is only for context.
        namepath: parent.namepath.clone(),
        depth,
        parent: None,
        children: Vec::new(),
        expanded: true,
        private: false,
        is_default: false,
        group: Some(name.to_owned()),
        summary: Some(format!(
            "{members} recipe{}",
            if members == 1 { "" } else { "s" }
        )),
        info: None,
        alias_target: None,
        // No file of its own, so config scopes are resolved as before.
        source: None,
        search_path: search_path(&parent.search_path, name),
        root_index: parent.root_index,
        global: parent.global,
        settings: Vec::new(),
        variables: Vec::new(),
        is_root: false,
    }
}

/// `prefix::name`, or just `name` at the top level.
fn search_path(prefix: &str, name: &str) -> String {
    match prefix.is_empty() {
        true => name.to_owned(),
        false => format!("{prefix}::{name}"),
    }
}

/// A module's `name := value` bindings, rendered for display.
fn assignments(file: &Justfile) -> Vec<(String, String)> {
    file.assignments
        .iter()
        .map(|(name, assignment)| {
            let mut label = String::new();
            if assignment.export {
                label.push_str("export ");
            }
            label.push_str(name);
            if assignment.private {
                label.push_str(" (private)");
            }
            (label, crate::model::render_literal(&assignment.value))
        })
        .collect()
}

#[derive(Default)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub roots: Vec<usize>,
}

impl Tree {
    /// One root row per loaded source: the project first, then any global
    /// recipe files.
    pub fn build(sources: &[Loaded], cache: &mut SourceCache) -> Self {
        Tree::build_with(sources, cache, true)
    }

    /// `groups` inserts a collapsible `[group('x')]` layer inside any module
    /// whose recipes are actually divided by one.
    pub fn build_with(sources: &[Loaded], cache: &mut SourceCache, groups: bool) -> Self {
        let mut tree = Tree::default();
        for (index, source) in sources.iter().enumerate() {
            let root = tree.add_source(source, index);
            tree.roots.push(root);
        }
        tree.load_sources(cache);
        if groups {
            tree.apply_groups();
        }
        tree
    }

    /// Sort each module's recipes into their groups. Done as a pass over the
    /// finished tree rather than during the build, so a module that turns out
    /// not to need the layer is left exactly as it was.
    fn apply_groups(&mut self) {
        // Group nodes are appended as this runs, and hold only recipes, so
        // only the modules that existed beforehand need visiting.
        let count = self.nodes.len();
        for id in 0..count {
            if self.nodes[id].kind == Kind::Module {
                self.group_children(id);
            }
        }
    }

    /// Recipes fall into buckets by group, in the order the module lists them.
    /// One bucket means the layer would divide nothing, so it is not added.
    fn group_children(&mut self, parent: usize) {
        let children = self.nodes[parent].children.clone();
        let mut buckets: Vec<(Option<String>, Vec<usize>)> = Vec::new();
        for &child in &children {
            if self.nodes[child].kind != Kind::Recipe {
                continue;
            }
            let key = self.nodes[child].group.clone();
            match buckets.iter_mut().find(|(name, _)| *name == key) {
                Some((_, members)) => members.push(child),
                None => buckets.push((key, vec![child])),
            }
        }
        if buckets.len() < 2 {
            return;
        }
        // Ungrouped first, then the groups by name, the way `just --list` puts
        // them; the module's own sub-modules and aliases keep their places.
        buckets.sort_by(|a, b| a.0.cmp(&b.0));

        let depth = self.nodes[parent].depth + 1;
        let mut modules = Vec::new();
        let mut plain = Vec::new();
        let mut aliases = Vec::new();
        for &child in &children {
            match self.nodes[child].kind {
                Kind::Module | Kind::Group => modules.push(child),
                Kind::Alias => aliases.push(child),
                Kind::Recipe => {}
            }
        }

        let mut folders = Vec::new();
        for (name, members) in buckets {
            let Some(name) = name else {
                plain.extend(members);
                continue;
            };
            let id = self.push(group_node(&self.nodes[parent], &name, depth, members.len()));
            for &member in &members {
                self.nodes[member].parent = Some(id);
                self.nodes[member].depth = depth + 1;
            }
            self.nodes[id].children = members;
            folders.push(id);
        }

        let mut order = modules;
        order.extend(plain);
        order.extend(folders);
        order.extend(aliases);
        self.nodes[parent].children = order;
    }

    fn add_source(&mut self, source: &Loaded, index: usize) -> usize {
        let file = &source.justfile;
        let path = source.path.clone();

        let root_id = self.push(Node {
            kind: Kind::Module,
            name: source.label.clone(),
            namepath: String::new(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            // Only the project starts open; a global library is usually long.
            expanded: !source.global,
            private: false,
            is_default: false,
            group: None,
            summary: file.doc.clone(),
            info: None,
            alias_target: None,
            source: path,
            search_path: source.label.clone(),
            root_index: index,
            global: source.global,
            settings: file.settings.summary(),
            variables: assignments(file),
            is_root: true,
        });
        // Global recipes are reached as `git log`, so searches should be too.
        let search_prefix = match source.global {
            true => source.label.clone(),
            false => String::new(),
        };
        let children = self.add_level(
            file,
            Some(root_id),
            1,
            "",
            &search_prefix,
            index,
            source.global,
        );
        self.nodes[root_id].children = children;
        root_id
    }

    #[allow(clippy::too_many_arguments)]
    fn add_level(
        &mut self,
        file: &Justfile,
        parent: Option<usize>,
        depth: usize,
        prefix: &str,
        search_prefix: &str,
        root_index: usize,
        global: bool,
    ) -> Vec<usize> {
        let mut ids = Vec::new();
        let source = file.source.as_ref().map(PathBuf::from);

        // Modules first, the way a file explorer lists directories.
        for (name, module) in &file.modules {
            let namepath = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}::{name}")
            };
            let id = self.push(Node {
                kind: Kind::Module,
                name: name.clone(),
                namepath: namepath.clone(),
                depth,
                parent,
                children: Vec::new(),
                expanded: depth == 0,
                private: false,
                is_default: false,
                group: None,
                summary: module.doc.clone(),
                info: None,
                alias_target: None,
                source: module.source.as_ref().map(PathBuf::from),
                search_path: search_path(search_prefix, name),
                root_index,
                global,
                settings: module.settings.summary(),
                variables: assignments(module),
                is_root: false,
            });
            let children = self.add_level(
                module,
                Some(id),
                depth + 1,
                &namepath,
                &search_path(search_prefix, name),
                root_index,
                global,
            );
            self.nodes[id].children = children;
            ids.push(id);
        }

        for (name, recipe) in &file.recipes {
            let id = self.push(Node {
                kind: Kind::Recipe,
                name: name.clone(),
                namepath: recipe.namepath.clone(),
                depth,
                parent,
                children: Vec::new(),
                expanded: false,
                private: recipe.private || name.starts_with('_'),
                is_default: file.first.as_deref() == Some(name.as_str()),
                group: recipe.group(),
                summary: recipe.doc_attribute().or_else(|| recipe.doc.clone()),
                info: Some(RecipeInfo {
                    code: recipe.body_text(),
                    exact: false,
                    doc: recipe.doc_attribute().or_else(|| recipe.doc.clone()),
                    source: source.clone(),
                    start_line: 1,
                    recipe: recipe.clone(),
                }),
                alias_target: None,
                source: source.clone(),
                search_path: search_path(search_prefix, name),
                root_index,
                global,
                settings: Vec::new(),
                variables: Vec::new(),
                is_root: false,
            });
            ids.push(id);
        }

        for alias in file.aliases.values() {
            let name = &alias.name;
            let namepath = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}::{name}")
            };
            ids.push(self.push(Node {
                kind: Kind::Alias,
                name: name.clone(),
                namepath,
                depth,
                parent,
                children: Vec::new(),
                expanded: false,
                private: false,
                is_default: false,
                group: None,
                summary: Some(format!("alias for {}", alias.target)),
                info: None,
                alias_target: Some(alias.target.clone()),
                source: source.clone(),
                search_path: search_path(search_prefix, name),
                root_index,
                global,
                settings: Vec::new(),
                variables: Vec::new(),
                is_root: false,
            }));
        }

        ids
    }

    fn push(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    /// Replace reconstructed bodies with the real source text where possible.
    fn load_sources(&mut self, cache: &mut SourceCache) {
        for node in &mut self.nodes {
            let (Some(info), Some(path)) = (node.info.as_mut(), node.source.clone()) else {
                continue;
            };
            if let Some(snippet) = cache.recipe(&path, &info.recipe.name) {
                info.code = snippet.code;
                info.exact = true;
                info.start_line = snippet.start_line;
                if let Some(attr_doc) = info.recipe.doc_attribute() {
                    info.doc = Some(attr_doc);
                } else if snippet.doc.is_some() {
                    info.doc = snippet.doc;
                }
                node.summary = info
                    .doc
                    .as_ref()
                    .and_then(|d| d.lines().next().map(str::to_owned));
            }
        }
    }

    /// Rows currently on screen, honouring collapse state and the filter.
    pub fn visible(&self, show_private: bool, filter: &Filter) -> Vec<usize> {
        let mut out = Vec::new();
        for &root in &self.roots {
            self.collect(root, show_private, filter, &mut out);
        }
        out
    }

    fn collect(&self, id: usize, show_private: bool, filter: &Filter, out: &mut Vec<usize>) {
        let node = &self.nodes[id];
        if node.private && !show_private {
            return;
        }
        // A source root disappears when nothing inside it matches, so a
        // search does not leave empty headings behind.
        if !filter.is_empty() && !self.subtree_matches(id, filter, show_private) {
            return;
        }
        out.push(id);
        // A filter forces every surviving branch open, so matches are reachable.
        if node.is_container() && (node.expanded || !filter.is_empty()) {
            for &child in &node.children {
                self.collect(child, show_private, filter, out);
            }
        }
    }

    fn subtree_matches(&self, id: usize, filter: &Filter, show_private: bool) -> bool {
        let node = &self.nodes[id];
        if node.private && !show_private {
            return false;
        }
        // Names match fuzzily; docs only on a literal substring, or a short
        // query would drag in nearly every recipe.
        if filter.matches(&node.search_path)
            || filter.contains(node.summary.as_deref().unwrap_or(""))
        {
            return true;
        }
        node.children
            .iter()
            .any(|&c| self.subtree_matches(c, filter, show_private))
    }

    /// Recipes across every source, private ones included.
    pub fn count_recipes(&self) -> usize {
        self.nodes.iter().filter(|n| n.kind == Kind::Recipe).count()
    }

    /// Whether any module needed a group layer, so the toggle can say so.
    pub fn has_groups(&self) -> bool {
        self.nodes.iter().any(|n| n.kind == Kind::Group)
    }

    /// Real `mod` entries — the synthetic root does not count.
    pub fn count_modules(&self) -> usize {
        self.nodes
            .iter()
            .filter(|n| n.kind == Kind::Module && !n.is_root)
            .count()
    }

    /// Directories of the module chain above `id`, outermost first. Config
    /// files are looked up in these, so a nested module overrides its parent.
    pub fn module_dirs(&self, id: usize) -> Vec<PathBuf> {
        let mut chain = Vec::new();
        let mut current = Some(id);
        while let Some(node_id) = current {
            let node = &self.nodes[node_id];
            if let Some(dir) = node.source.as_ref().and_then(|p| p.parent()) {
                let dir = dir.to_path_buf();
                if !chain.contains(&dir) {
                    chain.push(dir);
                }
            }
            current = node.parent;
        }
        chain.reverse();
        chain
    }

    /// Expand every module above `id`, so a node found by name can be shown.
    pub fn reveal(&mut self, id: usize) {
        let mut current = self.nodes[id].parent;
        while let Some(node_id) = current {
            self.nodes[node_id].expanded = true;
            current = self.nodes[node_id].parent;
        }
    }

    /// Find a recipe node by its full `mod::name` path within one source.
    pub fn find_namepath(&self, root_index: usize, namepath: &str) -> Option<usize> {
        self.nodes.iter().position(|n| {
            n.kind == Kind::Recipe && n.namepath == namepath && n.root_index == root_index
        })
    }
}

/// Case-insensitive subsequence match, the way fuzzy finders do it.
#[derive(Default, Clone)]
pub struct Filter {
    needle: String,
}

impl Filter {
    /// Queries are matched case-insensitively.
    pub fn new(text: &str) -> Self {
        Self {
            needle: text.to_lowercase(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.needle.is_empty()
    }

    /// Do the query's characters appear in order in `haystack`?
    pub fn matches(&self, haystack: &str) -> bool {
        if self.needle.is_empty() {
            return true;
        }
        let mut chars = self.needle.chars();
        let mut want = chars.next();
        for c in haystack.to_lowercase().chars() {
            if Some(c) == want {
                want = chars.next();
                if want.is_none() {
                    return true;
                }
            }
        }
        want.is_none()
    }

    /// Literal substring match, used for documentation text.
    pub fn contains(&self, haystack: &str) -> bool {
        !self.needle.is_empty() && haystack.to_lowercase().contains(&self.needle)
    }

    /// Indices of the characters that matched, for highlighting.
    pub fn match_indices(&self, haystack: &str) -> Vec<usize> {
        let mut out = Vec::new();
        if self.needle.is_empty() {
            return out;
        }
        let mut chars = self.needle.chars();
        let mut want = chars.next();
        for (i, c) in haystack.to_lowercase().chars().enumerate() {
            if Some(c) == want {
                out.push(i);
                want = chars.next();
                if want.is_none() {
                    break;
                }
            }
        }
        if want.is_some() { Vec::new() } else { out }
    }
}
