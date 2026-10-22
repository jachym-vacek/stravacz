use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{self, HeaderMap, HeaderValue};
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::model::{Account, Change, MealId, Menu, Money};
use crate::raw::{self, LoginResponse};

/// Where the web app lives.
pub const DEFAULT_BASE_URL: &str = "https://app.strava.cz";

/// The API is picky about clients that do not look like its own web app.
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
                          (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36";
const LANGUAGE: &str = "EN";
const LOGIN_PAGE: &str = "/en/prihlasit-se?jidelna";

/// What is needed to log in.
#[derive(Clone)]
pub struct Credentials {
    /// The canteen's number, as shown on strava.cz.
    pub canteen: String,
    pub username: String,
    pub password: String,
}

impl Credentials {
    pub fn new(
        canteen: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            canteen: canteen.into(),
            username: username.into(),
            password: password.into(),
        }
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("canteen", &self.canteen)
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

/// A connection to Strava.cz. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: Arc<str>,
}

impl Client {
    pub fn new() -> Result<Self> {
        Self::with_base_url(DEFAULT_BASE_URL)
    }

    /// Talk to a different host, e.g. a mock server in tests.
    pub fn with_base_url(base_url: &str) -> Result<Self> {
        let invalid = || Error::InvalidBaseUrl(base_url.to_owned());
        let base_url = base_url.trim_end_matches('/');
        let url = reqwest::Url::parse(base_url).map_err(|_| invalid())?;
        if !matches!(url.scheme(), "http" | "https") || !url.has_host() {
            return Err(invalid());
        }
        let header_value = |value: &str| HeaderValue::from_str(value).map_err(|_| invalid());

        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
        headers.insert(
            header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("en-US,en;q=0.9,cs;q=0.8"),
        );
        headers.insert(header::ORIGIN, header_value(base_url)?);
        headers.insert(
            header::REFERER,
            header_value(&format!("{base_url}{LOGIN_PAGE}"))?,
        );

        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .cookie_store(true)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|source| Error::Transport {
                endpoint: "client",
                source,
            })?;

        Ok(Self {
            http,
            base_url: base_url.into(),
        })
    }

    pub async fn login(&self, credentials: &Credentials) -> Result<Session> {
        self.warm_up().await;

        let response = self
            .post(
                "login",
                &json!({
                    "cislo": credentials.canteen,
                    "jmeno": credentials.username,
                    "heslo": credentials.password,
                    "zustatPrihlasen": true,
                    "environment": "W",
                    "lang": LANGUAGE,
                }),
            )
            .await?;

        let login: LoginResponse =
            serde_json::from_value(response).map_err(|err| Error::UnexpectedResponse {
                endpoint: "login",
                reason: err.to_string(),
            })?;
        if login.sid.is_empty() {
            return Err(Error::Authentication(
                "login returned no session id".to_owned(),
            ));
        }

        tracing::info!(canteen = credentials.canteen, "logged in");
        Ok(Session {
            client: self.clone(),
            inner: Arc::new(SessionInner {
                account: login
                    .uzivatel
                    .into_account(&credentials.username, &credentials.canteen),
                sid: login.sid,
                s5url: login.s5url,
            }),
        })
    }

    /// Load the login page once so the API sees a browser-like session.
    async fn warm_up(&self) {
        let url = format!("{}{LOGIN_PAGE}", self.base_url);
        if let Err(err) = self.http.get(url).send().await {
            tracing::debug!(%err, "session warm-up failed");
        }
    }

    /// POST to `/api/{endpoint}` and decode the API's error envelope.
    ///
    /// Strava.cz reports errors with HTTP 555 and a body like
    /// `{"state": "error", "number": 20, "message": "..."}`.
    async fn post(&self, endpoint: &'static str, body: &Value) -> Result<Value> {
        let transport = |source| Error::Transport { endpoint, source };

        let response = self
            .http
            .post(format!("{}/api/{endpoint}", self.base_url))
            .header(header::CONTENT_TYPE, "text/plain;charset=UTF-8")
            .body(body.to_string())
            .send()
            .await
            .map_err(transport)?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(transport)?;

        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| Error::UnexpectedResponse {
                endpoint,
                reason: format!("HTTP {status} with a non-JSON body"),
            })?;

        if value
            .get("state")
            .and_then(Value::as_str)
            .is_some_and(|state| state.eq_ignore_ascii_case("error"))
        {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("the request was rejected")
                .to_owned();
            return Err(Error::from_api(
                value.get("number").and_then(Value::as_i64),
                message,
            ));
        }
        if !status.is_success() {
            return Err(Error::UnexpectedResponse {
                endpoint,
                reason: format!("HTTP {status}"),
            });
        }

        tracing::debug!(endpoint, %status, "api call");
        Ok(value)
    }
}

/// A logged-in session. Cheap to clone, so it can be moved into tasks.
#[derive(Clone)]
pub struct Session {
    client: Client,
    inner: Arc<SessionInner>,
}

struct SessionInner {
    sid: String,
    s5url: String,
    account: Account,
}

/// The outcome of [`Session::submit`].
#[derive(Debug, Clone)]
pub struct Submission {
    /// The menu as the canteen sees it after the submission.
    pub menu: Menu,
    /// The balance reported by the canteen, if it sent one.
    pub balance: Option<Money>,
    /// Changes the canteen accepted but did not apply.
    pub rejected: Vec<MealId>,
}

impl Session {
    /// The account as it was at login.
    #[must_use]
    pub fn account(&self) -> &Account {
        &self.inner.account
    }

    pub async fn menu(&self) -> Result<Menu> {
        let SessionInner {
            sid,
            s5url,
            account,
        } = &*self.inner;
        // The web app sends the balance it knows; this is the one from login and
        // goes stale after orders. The server does not appear to rely on it.
        #[allow(clippy::cast_precision_loss)] // Informational only, precision does not matter.
        let balance = account.balance.hundredths() as f64 / 100.0;
        let response = self
            .client
            .post(
                "objednavky",
                &json!({
                    "cislo": account.canteen_number,
                    "sid": sid,
                    "s5url": s5url,
                    "lang": LANGUAGE,
                    "konto": balance,
                    "podminka": "",
                    "ignoreCert": false,
                }),
            )
            .await?;
        Ok(raw::parse_menu(&response))
    }

    /// Apply all `changes` in one transaction and verify them against a fresh menu.
    ///
    /// If any step fails, the staged changes are discarded on the server.
    pub async fn submit(&self, changes: &[Change]) -> Result<Submission> {
        let balance = if changes.is_empty() {
            None
        } else {
            match self.stage_and_save(changes).await {
                Ok(balance) => balance,
                Err(err) => {
                    if let Err(discard_err) = self.discard_staged().await {
                        tracing::warn!(%discard_err, "failed to discard staged changes");
                    }
                    return Err(err);
                }
            }
        };

        let menu = self.menu().await?;
        let rejected = changes
            .iter()
            .filter(|change| {
                menu.meal(change.meal)
                    .is_none_or(|meal| meal.ordered != change.ordered)
            })
            .map(|change| change.meal)
            .collect();

        Ok(Submission {
            menu,
            balance,
            rejected,
        })
    }

    pub async fn logout(&self) -> Result<()> {
        self.client
            .post("logOut", &self.payload(json!({})))
            .await
            .map(drop)
    }

    async fn stage_and_save(&self, changes: &[Change]) -> Result<Option<Money>> {
        // Cancellations first, so the freed credit can pay for the new orders.
        let mut changes = changes.to_vec();
        changes.sort_by_key(|change| change.ordered);

        let mut balance = None;
        for change in changes {
            let response = self
                .client
                .post(
                    "pridejJidloS5",
                    &self.payload(json!({
                        "veta": change.meal.to_string(),
                        "pocet": if change.ordered { "1" } else { "0" },
                    })),
                )
                .await?;
            balance = raw::balance(&response).or(balance);
        }

        let response = self
            .client
            .post("saveOrders", &self.payload(json!({ "xml": null })))
            .await?;
        Ok(raw::balance(&response).or(balance))
    }

    /// Reload the server-side state, dropping anything staged but not saved.
    async fn discard_staged(&self) -> Result<()> {
        self.client
            .post(
                "nactiVlastnostiPA",
                &self.payload(json!({
                    "getText": true,
                    "checkVersion": true,
                    "resetTables": true,
                    "frontendFunction": "refreshInformations",
                })),
            )
            .await
            .map(drop)
    }

    /// The fields every write endpoint expects, merged with `extra`.
    fn payload(&self, mut extra: Value) -> Value {
        let common = json!({
            "cislo": self.inner.account.canteen_number,
            "sid": self.inner.sid,
            "url": self.inner.s5url,
            "lang": LANGUAGE,
            "ignoreCert": "false",
        });
        if let (Value::Object(target), Value::Object(common)) = (&mut extra, common) {
            target.extend(common);
        }
        extra
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("account", &self.inner.account)
            .finish_non_exhaustive()
    }
}
