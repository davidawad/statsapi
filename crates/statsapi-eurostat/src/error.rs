//! Error type for the Eurostat client.

/// Errors returned while requesting or decoding Eurostat and GISCO responses.
#[derive(Debug)]
pub enum Error {
    /// The HTTP client could not be configured or a request failed.
    Http(reqwest::Error),
    /// The server answered with a non-retryable or exhausted-retry status.
    HttpStatus(u16),
    /// Reading a response body failed.
    Io(std::io::Error),
    /// The response body was not valid JSON.
    Json(serde_json::Error),
    /// A JSON response did not have the expected layout.
    Layout(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "eurostat request failed: {error}"),
            Self::HttpStatus(status) => write!(formatter, "eurostat returned HTTP {status}"),
            Self::Io(error) => write!(formatter, "eurostat response read failed: {error}"),
            Self::Json(error) => write!(formatter, "invalid eurostat JSON: {error}"),
            Self::Layout(what) => write!(formatter, "unexpected eurostat layout: {what}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::HttpStatus(_) | Self::Layout(_) => None,
        }
    }
}
