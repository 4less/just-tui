//! The justfile highlighter. Assertions are on the text and colour of the
//! spans, which is what the panes actually draw.

use ratatui::style::Color;
use ratatui::text::Line;

use crate::highlight;
use crate::theme;

/// The (text, colour) of every span on one line.
fn spans(line: &Line) -> Vec<(String, Option<Color>)> {
    line.spans
        .iter()
        .map(|s| (s.content.to_string(), s.style.fg))
        .collect()
}

fn only(code: &str) -> Vec<(String, Option<Color>)> {
    let lines = highlight::recipe(code);
    spans(&lines[lines.len() - 1])
}

#[test]
fn interpolations_stand_out_inside_strings() {
    let parts = only("greet:\n    echo \"hi {{ name }} there\"");
    let interp = parts
        .iter()
        .find(|(text, _)| text.contains("{{"))
        .expect("the interpolation is its own span");
    assert_eq!(interp.0, "{{ name }}");
    assert_eq!(interp.1, Some(theme::INTERP));

    // The quoted text around it stays a string.
    assert!(
        parts
            .iter()
            .any(|(text, color)| text.contains("hi ") && *color == Some(theme::STRING))
    );
    // Nothing is dropped on the way through.
    let joined: String = parts.iter().map(|(text, _)| text.as_str()).collect();
    assert_eq!(joined, "    echo \"hi {{ name }} there\"");
}

#[test]
fn shell_variables_and_comments_are_distinct() {
    let parts = only("r:\n    rm ${DIR}/x $TMP  # careful");
    let colors = |needle: &str| {
        parts
            .iter()
            .find(|(text, _)| text == needle)
            .map(|(_, color)| *color)
    };
    assert_eq!(colors("${DIR}"), Some(Some(theme::VARIABLE)));
    assert_eq!(colors("$TMP"), Some(Some(theme::VARIABLE)));
    assert_eq!(colors("# careful"), Some(Some(theme::COMMENT)));
}

#[test]
fn escaped_quotes_do_not_end_a_string() {
    let parts = only(
        r#"r:
    echo "a \" b" tail"#,
    );
    let string = parts
        .iter()
        .find(|(text, color)| *color == Some(theme::STRING) && text.starts_with('"'))
        .expect("one string span");
    assert_eq!(string.0, r#""a \" b""#);
    assert!(parts.iter().any(|(text, _)| text == "tail"));
}

#[test]
fn a_shebang_is_not_a_comment() {
    let lines = highlight::recipe("r:\n    #!/usr/bin/env bash\n    echo hi");
    let shebang = spans(&lines[1]);
    let (text, color) = shebang.last().expect("a shebang span");
    assert_eq!(text, "#!/usr/bin/env bash");
    assert_eq!(*color, Some(theme::KEYWORD), "{shebang:?}");

    // An ordinary comment inside a body still reads as one.
    let comment = spans(&highlight::recipe("r:\n    # note")[1]);
    assert_eq!(comment.last().unwrap().1, Some(theme::COMMENT));
}

#[test]
fn the_header_names_the_recipe_and_its_dependencies() {
    let lines = highlight::recipe("# doc\n[group('g')]\nbuild dir=\".\": fmt\n    cargo build");
    assert_eq!(spans(&lines[0])[0].1, Some(theme::COMMENT), "doc comment");
    assert_eq!(spans(&lines[1])[0].1, Some(theme::ATTRIBUTE), "attribute");

    let header = spans(&lines[2]);
    assert_eq!(header[0], ("build".to_owned(), Some(theme::RECIPE)));
    assert!(
        header
            .iter()
            .any(|(text, color)| text == "dir" && *color == Some(theme::VARIABLE))
    );
    assert!(
        header
            .iter()
            .any(|(text, color)| text.trim() == "fmt" && *color == Some(theme::MODULE)),
        "{header:?}"
    );
}
