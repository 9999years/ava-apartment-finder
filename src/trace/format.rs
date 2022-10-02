//! Support for formatting tracing events.
//!
//! This is used to output log messages to the console.
//!
//! Most of the logic is in the [`fmt::Display`] impl for [`EventVisitor`].

use std::fmt;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use chrono::Utc;
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

#[derive(Default)]
pub struct EventFormatter {
    /// We print blank lines before and after long log messages to help visually separate them.
    ///
    /// This becomes an issue if two long log messages are printed one after another.
    ///
    /// If this variable is `true`, we skip the blank line before to prevent printing two blank
    /// lines in a row.
    ///
    /// This variable is mutated whenever [`format_event`] is called.
    last_event_was_long: AtomicBool,
}

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
        let visitor = EventVisitor::new(
            *event.metadata().level(),
            AtomicBool::new(self.last_event_was_long.load(Ordering::SeqCst)),
        )
        .tap_mut(|visitor| event.record(visitor));
        write!(writer, "{visitor}")?;
        // Transfer `last_event_was_long` state back into this object.
        self.last_event_was_long.store(
            visitor.last_event_was_long.load(Ordering::SeqCst),
            Ordering::SeqCst,
        );
        Ok(())
    }
}

#[derive(Debug)]
pub struct EventVisitor {
    pub last_event_was_long: AtomicBool,
    pub level: Level,
    style: EventStyle,
    pub message: String,
    pub fields: Vec<(String, String)>,
}

impl EventVisitor {
    pub fn new(level: Level, last_event_was_long: AtomicBool) -> Self {
        Self {
            level,
            last_event_was_long,
            style: EventStyle::new(level),
            message: Default::default(),
            fields: Default::default(),
        }
    }

    /// If there's only one field, and it fits on the same line as the message, put it on the
    /// same line. Otherwise, we use the 'long format' with each field on a separate line.
    fn use_short_format(&self, term_width: usize) -> bool {
        self.fields.len() == 1
            && self.fields[0].0.len() + self.fields[0].1.len() + 2 < term_width - self.message.len()
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
        let indent_colored = self.style.indent_colored();

        let options = crate::wrap::options()
            .initial_indent(&indent_colored)
            .subsequent_indent(self.style.subsequent_indent);

        // Next, color the message _before_ wrapping it. If you wrap before coloring,
        // `textwrap` prepends the `initial_indent` to the first line. The `initial_indent` is
        // colored, so it has a reset sequence at the end, and the message ends up uncolored.
        let mut message = format!("{} {}", Utc::now().format("%c").dimmed(), self.message);

        // If there's only one field, and it fits on the same line as the message, put it on the
        // same line. Otherwise, we use the 'long format' with each field on a separate line.
        let short_format = self.use_short_format(options.width);

        if short_format {
            for (name, value) in &self.fields {
                message.push_str(&format!(" {}", self.style.style_field(name, value)));
            }
        }

        let message_colored = self.style.style_message(&message);

        let lines = options.wrap(&message_colored);

        // If there's more than one line of message, add a blank line before and after the message.
        // This doesn't account for fields, but I think that's fine?
        let add_blank_lines = lines.len() > 1;
        // Store `add_blank_lines` and fetch the previous value:
        let last_event_was_long = self
            .last_event_was_long
            .swap(add_blank_lines, Ordering::SeqCst);
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
                    "{}{}",
                    self.style.subsequent_indent,
                    self.style.style_field(name, value)
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

#[derive(Debug)]
struct EventStyle {
    /// First-line indent text.
    indent_text: &'static str,

    /// Subsequent indent text.
    subsequent_indent: &'static str,

    /// Style for first-line indent text.
    indent: Style,

    /// Style for message text.
    text: Style,

    /// Style for field names.
    field_name: Style,

    /// Style for field values.
    field_value: Style,
}

impl EventStyle {
    fn new(level: Level) -> Self {
        let indent_text;
        let mut indent = Style::new();
        let mut text = Style::new();
        let mut field_name = Style::new().bold();
        let mut field_value = Style::new();

        match level {
            Level::TRACE => {
                indent_text = "TRACE ";
                indent = indent.purple();
                text = text.dimmed();
                field_name = field_name.dimmed();
                field_value = field_value.dimmed();
            }
            Level::DEBUG => {
                indent_text = "DEBUG ";
                indent = indent.blue();
                text = text.dimmed();
                field_name = field_name.dimmed();
                field_value = field_value.dimmed();
            }
            Level::INFO => {
                indent_text = "• ";
                indent = indent.green();
            }
            Level::WARN => {
                indent_text = "⚠ ";
                indent = indent.yellow();
                text = text.yellow();
            }
            Level::ERROR => {
                indent_text = "⚠ ";
                indent = indent.red();
                text = text.red();
            }
        }

        Self {
            indent_text,
            subsequent_indent: "  ",
            indent,
            text,
            field_name,
            field_value,
        }
    }

    fn style_field(&self, name: &str, value: &str) -> String {
        format!(
            "{name}{value}",
            name = name.if_supports_color(Stdout, |text| self.field_name.style(text)),
            value =
                format!("={value}").if_supports_color(Stdout, |text| self.field_value.style(text)),
        )
    }

    fn indent_colored(&self) -> String {
        self.indent_text
            .if_supports_color(Stdout, |text| self.indent.style(text))
            .to_string()
    }

    fn style_message(&self, message: &str) -> String {
        message
            .if_supports_color(Stdout, |text| self.text.style(text))
            .to_string()
    }
}
