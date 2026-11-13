//! Errors for the files idf reads (`idf.toml`, `idf.lock`), rendered by
//! miette with the offending line and column highlighted.

use miette::{Diagnostic, NamedSource, SourceSpan};
use std::ops::Range;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum FileError {
    #[error("could not read {}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    #[diagnostic(transparent)]
    Invalid(Box<InvalidFile>),
}

/// A file that was read but is malformed or fails validation.
#[derive(Debug, thiserror::Error, Diagnostic)]
#[error("{message}")]
#[diagnostic(code(idf::invalid_file))]
pub struct InvalidFile {
    pub message: String,
    #[source_code]
    pub src: NamedSource<String>,
    #[label("{label}")]
    pub span: Option<SourceSpan>,
    pub label: String,
    #[help]
    pub help: Option<String>,
}

impl FileError {
    pub fn invalid(&self) -> Option<&InvalidFile> {
        match self {
            FileError::Invalid(inner) => Some(inner),
            FileError::Io { .. } => None,
        }
    }

    pub(crate) fn with_help(mut self, text: impl Into<String>) -> Self {
        if let FileError::Invalid(inner) = &mut self {
            inner.help = Some(text.into());
        }
        self
    }
}

/// Builder for [`FileError::Invalid`] that keeps call sites short.
pub(crate) struct Source<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

impl Source<'_> {
    pub fn error(
        &self,
        message: impl Into<String>,
        span: Option<Range<usize>>,
        label: impl Into<String>,
    ) -> FileError {
        FileError::Invalid(Box::new(InvalidFile {
            message: message.into(),
            src: NamedSource::new(self.name, self.text.to_owned()),
            span: span.map(Into::into),
            label: label.into(),
            help: None,
        }))
    }

    /// Turn a TOML parse/deserialize error into a diagnostic.
    pub fn toml_error(&self, err: toml::de::Error) -> FileError {
        // toml's message already says what was expected; keep it as the label
        // so it sits next to the highlighted span.
        let message = err.message().trim().to_owned();
        self.error(format!("invalid {}", self.file_name()), err.span(), message)
    }

    fn file_name(&self) -> &str {
        std::path::Path::new(self.name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(self.name)
    }
}

pub(crate) fn read(path: &std::path::Path) -> Result<String, FileError> {
    std::fs::read_to_string(path).map_err(|source| FileError::Io {
        path: path.to_owned(),
        source,
    })
}
