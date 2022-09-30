//! [`textwrap`] helpers.

use std::borrow::Cow;

use textwrap::Options;
use textwrap::WordSeparator;
use textwrap::WordSplitter;

/// Get [`textwrap`] options with our settings.
pub fn options<'a>() -> Options<'a> {
    Options::with_termwidth()
        .break_words(false)
        .word_separator(WordSeparator::AsciiSpace)
        .word_splitter(WordSplitter::NoHyphenation)
}

/// Extension trait adding methods to [`textwrap::Options`]
pub trait TextWrapOptionsExt {
    /// Subtract from the `width`.
    fn decrease_width(self, decrease: usize) -> Self;

    /// Wrap the given text into lines.
    fn wrap<'s>(&self, text: &'s str) -> Vec<Cow<'s, str>>;

    /// Wrap the given text into lines and return a `String`.
    ///
    /// Like [`wrap`] but with the lines pre-joined.
    fn fill(&self, text: &str) -> String;
}

impl<'a> TextWrapOptionsExt for Options<'a> {
    fn decrease_width(self, decrease: usize) -> Self {
        Self {
            width: self.width - decrease,
            ..self
        }
    }

    fn wrap<'s>(&self, text: &'s str) -> Vec<Cow<'s, str>> {
        textwrap::wrap(text, self)
    }

    fn fill(&self, text: &str) -> String {
        textwrap::fill(text, self)
    }
}
