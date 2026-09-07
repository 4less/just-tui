//! A small hand-rolled highlighter for justfile recipes and their shell bodies.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme;

/// Shell control flow, highlighted inside recipe bodies.
const SHELL_KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "until", "case",
    "esac", "function", "return", "exit", "set", "local", "export", "source", "trap", "shift",
    "break", "continue",
];

/// Commands common enough in justfiles to be worth colouring.
const SHELL_BUILTINS: &[&str] = &[
    "echo", "printf", "cd", "mkdir", "rm", "cp", "mv", "test", "read", "sed", "awk", "grep", "cat",
    "git", "cargo", "npm", "docker", "python", "python3", "make", "just", "curl", "ls", "touch",
    "chmod", "kill", "sleep",
];

/// Highlight a whole recipe: attributes, the signature line, then the body.
pub fn recipe(code: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut seen_header = false;
    for raw in code.lines() {
        let trimmed = raw.trim_start();
        let indented = raw.starts_with(' ') || raw.starts_with('\t');

        if trimmed.starts_with('#') && !indented && !seen_header {
            lines.push(Line::from(Span::styled(
                raw.to_owned(),
                Style::default()
                    .fg(theme::COMMENT)
                    .add_modifier(Modifier::ITALIC),
            )));
        } else if trimmed.starts_with('[') && !indented && !seen_header {
            lines.push(Line::from(Span::styled(
                raw.to_owned(),
                Style::default().fg(theme::ATTRIBUTE),
            )));
        } else if !indented && !trimmed.is_empty() && !seen_header {
            seen_header = true;
            lines.push(header_line(raw));
        } else {
            lines.push(body_line(raw));
        }
    }
    lines
}

/// `name arg="x": dep1 dep2`
fn header_line(raw: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let colon = raw.find(':').unwrap_or(raw.len());
    let (left, right) = raw.split_at(colon);

    let mut parts = left.split_whitespace();
    if let Some(name) = parts.next() {
        let offset = left.find(name).unwrap_or(0);
        if offset > 0 {
            spans.push(Span::raw(left[..offset].to_owned()));
        }
        spans.push(Span::styled(
            name.to_owned(),
            Style::default()
                .fg(theme::RECIPE)
                .add_modifier(Modifier::BOLD),
        ));
    }
    for param in parts {
        spans.push(Span::raw(" "));
        match param.split_once('=') {
            Some((n, d)) => {
                spans.push(Span::styled(
                    n.to_owned(),
                    Style::default().fg(theme::VARIABLE),
                ));
                spans.push(Span::styled("=", theme::label()));
                spans.push(Span::styled(
                    d.to_owned(),
                    Style::default().fg(theme::STRING),
                ));
            }
            None => spans.push(Span::styled(
                param.to_owned(),
                Style::default().fg(theme::VARIABLE),
            )),
        }
    }

    if !right.is_empty() {
        spans.push(Span::styled(":", theme::label()));
        let deps = &right[1..];
        for token in deps.split_inclusive(char::is_whitespace) {
            let style = if token.trim().is_empty() {
                theme::value()
            } else {
                Style::default().fg(theme::MODULE)
            };
            spans.push(Span::styled(token.to_owned(), style));
        }
    }
    Line::from(spans)
}

/// A body line: `{{ … }}` interpolation, shell strings, `$VAR`, comments.
///
/// Scanned left to right; each branch consumes one construct and appends its
/// spans, so no branch has to know about the others.
fn body_line(raw: &str) -> Line<'static> {
    let chars: Vec<char> = raw.chars().collect();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut word = String::new();
    let mut i = 0;

    // Indentation is emitted once, rather than a span per space.
    let indent = chars.iter().take_while(|c| c.is_whitespace()).count();
    if indent > 0 {
        spans.push(Span::raw(chars[..indent].iter().collect::<String>()));
        i = indent;
    }

    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();

        if c == '{' && next == Some('{') {
            flush_word(&mut spans, &mut word);
            spans.push(scan_interpolation(&chars, &mut i));
        } else if c == '#' {
            flush_word(&mut spans, &mut word);
            // A shebang is a directive to just, not a comment.
            let style = match next == Some('!') && only_blank(&spans) {
                true => Style::default()
                    .fg(theme::KEYWORD)
                    .add_modifier(Modifier::ITALIC),
                false => Style::default().fg(theme::COMMENT),
            };
            spans.push(Span::styled(chars[i..].iter().collect::<String>(), style));
            break;
        } else if c == '"' || c == '\'' {
            flush_word(&mut spans, &mut word);
            scan_string(&chars, &mut i, &mut spans);
        } else if c == '$' {
            flush_word(&mut spans, &mut word);
            spans.push(scan_variable(&chars, &mut i));
        } else if c.is_alphanumeric() || c == '_' || c == '-' {
            word.push(c);
            i += 1;
        } else {
            flush_word(&mut spans, &mut word);
            spans.push(Span::styled(c.to_string(), theme::value()));
            i += 1;
        }
    }
    flush_word(&mut spans, &mut word);

    if spans.is_empty() {
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

/// Emit the word gathered so far. The first word on a line is the command
/// being run, so it is highlighted like one — indentation does not count.
fn flush_word(spans: &mut Vec<Span<'static>>, word: &mut String) {
    if word.is_empty() {
        return;
    }
    let first = only_blank(spans);
    spans.push(word_span(word, first));
    word.clear();
}

/// True while nothing but indentation has been emitted.
fn only_blank(spans: &[Span<'static>]) -> bool {
    spans.iter().all(|s| s.content.trim().is_empty())
}

/// `{{ name }}` — a just interpolation, wherever it appears.
fn scan_interpolation(chars: &[char], i: &mut usize) -> Span<'static> {
    let start = *i;
    *i += 2;
    while *i < chars.len() {
        if chars[*i] == '}' && chars.get(*i + 1) == Some(&'}') {
            *i += 2;
            break;
        }
        *i += 1;
    }
    Span::styled(
        chars[start..*i].iter().collect::<String>(),
        Style::default()
            .fg(theme::INTERP)
            .add_modifier(Modifier::BOLD),
    )
}

/// A quoted string, with any interpolations inside it kept distinct.
fn scan_string(chars: &[char], i: &mut usize, spans: &mut Vec<Span<'static>>) {
    let quote = chars[*i];
    let mut start = *i;
    *i += 1;

    while *i < chars.len() {
        match chars[*i] {
            '\\' => *i = (*i + 2).min(chars.len()),
            '{' if chars.get(*i + 1) == Some(&'{') => {
                push_string(spans, &chars[start..*i]);
                spans.push(scan_interpolation(chars, i));
                start = *i;
            }
            c if c == quote => {
                *i += 1;
                break;
            }
            _ => *i += 1,
        }
    }
    push_string(spans, &chars[start..*i]);
}

/// Emit a run of string text, skipping empty pieces.
fn push_string(spans: &mut Vec<Span<'static>>, chars: &[char]) {
    if !chars.is_empty() {
        spans.push(Span::styled(
            chars.iter().collect::<String>(),
            Style::default().fg(theme::STRING),
        ));
    }
}

/// `$NAME` or `${NAME}` — a shell variable, not a just one.
fn scan_variable(chars: &[char], i: &mut usize) -> Span<'static> {
    let start = *i;
    *i += 1;
    if chars.get(*i) == Some(&'{') {
        while *i < chars.len() {
            let done = chars[*i] == '}';
            *i += 1;
            if done {
                break;
            }
        }
    } else {
        while *i < chars.len() && (chars[*i].is_alphanumeric() || chars[*i] == '_') {
            *i += 1;
        }
    }
    Span::styled(
        chars[start..*i].iter().collect::<String>(),
        Style::default().fg(theme::VARIABLE),
    )
}

/// Colour a bare word: shell keywords, known commands, and numbers.
fn word_span(word: &str, first: bool) -> Span<'static> {
    let style = if SHELL_KEYWORDS.contains(&word) {
        Style::default().fg(theme::KEYWORD)
    } else if SHELL_BUILTINS.contains(&word) || first {
        Style::default().fg(theme::ACCENT)
    } else if word.chars().all(|c| c.is_ascii_digit()) {
        Style::default().fg(theme::NUMBER)
    } else {
        theme::value()
    };
    Span::styled(word.to_owned(), style)
}

/// Markdown-ish rendering for doc comment text.
pub fn doc(text: &str) -> Vec<Line<'static>> {
    text.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                let indent = &line[..line.len() - trimmed.len()];
                Line::from(vec![
                    Span::raw(indent.to_owned()),
                    Span::styled("• ", Style::default().fg(theme::ACCENT)),
                    Span::styled(trimmed[2..].to_owned(), theme::value()),
                ])
            } else if trimmed.starts_with('`') && trimmed.ends_with('`') && trimmed.len() > 1 {
                Line::from(Span::styled(
                    line.to_owned(),
                    Style::default().fg(theme::STRING),
                ))
            } else {
                Line::from(Span::styled(line.to_owned(), theme::value()))
            }
        })
        .collect()
}
