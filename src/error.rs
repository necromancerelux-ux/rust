//! Error plumbing.
//!
//! Poise wants a boxed `Error`; everything internal uses `anyhow` for context
//! and the few recoverable cases get a typed [`OmniError`].

use std::fmt;

/// Recoverable, user-facing failures.
#[derive(Debug, thiserror::Error)]
pub enum OmniError {
    #[error("this feature is not configured on this instance")]
    NotConfigured,

    #[error("you need the {0} permission to use this")]
    MissingPermission(&'static str),

    #[error("that player ID does not look valid: {0}")]
    BadPlayerId(String),

    #[error("nothing is playing right now")]
    NothingPlaying,

    #[error("you must be in a voice channel first")]
    NotInVoice,

    #[error("I could not join {0}")]
    VoiceJoin(String),

    #[error("lookup failed: {0}")]
    Lookup(String),

    #[error("oracle rejected the session (HTTP {0})")]
    OracleAuth(u16),

    /// Boxed: serenity's error type is large and we return `OmniError` by
    /// value from every fallible helper, so keeping it small matters.
    #[error("discord rejected the request: {0}")]
    Discord(Box<serenity::Error>),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<serenity::Error> for OmniError {
    fn from(error: serenity::Error) -> Self {
        Self::Discord(Box::new(error))
    }
}

impl OmniError {
    /// Convert into the boxed error type poise expects while keeping the
    /// display text identical.
    ///
    /// anyhow already converts `StdError` types into `anyhow::Error` via its
    /// blanket `impl<E> From<E>`, so we do not provide our own.
    pub fn boxed(self) -> anyhow::Error {
        anyhow::Error::new(self)
    }
}

/// Pretty-print an error chain into a single line, used for logging.
pub fn chain(err: &anyhow::Error) -> String {
    let mut out = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        out.push_str(" -> ");
        out.push_str(&cause.to_string());
        source = cause.source();
    }
    out
}

/// Helper so `format!("{err:#}")` never produces an empty string.
pub fn display_anyhow(err: &anyhow::Error) -> String {
    let rendered = format!("{err:#}");
    if rendered.trim().is_empty() {
        format!("{:?}", err)
    } else {
        rendered
    }
}

/// Truncate a string to `limit` characters, appending an ellipsis.
pub fn truncate(input: &str, limit: usize) -> String {
    if input.chars().count() <= limit {
        return input.to_string();
    }
    let mut out: String = input.chars().take(limit.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Wrapper that renders an `OmniError` for a Discord reply.
pub struct UserFacing<'a>(pub &'a anyhow::Error);

impl fmt::Display for UserFacing<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(omni) = self.0.downcast_ref::<OmniError>() {
            write!(f, "{omni}")
        } else {
            write!(f, "something went wrong — the error has been logged")
        }
    }
}

/// SQL helpers: turn an optional row into a "not found" error.
pub fn required<T>(value: Option<T>, what: &str) -> Result<T, OmniError> {
    value.ok_or_else(|| OmniError::Other(anyhow::anyhow!("{what} was not found")))
}

const GENERIC_USER_ERROR: &str = "something went wrong — the error has been logged";

fn is_internal(err: &(dyn std::error::Error + 'static)) -> bool {
    err.is::<sqlx::Error>()
        || err.is::<serenity::Error>()
        || err.is::<reqwest::Error>()
        || err.is::<std::io::Error>()
        || err.is::<serde_json::Error>()
}

/// Text that is safe to show to a Discord user for `err`.
///
/// Messages we wrote ourselves (`anyhow!("enter a number…")`, [`OmniError`] variants other
/// than database/Discord) are shown; anything that wraps a database, HTTP, Discord, I/O or
/// JSON error is replaced by a generic line, because those carry SQL, URLs and internals.
/// The full error should still be logged by the caller.
pub fn public<E: fmt::Display + 'static>(err: &E) -> String {
    let any: &dyn std::any::Any = err;
    if let Some(error) = any.downcast_ref::<anyhow::Error>() {
        if let Some(omni) = error.downcast_ref::<OmniError>() {
            return match omni {
                OmniError::Database(_) | OmniError::Discord(_) => GENERIC_USER_ERROR.to_string(),
                OmniError::Other(inner) if inner.chain().any(|c| is_internal(c)) => {
                    GENERIC_USER_ERROR.to_string()
                }
                other => truncate(&other.to_string(), 200),
            };
        }
        if error.chain().any(|c| is_internal(c)) {
            return GENERIC_USER_ERROR.to_string();
        }
        return truncate(&error.to_string(), 200);
    }
    if any.is::<sqlx::Error>()
        || any.is::<serenity::Error>()
        || any.is::<reqwest::Error>()
        || any.is::<std::io::Error>()
        || any.is::<serde_json::Error>()
    {
        return GENERIC_USER_ERROR.to_string();
    }
    truncate(&err.to_string(), 200)
}
