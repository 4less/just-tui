//! Editing a recipe in the source pane: a small multi-line text buffer with
//! a caret, undo, and the splice that puts the result back into its file.
//!
//! Only the recipe's own lines are edited — attributes, comment block,
//! header and body, exactly the span the source pane shows — so the rest of
//! the file is never touched, and a file changed on disk since is noticed
//! rather than overwritten.

use std::path::PathBuf;

/// Width a tab is drawn at. The file keeps its tabs; this is only the screen.
pub const TAB_WIDTH: usize = 4;

/// What the last change was, so a run of typing undoes as one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit {
    Insert,
    Delete,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

#[derive(Debug, Clone)]
pub struct Editor {
    /// The file the recipe lives in.
    pub path: PathBuf,
    /// The recipe's namepath, for messages.
    pub namepath: String,
    /// 1-based line in the file of the first line being edited.
    pub start_line: usize,
    /// The lines as they were when editing began, to find them again on save.
    original: Vec<String>,
    pub lines: Vec<String>,
    /// Caret: line, and position in characters (not bytes) along it.
    pub row: usize,
    pub col: usize,
    /// First line and first screen column in view.
    pub top: usize,
    pub left: usize,
    /// Text area size last drawn, for paging and keeping the caret in view.
    pub height: usize,
    pub width: usize,
    /// Where the text area was drawn, for placing the caret by mouse.
    pub view: ratatui::layout::Rect,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: Edit,
    /// Esc was pressed once with unsaved changes; a second one discards.
    pub confirm_discard: bool,
}

impl Editor {
    pub fn new(path: PathBuf, namepath: String, start_line: usize, code: &str) -> Self {
        let mut lines: Vec<String> = code.lines().map(str::to_owned).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Self {
            path,
            namepath,
            start_line,
            original: lines.clone(),
            lines,
            row: 0,
            col: 0,
            top: 0,
            left: 0,
            height: 10,
            width: 40,
            view: ratatui::layout::Rect::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            last: Edit::Other,
            confirm_discard: false,
        }
    }

    pub fn dirty(&self) -> bool {
        self.lines != self.original
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    // ------------------------------------------------------------- editing --

    pub fn insert_char(&mut self, c: char) {
        self.checkpoint(Edit::Insert);
        let at = self.byte_col();
        self.lines[self.row].insert(at, c);
        self.col += 1;
    }

    /// Split the line at the caret, carrying its indentation onto the new
    /// line: inside a recipe body that is nearly always what comes next.
    pub fn newline(&mut self) {
        self.checkpoint(Edit::Other);
        let at = self.byte_col();
        let rest = self.lines[self.row].split_off(at);
        let indent: String = self.lines[self.row]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        self.col = indent.chars().count();
        self.row += 1;
        self.lines.insert(self.row, indent + rest.trim_start());
    }

    /// One level of the indentation the recipe body already uses.
    pub fn indent(&mut self) {
        let unit = self.indent_unit();
        self.checkpoint(Edit::Other);
        let at = self.byte_col();
        self.lines[self.row].insert_str(at, &unit);
        self.col += unit.chars().count();
    }

    /// Take one level of indentation off the start of the line.
    pub fn dedent(&mut self) {
        let line = &self.lines[self.row];
        let remove = if line.starts_with('\t') {
            1
        } else {
            line.chars()
                .take(self.indent_unit().chars().count())
                .take_while(|c| *c == ' ')
                .count()
        };
        if remove == 0 {
            return;
        }
        self.checkpoint(Edit::Other);
        self.lines[self.row].replace_range(..remove, "");
        self.col = self.col.saturating_sub(remove);
    }

    pub fn backspace(&mut self) {
        if self.col > 0 {
            self.checkpoint(Edit::Delete);
            self.col -= 1;
            let at = self.byte_col();
            self.lines[self.row].remove(at);
        } else if self.row > 0 {
            self.checkpoint(Edit::Other);
            let line = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.line_len();
            self.lines[self.row].push_str(&line);
        }
    }

    pub fn delete(&mut self) {
        if self.col < self.line_len() {
            self.checkpoint(Edit::Delete);
            let at = self.byte_col();
            self.lines[self.row].remove(at);
        } else if self.row + 1 < self.lines.len() {
            self.checkpoint(Edit::Other);
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
        }
    }

    /// Remove the caret's line, keeping at least one.
    pub fn delete_line(&mut self) {
        self.checkpoint(Edit::Other);
        if self.lines.len() == 1 {
            self.lines[0].clear();
        } else {
            self.lines.remove(self.row);
            self.row = self.row.min(self.lines.len() - 1);
        }
        self.col = self.col.min(self.line_len());
    }

    pub fn undo(&mut self) {
        if let Some(snapshot) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(snapshot);
        }
    }

    pub fn redo(&mut self) {
        if let Some(snapshot) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(snapshot);
        }
    }

    // -------------------------------------------------------------- moving --

    pub fn move_left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.line_len();
        }
        self.last = Edit::Other;
    }

    pub fn move_right(&mut self) {
        if self.col < self.line_len() {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
        self.last = Edit::Other;
    }

    pub fn move_rows(&mut self, delta: isize) {
        let last = self.lines.len() as isize - 1;
        self.row = (self.row as isize + delta).clamp(0, last) as usize;
        self.col = self.col.min(self.line_len());
        self.last = Edit::Other;
    }

    pub fn home(&mut self) {
        // First to the text, then to the margin, like most editors.
        let indent = self.lines[self.row]
            .chars()
            .take_while(|c| c.is_whitespace())
            .count();
        self.col = if self.col == indent { 0 } else { indent };
        self.last = Edit::Other;
    }

    pub fn end(&mut self) {
        self.col = self.line_len();
        self.last = Edit::Other;
    }

    /// Put the caret under a screen position inside the text area.
    pub fn click(&mut self, x: usize, y: usize) {
        self.row = (self.top + y).min(self.lines.len() - 1);
        let target = self.left + x;
        let mut width = 0;
        self.col = self.line_len();
        for (i, c) in self.lines[self.row].chars().enumerate() {
            let next = advance(width, c);
            if next > target {
                self.col = i;
                break;
            }
            width = next;
        }
        self.last = Edit::Other;
    }

    /// Screen column of the caret along its line, tabs expanded.
    pub fn caret_column(&self) -> usize {
        self.lines[self.row].chars().take(self.col).fold(0, advance)
    }

    /// Scroll so the caret is on screen.
    pub fn follow(&mut self) {
        if self.row < self.top {
            self.top = self.row;
        } else if self.height > 0 && self.row >= self.top + self.height {
            self.top = self.row + 1 - self.height;
        }
        let column = self.caret_column();
        if column < self.left {
            self.left = column;
        } else if self.width > 0 && column >= self.left + self.width {
            self.left = column + 1 - self.width;
        }
    }

    // -------------------------------------------------------------- saving --

    /// The file's text with the edited lines in place of the original ones.
    /// Fails when those lines are no longer where they were.
    pub fn splice(&self, file: &str) -> Result<String, String> {
        let newline = if file.contains("\r\n") { "\r\n" } else { "\n" };
        let mut lines: Vec<&str> = file.lines().collect();
        let start = self.start_line - 1;
        let end = start + self.original.len();
        let unchanged = lines
            .get(start..end)
            .is_some_and(|span| span.iter().eq(self.original.iter()));
        if !unchanged {
            return Err(format!(
                "{} changed on disk since editing began — Esc, R to reload, then edit again",
                self.path.display()
            ));
        }
        lines.splice(start..end, self.lines.iter().map(String::as_str));
        let mut text = lines.join(newline);
        if file.ends_with('\n') || file.is_empty() {
            text.push_str(newline);
        }
        Ok(text)
    }

    // ------------------------------------------------------------- helpers --

    fn line_len(&self) -> usize {
        self.lines[self.row].chars().count()
    }

    fn byte_col(&self) -> usize {
        let line = &self.lines[self.row];
        line.char_indices()
            .nth(self.col)
            .map_or(line.len(), |(i, _)| i)
    }

    /// The body's own indentation: a tab if it uses tabs, otherwise the
    /// spaces in front of its first indented line, four if there is none.
    fn indent_unit(&self) -> String {
        self.lines
            .iter()
            .find(|l| l.starts_with([' ', '\t']) && !l.trim().is_empty())
            .map(|l| match l.starts_with('\t') {
                true => "\t".to_owned(),
                false => l.chars().take_while(|c| *c == ' ').collect(),
            })
            .unwrap_or_else(|| "    ".to_owned())
    }

    /// Remember the state before a change. Typing or deleting in a row
    /// shares one snapshot, so undo takes back a word, not a letter.
    fn checkpoint(&mut self, kind: Edit) {
        self.confirm_discard = false;
        if kind == Edit::Other || kind != self.last {
            self.undo.push(self.snapshot());
            self.redo.clear();
        }
        self.last = kind;
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            row: self.row,
            col: self.col,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.lines = snapshot.lines;
        self.row = snapshot.row;
        self.col = snapshot.col;
        self.last = Edit::Other;
    }
}

/// Screen width after drawing `c` at column `width`.
fn advance(width: usize, c: char) -> usize {
    match c {
        '\t' => (width / TAB_WIDTH + 1) * TAB_WIDTH,
        _ => width + 1,
    }
}

/// A line with its tabs turned into the spaces they are drawn as.
pub fn expand_tabs(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut width = 0;
    for c in line.chars() {
        let next = advance(width, c);
        match c {
            '\t' => out.extend(std::iter::repeat_n(' ', next - width)),
            _ => out.push(c),
        }
        width = next;
    }
    out
}
