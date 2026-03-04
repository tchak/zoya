use tower::util::BoxCloneService;

/// Parsed fetch inputs extracted from JS arguments.
#[derive(Clone)]
pub struct FetchInput {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// Structured output from a fetch operation.
pub struct FetchOutput {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Default for FetchOutput {
    fn default() -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }
}

/// Errors that can occur during a fetch operation.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FetchError {
    #[error("network error: {0}")]
    Network(String),
    #[error("unsupported URL scheme: {0}")]
    UnsupportedScheme(String),
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("request timed out")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

/// A composable fetch service built on Tower.
///
/// All fetch call sites must provide a `FetchService`. The default
/// implementation (`HttpFetchService` in `zoya-fetch`) handles HTTP(S)
/// URLs. Middleware layers can be added to intercept custom schemes
/// (e.g. `zoya://`), add logging, authentication, etc.
pub type FetchService = BoxCloneService<FetchInput, FetchOutput, FetchError>;
