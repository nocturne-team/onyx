//! Modrinth OAuth: authorization code flow with a loopback redirect.
//!
//! Modrinth matches redirect URIs exactly (port included) and has no PKCE, so
//! we listen on one of a few fixed, pre-registered ports and send the client
//! secret with the token exchange.

use std::future::Future;
use std::time::Duration;

use reqwest::Url;
use serde::Deserialize;

use super::loopback::{bind, random_state, wait_for_code};
use super::{AuthError, MODRINTH_SCOPES, ModrinthToken, now_secs};
use crate::config;

/// Must match the redirect URIs registered on the Modrinth OAuth app.
const REDIRECT_PORTS: &[u16] = &[47823, 47824, 47825];
const AUTHORIZE_URL: &str = "https://modrinth.com/auth/authorize";
const TOKEN_URL: &str = "https://api.modrinth.com/_internal/oauth/token";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub authorize_url: String,
    pub token_url: String,
    pub redirect_ports: Vec<u16>,
    pub timeout: Duration,
}

impl OAuthConfig {
    /// The client baked in at build time, or `None` if the build had no credentials.
    pub fn from_build() -> Option<Self> {
        Some(Self {
            client_id: config::MODRINTH_CLIENT_ID?.to_owned(),
            client_secret: config::MODRINTH_CLIENT_SECRET?.to_owned(),
            authorize_url: AUTHORIZE_URL.to_owned(),
            token_url: TOKEN_URL.to_owned(),
            redirect_ports: REDIRECT_PORTS.to_vec(),
            timeout: LOGIN_TIMEOUT,
        })
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
}

/// Runs the whole login: opens the browser via `open_browser`, waits for the
/// redirect, and exchanges the code for a token. Resolves early with
/// `AuthError::Cancelled` if `cancel` completes first.
pub async fn login(
    config: &OAuthConfig,
    http: &reqwest::Client,
    open_browser: impl FnOnce(&str) -> Result<(), String>,
    cancel: impl Future<Output = ()>,
) -> Result<ModrinthToken, AuthError> {
    let (listener, port) = bind(&config.redirect_ports).await?;
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let state = random_state();

    let authorize = Url::parse_with_params(
        &config.authorize_url,
        &[
            ("client_id", config.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("scope", &MODRINTH_SCOPES.join(" ")),
            ("state", &state),
        ],
    )
    .expect("authorize URL is valid");
    open_browser(authorize.as_str()).map_err(AuthError::Browser)?;

    let code = tokio::select! {
        result = wait_for_code(&listener, &state, "/callback") => result?,
        () = tokio::time::sleep(config.timeout) => return Err(AuthError::TimedOut),
        () = cancel => return Err(AuthError::Cancelled),
    };
    drop(listener);

    exchange_code(config, http, &code, &redirect_uri).await
}

async fn exchange_code(
    config: &OAuthConfig,
    http: &reqwest::Client,
    code: &str,
    redirect_uri: &str,
) -> Result<ModrinthToken, AuthError> {
    let response = http
        .post(&config.token_url)
        .header("Authorization", &config.client_secret)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", &config.client_id),
        ])
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        return Err(AuthError::TokenExchange {
            status: status.as_u16(),
            body: response.text().await.unwrap_or_default(),
        });
    }
    let token: TokenResponse = response.json().await?;
    Ok(ModrinthToken {
        access_token: token.access_token,
        expires_at: Some(now_secs() + token.expires_in.max(0) as u64),
    })
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use tokio::net::TcpListener;

    use super::*;

    fn test_config(token_url: String) -> OAuthConfig {
        OAuthConfig {
            client_id: "client123".into(),
            client_secret: "secret456".into(),
            authorize_url: "https://modrinth.example/auth/authorize".into(),
            token_url,
            // Port 0 lets the OS pick, so tests can run in parallel.
            redirect_ports: vec![0],
            timeout: Duration::from_secs(10),
        }
    }

    /// Plays the browser: reads the authorize URL, then hits the redirect URI
    /// with the given query, built from the authorize URL's own parameters.
    fn fake_browser(
        make_query: impl FnOnce(&str) -> String + Send + 'static,
    ) -> impl FnOnce(&str) -> Result<(), String> {
        move |authorize: &str| {
            let url = Url::parse(authorize).unwrap();
            let param = |name: &str| {
                url.query_pairs()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.into_owned())
                    .unwrap()
            };
            assert_eq!(param("client_id"), "client123");
            assert_eq!(param("scope"), MODRINTH_SCOPES.join(" "));
            let redirect = format!("{}?{}", param("redirect_uri"), make_query(&param("state")));
            tokio::spawn(async move {
                let _ = reqwest::get(redirect).await;
            });
            Ok(())
        }
    }

    #[tokio::test]
    async fn full_login_exchanges_code_with_client_secret() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(header("authorization", "secret456"))
            .and(body_string_contains("code=abc"))
            .and(body_string_contains("client_id=client123"))
            .and(body_string_contains("grant_type=authorization_code"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"access_token":"mro_token","token_type":"Bearer","expires_in":3600}"#,
            ))
            .expect(1)
            .mount(&server)
            .await;

        let config = test_config(format!("{}/token", server.uri()));
        let token = login(
            &config,
            &reqwest::Client::new(),
            fake_browser(|state| format!("code=abc&state={state}")),
            std::future::pending(),
        )
        .await
        .unwrap();

        assert_eq!(token.access_token, "mro_token");
        let expires_at = token.expires_at.unwrap();
        assert!(expires_at > now_secs() + 3500 && expires_at <= now_secs() + 3600);
    }

    #[tokio::test]
    async fn wrong_state_is_rejected_before_any_token_request() {
        let config = test_config("http://127.0.0.1:1/never-called".into());
        let err = login(
            &config,
            &reqwest::Client::new(),
            fake_browser(|_| "code=abc&state=forged".into()),
            std::future::pending(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AuthError::StateMismatch));
    }

    #[tokio::test]
    async fn user_denying_access_is_reported() {
        let config = test_config("http://127.0.0.1:1/never-called".into());
        let err = login(
            &config,
            &reqwest::Client::new(),
            fake_browser(|state| format!("error=access_denied&state={state}")),
            std::future::pending(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AuthError::Denied(reason) if reason == "access_denied"));
    }

    #[tokio::test]
    async fn cancel_stops_waiting() {
        let config = test_config("http://127.0.0.1:1/never-called".into());
        let err = login(&config, &reqwest::Client::new(), |_| Ok(()), async {})
            .await
            .unwrap_err();
        assert!(matches!(err, AuthError::Cancelled));
    }

    #[tokio::test]
    async fn busy_ports_are_reported() {
        let taken = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = test_config("http://127.0.0.1:1/never-called".into());
        config.redirect_ports = vec![taken.local_addr().unwrap().port()];
        let err = login(
            &config,
            &reqwest::Client::new(),
            |_| Ok(()),
            std::future::pending(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AuthError::PortsInUse(_)));
    }
}
