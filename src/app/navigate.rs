//! Moving around the explorer: the cursor, expansion state, and scrolling.

use super::{App, Pane};
use crate::tree::{Kind, Node};

impl App {
    pub fn refresh_visible(&mut self) {
        self.visible = self.tree.visible(self.show_private, &self.filter());
        let cursor = self
            .selected_id
            .and_then(|id| self.visible.iter().position(|&v| v == id))
            .or(if self.visible.is_empty() {
                None
            } else {
                Some(0)
            });
        self.list_state.select(cursor);
        self.selected_id = cursor.map(|i| self.visible[i]);
    }

    pub(super) fn select_first_recipe(&mut self) {
        if let Some(i) = self
            .visible
            .iter()
            .position(|&id| self.tree.nodes[id].kind == Kind::Recipe)
        {
            self.set_cursor(i);
        }
    }

    pub fn selected(&self) -> Option<&Node> {
        self.selected_id.map(|id| &self.tree.nodes[id])
    }

    pub(super) fn set_cursor(&mut self, index: usize) {
        if self.visible.is_empty() {
            self.selected_id = None;
            self.list_state.select(None);
            return;
        }
        let index = index.min(self.visible.len() - 1);
        self.list_state.select(Some(index));
        self.selected_id = Some(self.visible[index]);
        self.code_scroll = 0;
        self.doc_scroll = 0;
    }

    pub fn select_visible(&mut self, row: usize) {
        if row < self.visible.len() {
            self.set_cursor(row);
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let Some(current) = self.list_state.selected() else {
            self.set_cursor(0);
            return;
        };
        let len = self.visible.len() as isize;
        if len == 0 {
            return;
        }
        let next = (current as isize + delta).clamp(0, len - 1);
        self.set_cursor(next as usize);
    }

    pub fn move_to_recipe(&mut self, forward: bool) {
        let Some(current) = self.list_state.selected() else {
            return;
        };
        let range: Vec<usize> = if forward {
            (current + 1..self.visible.len()).collect()
        } else {
            (0..current).rev().collect()
        };
        for i in range {
            if self.tree.nodes[self.visible[i]].kind == Kind::Recipe {
                self.set_cursor(i);
                return;
            }
        }
    }

    pub fn toggle_expand(&mut self) {
        let Some(id) = self.selected_id else { return };
        if self.tree.nodes[id].is_container() {
            self.tree.nodes[id].expanded = !self.tree.nodes[id].expanded;
            self.refresh_visible();
        }
    }

    pub fn expand(&mut self) {
        let Some(id) = self.selected_id else { return };
        if self.tree.nodes[id].is_container() && !self.tree.nodes[id].expanded {
            self.tree.nodes[id].expanded = true;
            self.refresh_visible();
        } else if self.tree.nodes[id].is_container() {
            self.move_cursor(1);
        } else {
            self.focus = Pane::Code;
        }
    }

    pub fn collapse(&mut self) {
        let Some(id) = self.selected_id else { return };
        let node = &self.tree.nodes[id];
        if node.is_container() && node.expanded {
            self.tree.nodes[id].expanded = false;
            self.refresh_visible();
        } else if let Some(parent) = node.parent {
            self.selected_id = Some(parent);
            self.refresh_visible();
        }
    }

    pub fn set_all_expanded(&mut self, expanded: bool) {
        for node in &mut self.tree.nodes {
            if node.kind == Kind::Module {
                node.expanded = expanded || node.depth == 0;
            }
        }
        self.refresh_visible();
    }

    pub fn toggle_private(&mut self) {
        self.show_private = !self.show_private;
        self.refresh_visible();
        self.info(if self.show_private {
            "private recipes shown"
        } else {
            "private recipes hidden"
        });
    }

    pub(super) fn scroll_target(&mut self, delta: i32) {
        match self.focus {
            Pane::Doc => {
                let max = self.doc_lines.saturating_sub(self.doc_height as usize) as i32;
                let next = (self.doc_scroll as i32 + delta).clamp(0, max.max(0));
                self.doc_scroll = next as u16;
            }
            _ => {
                let max = self.code_lines.saturating_sub(self.code_height as usize) as i32;
                let next = (self.code_scroll as i32 + delta).clamp(0, max.max(0));
                self.code_scroll = next as u16;
            }
        }
    }

    pub fn scroll(&mut self, delta: i32) {
        if self.focus == Pane::Tree {
            self.move_cursor(delta as isize);
        } else {
            self.scroll_target(delta);
        }
    }
}
