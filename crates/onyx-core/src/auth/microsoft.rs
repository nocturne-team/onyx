//! Minecraft sign-in: Microsoft OAuth (auth code + PKCE, loopback redirect),
//! then Xbox Live, XSTS and Minecraft services.

use std::future::Future;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::loopback::{bind, random_state, wait_for_code};
use super::{AuthError, now_secs};
use crate::config;

const AUTHORIZE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT_URL: &str = "https://api.minecraftservices.com";
const SCOPES: &str = "XboxLive.signin offline_access";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone)]
pub struct MsaConfig {
    pub client_id: String,
    pub authorize_url: String,
    pub token_url: String,
    pub device_code_url: String,
    pub xbl_url: String,
    pub xsts_url: String,
    /// Base of `/authentication/login_with_xbox` and `/minecraft/profile`.
    pub minecraft_url: String,
    pub timeout: Duration,
}

impl MsaConfig {
    /// The client ID baked in at build time, or `None` if the build had none.
    pub fn from_build() -> Option<Self> {
        Some(Self {
            client_id: config::MSA_CLIENT_ID?.to_owned(),
            authorize_url: AUTHORIZE_URL.to_owned(),
            token_url: TOKEN_URL.to_owned(),
            device_code_url: DEVICE_CODE_URL.to_owned(),
            xbl_url: XBL_URL.to_owned(),
            xsts_url: XSTS_URL.to_owned(),
            minecraft_url: MINECRAFT_URL.to_owned(),
            timeout: LOGIN_TIMEOUT,
        })
    }
}

/// What gets remembered about an account outside the keyring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftAccount {
    /// Profile UUID without dashes.
    pub id: String,
    pub name: String,
    pub skin_url: Option<String>,
    /// Local-only account without Microsoft sign-in (see `auth::offline`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub offline: bool,
}

/// A signed-in account with live tokens.
#[derive(Debug, Clone)]
pub struct MinecraftSession {
    pub account: MinecraftAccount,
    pub access_token: String,
    /// Unix seconds.
    pub expires_at: u64,
    pub xuid: String,
    /// Microsoft refresh token. Microsoft rotates these, so save the newest one.
    pub refresh_token: String,
}

impl MinecraftSession {
    /// Treats tokens within five minutes of expiring as expired.
    pub fn is_expired_now(&self) -> bool {
        now_secs() + 5 * 60 >= self.expires_at
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MsaError {
    #[error("this build has no Microsoft client ID configured")]
    NotConfigured,
    #[error(transparent)]
    Login(#[from] AuthError),
    #[error("could not reach {service}: {source}")]
    Http {
        service: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("{service} returned HTTP {status}: {body}")]
    Service {
        service: &'static str,
        status: u16,
        body: String,
    },
    #[error("your Microsoft login expired, sign in again")]
    SignInAgain,
    #[error(
        "this Microsoft account has no Xbox profile yet; sign in once at xbox.com to create one"
    )]
    NoXboxAccount,
    #[error("this account belongs to a child; an adult has to add it to a Microsoft family first")]
    ChildAccount,
    #[error("Xbox Live isn't available in this account's country or region")]
    XboxUnavailable,
    #[error("Xbox Live refused this account (XErr {0})")]
    Xbox(u64),
    #[error("this Microsoft account doesn't own Minecraft: Java Edition")]
    NoMinecraft,
}

#[derive(Deserialize)]
struct MsToken {
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize)]
struct MsTokenError {
    error: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxToken {
    token: String,
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<XboxUser>,
}

#[derive(Deserialize)]
struct XboxUser {
    uhs: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XstsError {
    x_err: u64,
}

#[derive(Deserialize)]
struct MinecraftToken {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct Profile {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<Skin>,
}

#[derive(Deserialize)]
struct Skin {
    state: String,
    url: String,
}

/// Opens Microsoft's sign-in page via `open_browser`, waits for the redirect,
/// and signs in to Minecraft with the result.
pub async fn login(
    config: &MsaConfig,
    http: &reqwest::Client,
    open_browser: impl FnOnce(&str) -> Result<(), String>,
    cancel: impl Future<Output = ()>,
) -> Result<MinecraftSession, MsaError> {
    // Microsoft accepts any port on 127.0.0.1, but only with no path.
    let (listener, port) = bind(&[0]).await?;
    let redirect_uri = format!("http://127.0.0.1:{port}");
    let state = random_state();
    let verifier = pkce_verifier();

    let authorize = Url::parse_with_params(
        &config.authorize_url,
        &[
            ("client_id", config.client_id.as_str()),
            ("response_type", "code"),
            ("redirect_uri", &redirect_uri),
            ("scope", SCOPES),
            ("state", &state),
            ("code_challenge", &pkce_challenge(&verifier)),
            ("code_challenge_method", "S256"),
            // Lets the user pick a different account when adding another one.
            ("prompt", "select_account"),
        ],
    )
    .expect("authorize URL is valid");
    open_browser(authorize.as_str()).map_err(AuthError::Browser)?;

    let code = tokio::select! {
        result = wait_for_code(&listener, &state, "/") => result?,
        () = tokio::time::sleep(config.timeout) => return Err(AuthError::TimedOut.into()),
        () = cancel => return Err(AuthError::Cancelled.into()),
    };
    drop(listener);

    let token = ms_token(
        config,
        http,
        &[
            ("client_id", &config.client_id),
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect_uri),
            ("code_verifier", &verifier),
            ("scope", SCOPES),
        ],
    )
    .await?;
    minecraft_session(config, http, token).await
}

/// The code the user types at `verification_uri` in the device code flow.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    /// Secret; only sent back to Microsoft while polling.
    #[serde(skip_serializing)]
    pub device_code: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Starts the device code flow: the user signs in on any device by entering
/// `user_code` at `verification_uri`. Then call [`finish_device_login`].
pub async fn start_device_login(
    config: &MsaConfig,
    http: &reqwest::Client,
) -> Result<DeviceCode, MsaError> {
    let response = http
        .post(&config.device_code_url)
        .form(&[("client_id", config.client_id.as_str()), ("scope", SCOPES)])
        .send()
        .await
        .map_err(|source| MsaError::Http {
            service: "Microsoft",
            source,
        })?;
    read_json(response, "Microsoft").await
}

/// Polls until the user finishes signing in (or the code expires, or
/// `cancel` completes), then signs in to Minecraft.
pub async fn finish_device_login(
    config: &MsaConfig,
    http: &reqwest::Client,
    code: &DeviceCode,
    cancel: impl Future<Output = ()>,
) -> Result<MinecraftSession, MsaError> {
    let poll = async {
        let mut interval = code.interval.max(1);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(code.expires_in);
        loop {
            tokio::time::sleep(Duration::from_secs(interval)).await;
            if tokio::time::Instant::now() >= deadline {
                return Err(MsaError::Login(AuthError::TimedOut));
            }
            let response = http
                .post(&config.token_url)
                .form(&[
                    ("client_id", config.client_id.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", code.device_code.as_str()),
                ])
                .send()
                .await
                .map_err(|source| MsaError::Http {
                    service: "Microsoft",
                    source,
                })?;
            let status = response.status();
            let body = response.text().await.map_err(|source| MsaError::Http {
                service: "Microsoft",
                source,
            })?;
            if status.is_success() {
                let token: MsToken =
                    serde_json::from_str(&body).map_err(|_| MsaError::Service {
                        service: "Microsoft",
                        status: status.as_u16(),
                        body: "unexpected token response".into(),
                    })?;
                return Ok(token);
            }
            match serde_json::from_str::<MsTokenError>(&body).map(|e| e.error) {
                Ok(e) if e == "authorization_pending" => {}
                Ok(e) if e == "slow_down" => interval += 5,
                Ok(e) if e == "authorization_declined" => {
                    return Err(AuthError::Denied("you declined the sign-in".into()).into());
                }
                Ok(e) if e == "expired_token" => return Err(AuthError::TimedOut.into()),
                _ => {
                    return Err(MsaError::Service {
                        service: "Microsoft",
                        status: status.as_u16(),
                        body: truncate(body),
                    });
                }
            }
        }
    };
    let token = tokio::select! {
        result = poll => result?,
        () = cancel => return Err(AuthError::Cancelled.into()),
    };
    minecraft_session(config, http, token).await
}

/// Gets fresh tokens from a saved Microsoft refresh token.
pub async fn refresh(
    config: &MsaConfig,
    http: &reqwest::Client,
    refresh_token: &str,
) -> Result<MinecraftSession, MsaError> {
    let token = ms_token(
        config,
        http,
        &[
            ("client_id", &config.client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", SCOPES),
        ],
    )
    .await?;
    minecraft_session(config, http, token).await
}

fn pkce_verifier() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS random number generator failed");
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

async fn ms_token(
    config: &MsaConfig,
    http: &reqwest::Client,
    form: &[(&str, &str)],
) -> Result<MsToken, MsaError> {
    const SERVICE: &str = "Microsoft";
    let response = http
        .post(&config.token_url)
        .form(form)
        .send()
        .await
        .map_err(|source| MsaError::Http {
            service: SERVICE,
            source,
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|source| MsaError::Http {
        service: SERVICE,
        source,
    })?;
    if status.is_success() {
        return serde_json::from_str(&body).map_err(|_| MsaError::Service {
            service: SERVICE,
            status: status.as_u16(),
            body: "unexpected token response".into(),
        });
    }
    match serde_json::from_str::<MsTokenError>(&body) {
        Ok(e) if e.error == "invalid_grant" => Err(MsaError::SignInAgain),
        _ => Err(MsaError::Service {
            service: SERVICE,
            status: status.as_u16(),
            body: truncate(body),
        }),
    }
}

async fn minecraft_session(
    config: &MsaConfig,
    http: &reqwest::Client,
    ms: MsToken,
) -> Result<MinecraftSession, MsaError> {
    let xbl: XboxToken = post_json(
        http,
        "Xbox Live",
        &config.xbl_url,
        &json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={}", ms.access_token),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }),
    )
    .await?;

    let xsts: XboxToken = match post_json(
        http,
        "Xbox Live",
        &config.xsts_url,
        &json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl.token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }),
    )
    .await
    {
        Err(MsaError::Service {
            status: 401, body, ..
        }) => {
            return Err(
                match serde_json::from_str::<XstsError>(&body).map(|e| e.x_err) {
                    Ok(2148916233) => MsaError::NoXboxAccount,
                    Ok(2148916235) => MsaError::XboxUnavailable,
                    Ok(2148916238) => MsaError::ChildAccount,
                    Ok(code) => MsaError::Xbox(code),
                    Err(_) => MsaError::Service {
                        service: "Xbox Live",
                        status: 401,
                        body,
                    },
                },
            );
        }
        other => other?,
    };
    let uhs = &xsts
        .display_claims
        .xui
        .first()
        .ok_or_else(|| MsaError::Service {
            service: "Xbox Live",
            status: 200,
            body: "no user hash in XSTS token".into(),
        })?
        .uhs;

    let minecraft: MinecraftToken = post_json(
        http,
        "Minecraft",
        &format!("{}/authentication/login_with_xbox", config.minecraft_url),
        &json!({ "identityToken": format!("XBL3.0 x={uhs};{}", xsts.token) }),
    )
    .await?;

    let response = http
        .get(format!("{}/minecraft/profile", config.minecraft_url))
        .bearer_auth(&minecraft.access_token)
        .send()
        .await
        .map_err(|source| MsaError::Http {
            service: "Minecraft",
            source,
        })?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(MsaError::NoMinecraft);
    }
    let profile: Profile = read_json(response, "Minecraft").await?;

    let skin_url = profile
        .skins
        .iter()
        .find(|s| s.state == "ACTIVE")
        .map(|s| s.url.replacen("http://", "https://", 1));
    Ok(MinecraftSession {
        account: MinecraftAccount {
            id: profile.id,
            name: profile.name,
            skin_url,
            offline: false,
        },
        xuid: xuid_from_token(&minecraft.access_token).unwrap_or_else(|| "0".into()),
        access_token: minecraft.access_token,
        expires_at: now_secs() + minecraft.expires_in,
        refresh_token: ms.refresh_token,
    })
}

/// The Minecraft access token is a JWT whose payload carries the Xbox user ID.
fn xuid_from_token(token: &str) -> Option<String> {
    let payload = URL_SAFE_NO_PAD.decode(token.split('.').nth(1)?).ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    match &claims["xuid"] {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

async fn post_json<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    service: &'static str,
    url: &str,
    body: &serde_json::Value,
) -> Result<T, MsaError> {
    let response = http
        .post(url)
        .header("Accept", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|source| MsaError::Http { service, source })?;
    read_json(response, service).await
}

async fn read_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    service: &'static str,
) -> Result<T, MsaError> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|source| MsaError::Http { service, source })?;
    if !status.is_success() {
        return Err(MsaError::Service {
            service,
            status: status.as_u16(),
            body: truncate(body),
        });
    }
    serde_json::from_str(&body).map_err(|e| MsaError::Service {
        service,
        status: status.as_u16(),
        body: format!("unexpected response: {e}"),
    })
}

/// Error bodies end up in the UI; keep them short.
fn truncate(mut body: String) -> String {
    if body.len() > 300 {
        let mut end = 300;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        body.push_str("...");
    }
    body
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_partial_json, body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn test_config(server: &MockServer) -> MsaConfig {
        MsaConfig {
            client_id: "client123".into(),
            authorize_url: "https://login.example/authorize".into(),
            token_url: format!("{}/token", server.uri()),
            device_code_url: format!("{}/devicecode", server.uri()),
            xbl_url: format!("{}/xbl", server.uri()),
            xsts_url: format!("{}/xsts", server.uri()),
            minecraft_url: server.uri(),
            timeout: Duration::from_secs(10),
        }
    }

    fn jwt(payload: &str) -> String {
        format!("h.{}.s", URL_SAFE_NO_PAD.encode(payload))
    }

    /// Mounts the Xbox and Minecraft steps. `xsts` is the XSTS response.
    async fn mount_chain(server: &MockServer, xsts: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path("/xbl"))
            .and(body_partial_json(
                json!({"Properties": {"RpsTicket": "d=ms_access"}}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "Token": "xbl_token",
                "DisplayClaims": {"xui": [{"uhs": "userhash"}]}
            })))
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/xsts"))
            .and(body_partial_json(
                json!({"Properties": {"UserTokens": ["xbl_token"]}}),
            ))
            .respond_with(xsts)
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/authentication/login_with_xbox"))
            .and(body_partial_json(
                json!({"identityToken": "XBL3.0 x=userhash;xsts_token"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": jwt(r#"{"xuid":"2535400000000000"}"#),
                "expires_in": 86400
            })))
            .mount(server)
            .await;
    }

    fn xsts_ok() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({
            "Token": "xsts_token",
            "DisplayClaims": {"xui": [{"uhs": "userhash"}]}
        }))
    }

    async fn mount_profile(server: &MockServer, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path("/minecraft/profile"))
            .and(header(
                "authorization",
                format!("Bearer {}", jwt(r#"{"xuid":"2535400000000000"}"#)).as_str(),
            ))
            .respond_with(response)
            .mount(server)
            .await;
    }

    async fn mount_refresh(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=old_refresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "ms_access",
                "refresh_token": "new_refresh",
                "expires_in": 3600
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn refresh_runs_the_whole_chain() {
        let server = MockServer::start().await;
        mount_refresh(&server).await;
        mount_chain(&server, xsts_ok()).await;
        mount_profile(
            &server,
            ResponseTemplate::new(200).set_body_json(json!({
                "id": "0123456789abcdef0123456789abcdef",
                "name": "Steve",
                "skins": [
                    {"state": "INACTIVE", "url": "http://textures.minecraft.net/texture/old"},
                    {"state": "ACTIVE", "url": "http://textures.minecraft.net/texture/abc"}
                ]
            })),
        )
        .await;

        let session = refresh(
            &test_config(&server),
            &reqwest::Client::new(),
            "old_refresh",
        )
        .await
        .unwrap();
        assert_eq!(session.account.name, "Steve");
        assert_eq!(
            session.account.skin_url.as_deref(),
            Some("https://textures.minecraft.net/texture/abc")
        );
        assert_eq!(session.refresh_token, "new_refresh");
        assert_eq!(session.xuid, "2535400000000000");
        assert!(!session.is_expired_now());
    }

    #[tokio::test]
    async fn no_xbox_profile_is_explained() {
        let server = MockServer::start().await;
        mount_refresh(&server).await;
        mount_chain(
            &server,
            ResponseTemplate::new(401)
                .set_body_json(json!({"Identity": "0", "XErr": 2148916233u64})),
        )
        .await;
        let err = refresh(
            &test_config(&server),
            &reqwest::Client::new(),
            "old_refresh",
        )
        .await
        .unwrap_err();
        assert!(matches!(err, MsaError::NoXboxAccount));
    }

    #[tokio::test]
    async fn not_owning_the_game_is_explained() {
        let server = MockServer::start().await;
        mount_refresh(&server).await;
        mount_chain(&server, xsts_ok()).await;
        mount_profile(&server, ResponseTemplate::new(404)).await;
        let err = refresh(
            &test_config(&server),
            &reqwest::Client::new(),
            "old_refresh",
        )
        .await
        .unwrap_err();
        assert!(matches!(err, MsaError::NoMinecraft));
    }

    #[tokio::test]
    async fn revoked_refresh_tokens_ask_for_a_new_login() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(
                    json!({"error": "invalid_grant", "error_description": "expired"}),
                ),
            )
            .mount(&server)
            .await;
        let err = refresh(
            &test_config(&server),
            &reqwest::Client::new(),
            "old_refresh",
        )
        .await
        .unwrap_err();
        assert!(matches!(err, MsaError::SignInAgain));
    }

    #[tokio::test]
    async fn browser_login_uses_pkce_and_a_pathless_redirect() {
        let server = MockServer::start().await;
        mount_chain(&server, xsts_ok()).await;
        mount_profile(
            &server,
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": "0123456789abcdef0123456789abcdef", "name": "Alex"})),
        )
        .await;

        let challenge = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let seen = challenge.clone();
        let config = test_config(&server);
        let browser = move |authorize: &str| {
            let url = Url::parse(authorize).unwrap();
            let param = |name: &str| {
                url.query_pairs()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.into_owned())
                    .unwrap()
            };
            assert_eq!(param("code_challenge_method"), "S256");
            assert_eq!(param("scope"), SCOPES);
            let redirect = param("redirect_uri");
            assert!(redirect.starts_with("http://127.0.0.1:"));
            assert_eq!(Url::parse(&redirect).unwrap().path(), "/");
            *seen.lock().unwrap() = param("code_challenge");
            let state = param("state");
            tokio::spawn(async move {
                let _ = reqwest::get(format!("{redirect}/?code=the_code&state={state}")).await;
            });
            Ok(())
        };

        // The token request must carry a verifier matching the challenge.
        struct VerifierMatches(std::sync::Arc<std::sync::Mutex<String>>);
        impl wiremock::Match for VerifierMatches {
            fn matches(&self, request: &wiremock::Request) -> bool {
                let form: Vec<(String, String)> =
                    serde_urlencoded_pairs(&String::from_utf8_lossy(&request.body));
                form.iter().any(|(k, v)| {
                    k == "code_verifier" && pkce_challenge(v) == *self.0.lock().unwrap()
                })
            }
        }
        fn serde_urlencoded_pairs(body: &str) -> Vec<(String, String)> {
            Url::parse(&format!("http://x/?{body}"))
                .unwrap()
                .query_pairs()
                .into_owned()
                .collect()
        }
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code=the_code"))
            .and(VerifierMatches(challenge))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "ms_access",
                "refresh_token": "first_refresh"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let session = login(
            &config,
            &reqwest::Client::new(),
            browser,
            std::future::pending(),
        )
        .await
        .unwrap();
        assert_eq!(session.account.name, "Alex");
        assert_eq!(session.refresh_token, "first_refresh");
    }

    // Polls once a second for real, so this takes ~3 s.
    #[tokio::test]
    async fn device_login_polls_until_approved() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/devicecode"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "user_code": "ABCD1234", "device_code": "secret-device",
                "verification_uri": "https://www.microsoft.com/link",
                "expires_in": 900, "interval": 1, "message": "..."
            })))
            .mount(&server)
            .await;
        // Pending twice, then approved.
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("device_code=secret-device"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error": "authorization_pending"})),
            )
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains(
                "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "ms_access", "refresh_token": "device_refresh"
            })))
            .mount(&server)
            .await;
        mount_chain(&server, xsts_ok()).await;
        mount_profile(
            &server,
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": "0123456789abcdef0123456789abcdef", "name": "Alex"})),
        )
        .await;

        let config = test_config(&server);
        let http = reqwest::Client::new();
        let code = start_device_login(&config, &http).await.unwrap();
        assert_eq!(code.user_code, "ABCD1234");
        let session = finish_device_login(&config, &http, &code, std::future::pending())
            .await
            .unwrap();
        assert_eq!(session.refresh_token, "device_refresh");
        assert_eq!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.url.path() == "/token")
                .count(),
            3
        );
    }

    #[test]
    fn pkce_matches_rfc_7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(pkce_verifier().len(), 43);
    }
}
