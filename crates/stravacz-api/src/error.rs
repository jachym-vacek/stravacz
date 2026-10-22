/// A `Result` defaulting to this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Everything that can go wrong while talking to Strava.cz.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Wrong credentials, or the session is no longer valid.
    #[error("{0}")]
    Authentication(String),

    /// The account balance does not cover the order.
    #[error("{0}")]
    InsufficientBalance(String),

    /// Any other error reported by the canteen system.
    #[error("{message}")]
    Api { code: Option<i64>, message: String },

    /// The server address given to [`Client::with_base_url`](crate::Client::with_base_url)
    /// is not an `http(s)` URL.
    #[error("invalid server URL `{0}`")]
    InvalidBaseUrl(String),

    /// The request never got a response.
    #[error("could not reach Strava.cz: {source}")]
    Transport {
        endpoint: &'static str,
        #[source]
        source: reqwest::Error,
    },

    /// The response did not look like anything the web app would receive.
    #[error("unexpected response from `{endpoint}`: {reason}")]
    UnexpectedResponse {
        endpoint: &'static str,
        reason: String,
    },
}

impl Error {
    /// Whether the user has to log in (again) before retrying.
    #[must_use]
    pub fn is_authentication(&self) -> bool {
        matches!(self, Self::Authentication(_))
    }

    /// Map the API's `{"state": "error", "number": .., "message": ..}` envelope.
    pub(crate) fn from_api(code: Option<i64>, message: String) -> Self {
        match code {
            Some(20) => Self::Authentication(message),
            Some(35) => Self::InsufficientBalance(message),
            _ => Self::Api { code, message },
        }
    }
}
