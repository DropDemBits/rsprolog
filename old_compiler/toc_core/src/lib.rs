//! Core utilities shared by most of toc's crates
//! Includes
//! - Common location information
//! - Common status reporting facility
use std::fmt::{self, Arguments, Display, Formatter};

pub mod token;

/// Location of a token in a file/text stream
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct Location {
    /// Starting byte of a lexeme
    // For now, export start and end to allow it to modify bounds
    // Probably should use a token joining thing
    pub start: usize,
    /// Ending byte of a lexeme
    pub end: usize,
    /// Starting line number of the lexeme
    pub line: usize,
    /// Starting column of the lexeme
    pub column: usize,
    /// The column span of the lexeme, in columns
    pub width: usize,
    /// The line span of the lexeme, in lines
    pub line_span: usize,
}

impl Location {
    #[must_use]
    pub fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            line: 1,
            column: 1,
            width: 0,
            line_span: 0,
        }
    }

    /// Creates a new location spanning from this location to the other location, including the end location's token
    #[must_use = "creates a new spanning Location instead of mutating the current one"]
    pub fn span_to(&self, other: &Self) -> Self {
        // Compute the correct span ends
        let line_span = other.line.saturating_sub(self.line);
        let width = if line_span > 0 {
            // More than one line, take the end span's column (plus the width, minus 1)
            other.column.saturating_add(other.width).saturating_sub(1)
        } else {
            // On the same line, span between the columns (including the widths)
            other
                .column
                .saturating_add(other.width)
                .saturating_sub(self.column)
        };

        Self {
            start: self.start,
            line: self.line,
            column: self.column,
            end: other.end,
            width,
            line_span,
        }
    }

    /// Advances the location to the next lexeme, beginning a new lexeme
    pub fn step(&mut self) {
        self.start = self.end;
        self.column += self.width;
        self.line += self.line_span;
        self.width = 0;
        self.line_span = 0;
    }

    /// Advances the column location by the give amount of steps
    pub fn columns(&mut self, steps: usize) {
        self.width += steps;
    }

    /// Advances the line location by the give amount of steps, as well as resetting the column
    pub fn lines(&mut self, steps: usize) {
        self.column = 1;
        self.width = 0;
        self.line_span += steps;
    }

    /// Moves the end of the lexeme to the given byte index
    pub fn current_to(&mut self, next_end: usize) {
        self.end = next_end;
    }

    /// Moves the end of the lexeme to the end of the given location
    pub fn current_to_other(&mut self, other: &Self) {
        self.end = other.end;
    }

    /// Gets the lexeme corresponding to this location
    #[must_use = "will use the lexeme produced from the source"]
    pub fn get_lexeme<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

impl Default for Location {
    fn default() -> Self {
        Self::new()
    }
}

// Status reporter
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ReportKind {
    Error,
    Warning,
}

impl Display for ReportKind {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        match self {
            ReportKind::Error => f.write_str("error"),
            ReportKind::Warning => f.write_str("warn"),
        }
    }
}

#[derive(Debug)]
pub struct ReportMessage {
    kind: ReportKind,
    at: Location,
    message: String,
}

/// Common status reporter
#[derive(Debug)]
pub struct StatusReporter {
    /// If the reporter has reported an error
    has_error: bool,
    /// Messages to report
    messages: Vec<ReportMessage>,
}

impl StatusReporter {
    pub fn new() -> Self {
        Self {
            has_error: false,
            messages: vec![],
        }
    }

    /// Reports messages given by an iterator
    ///
    /// # Returns
    /// Returns true if any errors were reported
    pub fn report_messages<'m>(
        messages: impl Iterator<Item = &'m ReportMessage>,
        mute_warnings: bool,
    ) -> bool {
        use std::cmp::Ordering;

        let mut messages: Vec<&ReportMessage> = messages.collect();

        messages.sort_by(|a, b| {
            // All errors before warnings
            let ordering = a.kind.cmp(&b.kind);

            if ordering == Ordering::Equal {
                a.at.start.cmp(&b.at.start)
            } else {
                ordering
            }
        });

        let mut reported_errors = false;

        for reporting in messages {
            if mute_warnings && reporting.kind == ReportKind::Warning {
                continue;
            }

            reported_errors |= reporting.kind == ReportKind::Error;

            let end_column = reporting.at.column + reporting.at.width;

            eprintln!(
                "{} line:{} column:{}-{} {}",
                reporting.kind,
                reporting.at.line,
                reporting.at.column,
                end_column,
                reporting.message
            );
        }

        reported_errors
    }

    /// Takes all stored messages
    pub fn take_messages(&mut self) -> Vec<ReportMessage> {
        std::mem::take(&mut self.messages)
    }

    fn report_at(&mut self, reporting: ReportMessage) {
        self.messages.push(reporting);
    }

    pub fn report_error<'k>(&'k mut self, at: &Location, message: Arguments<'k>) {
        self.has_error = true;

        self.report_at(ReportMessage {
            kind: ReportKind::Error,
            at: *at,
            message: message.to_string(),
        });
    }

    pub fn report_warning<'k>(&'k mut self, at: &Location, message: Arguments<'k>) {
        self.report_at(ReportMessage {
            kind: ReportKind::Warning,
            at: *at,
            message: message.to_string(),
        });
    }

    pub fn has_error(&self) -> bool {
        self.has_error
    }
}

impl Default for StatusReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Anything which reports messages
pub trait MessageSource {
    /// Takes reported messages from an integrated reporter
    fn take_reported_messages(&mut self) -> Vec<ReportMessage>;
}
