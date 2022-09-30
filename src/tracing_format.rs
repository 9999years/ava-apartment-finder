//! Support for formatting tracing events.
//!
//! This is used to output log messages to the console.
//!
//! Most of the logic is in the [`fmt::Display`] impl for [`EventVisitor`].

use std::fmt;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use owo_colors::Style;
use tap::Tap;
use tracing::field::Field;
use tracing::field::Visit;
use tracing::Level;
use tracing::Subscriber;
use tracing_subscriber::fmt::FormatEvent;
use tracing_subscriber::fmt::FormatFields;
use tracing_subscriber::registry::LookupSpan;

use crate::wrap::TextWrapOptionsExt;

/// We print blank lines before and after long log messages to help visually separate them.
///
/// This becomes an issue if two long log messages are printed one after another.
///
/// If this variable is `true`, we skip the blank line before to prevent printing two blank lines
/// in a row.
///
/// This variable is mutated whenever an [`EventVisitor`] is [`Display`]ed.
static LAST_EVENT_WAS_LONG: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub struct EventFormatter {}

impl<S, N> FormatEvent<S, N> for EventFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>,
        mut writer: tracing_subscriber::fmt::format::Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let visitor =
            EventVisitor::from(*event.metadata().level()).tap_mut(|visitor| event.record(visitor));
        write!(writer, "{visitor}")?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct EventVisitor {
    pub level: Level,
    pub message: String,
    pub fields: Vec<(String, String)>,
}

impl From<Level> for EventVisitor {
    fn from(level: Level) -> Self {
        Self {
            level,
            message: Default::default(),
            fields: Default::default(),
        }
    }
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.fields
                .push((field.name().to_owned(), format!("{value:?}")));
        }
    }
}

impl fmt::Display for EventVisitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // First-line indent text.
        let indent;

        // First-line indent style.
        let mut indent_style = Style::new();

        // Style for the rest of the message.
        let mut text_style = Style::new();

        // Style for field names.
        let mut field_name_style = Style::new().bold();

        // Style for field values.
        let mut field_value_style = Style::new();

        match self.level {
            Level::TRACE => {
                indent = "TRACE ";
                indent_style = indent_style.purple();
                text_style = text_style.dimmed();
                field_name_style = field_name_style.dimmed();
                field_value_style = field_value_style.dimmed();
            }
            Level::DEBUG => {
                indent = "DEBUG ";
                indent_style = indent_style.blue();
                text_style = text_style.dimmed();
                field_name_style = field_name_style.dimmed();
                field_value_style = field_value_style.dimmed();
            }
            Level::INFO => {
                indent = "• ";
                indent_style = indent_style.green();
            }
            Level::WARN => {
                indent = "⚠ ";
                indent_style = indent_style.yellow();
                text_style = text_style.yellow();
            }
            Level::ERROR => {
                indent = "⚠ ";
                indent_style = indent_style.red();
                text_style = text_style.red();
            }
        }

        let indent_colored = format!(
            "{}",
            indent.if_supports_color(Stdout, |text| indent_style.style(text))
        );

        let options = crate::wrap::options()
            .initial_indent(&indent_colored)
            .subsequent_indent("  ");

        let mut message = self.message.clone();

        // If there's only one field, and it fits on the same line as the message, put it on the
        // same line. Otherwise, we use the 'long format' with each field on a separate line.
        let short_format =
            self.fields.len() == 1 && self.fields[0].1.len() < options.width - message.len();

        if short_format {
            for (name, value) in &self.fields {
                message.push_str(&format!(
                    " {}{}",
                    name.if_supports_color(Stdout, |text| field_name_style.style(text)),
                    format!("={value}")
                        .if_supports_color(Stdout, |text| field_value_style.style(text))
                ));
            }
        }

        // Next, color the message _before_ wrapping it. If you wrap before coloring,
        // `textwrap` prepends the `initial_indent` to the first line. The `initial_indent` is
        // colored, so it has a reset sequence at the end, and the message ends up uncolored.
        let message_colored = format!(
            "{}",
            message.if_supports_color(Stdout, |text| text_style.style(text))
        );

        let lines = options.wrap(&message_colored);

        // If there's more than one line of message, add a blank line before and after the message.
        // This doesn't account for fields, but I think that's fine?
        let add_blank_lines = lines.len() > 1;
        // Store `add_blank_lines` and fetch the previous value:
        let last_event_was_long = LAST_EVENT_WAS_LONG.swap(add_blank_lines, Ordering::SeqCst);
        if add_blank_lines && !last_event_was_long {
            writeln!(f)?;
        };

        // Write the actual message, line by line.
        for line in &lines {
            writeln!(f, "{line}")?;
        }

        // Add fields, one per line, at the end.
        if !short_format {
            for (name, value) in &self.fields {
                writeln!(
                    f,
                    "  {}={}",
                    name.if_supports_color(Stdout, |text| field_name_style.style(text)),
                    value.if_supports_color(Stdout, |text| field_value_style.style(text))
                )?;
            }
        }

        // If there's more than one line of output, add a blank line before and after the message.
        if add_blank_lines {
            writeln!(f)?;
        };

        Ok(())
    }
}
