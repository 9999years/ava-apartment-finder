use std::fmt::Display;
use std::fmt::Write;

use color_eyre::eyre;
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use owo_colors::Style;
use similar::ChangeTag;
use similar::TextDiff;

/// Format a diff of two strings, with colors if `Stdout` is a tty.
///
/// Like [`diff`] but includes a header showing the filenames.
pub fn diff_header(
    old: &str,
    new: &str,
    old_path: impl Display,
    new_path: impl Display,
) -> eyre::Result<String> {
    Ok(format!(
        "{} {}\n{} {}\n{}",
        "---".if_supports_color(Stdout, |text| Style::new().bright_red().bold().style(text)),
        old_path.if_supports_color(Stdout, |text| text.red()),
        "+++".if_supports_color(Stdout, |text| Style::new()
            .bright_green()
            .bold()
            .style(text)),
        new_path.if_supports_color(Stdout, |text| text.green()),
        diff(old, new)?
    ))
}

/// Format a diff of two strings, with colors if `Stdout` is a tty.
pub fn diff(old: &str, new: &str) -> eyre::Result<String> {
    // Adapted from: https://github.com/mitsuhiko/similar/blob/77c20faf94c1969bcedc219851f7b89ab4a8ac5a/examples/terminal-inline.rs

    let mut ret = String::with_capacity(new.len());

    let diff = TextDiff::from_lines(old, new);

    for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
        if idx > 0 {
            // NB: This uses a horizontal line box drawing character (U+2500)
            ret.push_str(&"─".repeat(80));
            ret.push('\n');
        }
        for op in group {
            for change in diff.iter_inline_changes(op) {
                let (sign, style, line_style) = match change.tag() {
                    ChangeTag::Delete => ("-", Style::new().bright_red(), Style::new().red()),
                    ChangeTag::Insert => ("+", Style::new().bright_green(), Style::new().green()),
                    ChangeTag::Equal => (" ", Style::new().dimmed(), Style::new()),
                };
                write!(
                    &mut ret,
                    // NB: This uses a vertical line box drawing character (U+2502)
                    "{}{} │{}",
                    Line(change.old_index()).if_supports_color(Stdout, |text| text.dimmed()),
                    Line(change.new_index()).if_supports_color(Stdout, |text| text.dimmed()),
                    sign.if_supports_color(Stdout, |text| style.bold().style(text)),
                )?;
                for (emphasized, value) in change.iter_strings_lossy() {
                    if emphasized {
                        write!(
                            &mut ret,
                            "{}",
                            value.if_supports_color(Stdout, |text| style
                                .underline()
                                .bold()
                                .on_black()
                                .style(text))
                        )?;
                    } else {
                        write!(
                            &mut ret,
                            "{}",
                            value.if_supports_color(Stdout, |text| line_style.style(text))
                        )?;
                    }
                }
                if change.missing_newline() {
                    ret.push('\n');
                }
            }
        }
    }

    Ok(ret)
}

struct Line(Option<usize>);

impl Display for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.0 {
            None => write!(f, "    "),
            Some(idx) => write!(f, "{:<4}", idx + 1),
        }
    }
}
