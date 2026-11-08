//! Modrinth API v2 client.

mod types;

use std::sync::Arc;
use std::time::Duration;

use reqwest::{Response, StatusCode};
use serde::de::DeserializeOwned;

use crate::cache::{CachePolicy, ResponseCache};
use crate::config::USER_AGENT;

pub use types::*;

/// API root; paths include the version (`/v2/...`, `/v3/...`). Most calls use
/// v2, but organizations only exist in v3.
pub const API_BASE: &str = "https://api.modrinth.com";

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("could not reach Modrinth: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Modrinth is rate limiting us, try again in {retry_after}s")]
    RateLimited { retry_after: u64 },
    #[error("Modrinth didn't accept the saved login, sign in again")]
    Unauthorized,
    #[error("Modrinth returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("unexpected response from Modrinth: {0}")]
    Decode(#[from] serde_json::Error),
}

/// Cheap to clone; clones share the connection pool and cache.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
    cache: Arc<ResponseCache>,
}

impl Client {
    pub fn new(cache: ResponseCache) -> Self {
        Self::with_base(API_BASE, cache)
    }

    pub fn with_base(base: impl Into<String>, cache: ResponseCache) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .build()
            .expect("TLS backend failed to initialize");
        Self {
            http,
            base: base.into().trim_end_matches('/').to_owned(),
            cache: Arc::new(cache),
        }
    }

    /// The underlying HTTP client, which already sends our User-Agent.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// `cache: None` always goes to the network and stores nothing.
    pub async fn search(
        &self,
        params: &SearchParams,
        cache: Option<CachePolicy>,
    ) -> Result<SearchResults, ApiError> {
        self.get_cached("/v2/search", &params.to_query(), None, cache)
            .await
    }

    /// Every category tag, for all project types.
    pub async fn categories(&self, cache: Option<CachePolicy>) -> Result<Vec<Category>, ApiError> {
        self.get_cached("/v2/tag/category", &[], None, cache).await
    }

    /// The user the token belongs to. Never cached: it's how we check a token still works.
    pub async fn current_user(&self, token: &str) -> Result<User, ApiError> {
        self.get_cached("/v2/user", &[], Some(token), None).await
    }

    /// Projects the user is a direct member of, including drafts, private and
    /// in-review ones when `token` belongs to that user. Doesn't include
    /// projects owned by the user's organizations; see [`Self::all_user_projects`].
    /// Don't persist this cache: it holds private project data.
    pub async fn user_projects(
        &self,
        user_id: &str,
        token: &str,
        cache: Option<CachePolicy>,
    ) -> Result<Vec<Project>, ApiError> {
        debug_assert!(cache.is_none_or(|c| !c.persist));
        self.get_cached(
            &format!("/v2/user/{user_id}/projects"),
            &[],
            Some(token),
            cache,
        )
        .await
    }

    pub async fn user_organizations(
        &self,
        user_id: &str,
        token: &str,
        cache: Option<CachePolicy>,
    ) -> Result<Vec<Organization>, ApiError> {
        self.get_cached(
            &format!("/v3/user/{user_id}/organizations"),
            &[],
            Some(token),
            cache,
        )
        .await
    }

    /// The organization's projects, with `organization_name` filled in.
    /// Hidden ones are only included if `token` belongs to a member.
    pub async fn organization_projects(
        &self,
        organization: &Organization,
        token: &str,
        cache: Option<CachePolicy>,
    ) -> Result<Vec<Project>, ApiError> {
        debug_assert!(cache.is_none_or(|c| !c.persist));
        let projects: Vec<ProjectV3> = self
            .get_cached(
                &format!("/v3/organization/{}/projects", organization.id),
                &[],
                Some(token),
                cache,
            )
            .await?;
        Ok(projects
            .into_iter()
            .map(|p| {
                let mut project = Project::from(p);
                project.organization_name = Some(organization.name.clone());
                project
            })
            .collect())
    }

    /// Everything the user can manage: their own projects plus those of
    /// every organization they're in. Projects showing up in both keep the
    /// organization tag.
    pub async fn all_user_projects(
        &self,
        user_id: &str,
        token: &str,
        cache: Option<CachePolicy>,
    ) -> Result<Vec<Project>, ApiError> {
        let mut projects = Vec::new();
        for organization in self.user_organizations(user_id, token, cache).await? {
            projects.extend(
                self.organization_projects(&organization, token, cache)
                    .await?,
            );
        }
        for project in self.user_projects(user_id, token, cache).await? {
            if !projects.iter().any(|p| p.id == project.id) {
                projects.push(project);
            }
        }
        Ok(projects)
    }

    /// Versions of a project (id or slug), newest first, filtered to the given
    /// loaders and game versions when those aren't empty.
    pub async fn project_versions(
        &self,
        project: &str,
        loaders: &[&str],
        game_versions: &[&str],
        cache: Option<CachePolicy>,
    ) -> Result<Vec<Version>, ApiError> {
        self.project_versions_as(project, loaders, game_versions, None, cache)
            .await
    }

    /// Like [`Self::project_versions`], as the token's user, so versions of
    /// their own non-public projects are included. Don't persist that cache.
    pub async fn project_versions_as(
        &self,
        project: &str,
        loaders: &[&str],
        game_versions: &[&str],
        token: Option<&str>,
        cache: Option<CachePolicy>,
    ) -> Result<Vec<Version>, ApiError> {
        debug_assert!(token.is_none() || cache.is_none_or(|c| !c.persist));
        let mut query = Vec::new();
        if !loaders.is_empty() {
            query.push((
                "loaders",
                serde_json::to_string(loaders).expect("strings serialize"),
            ));
        }
        if !game_versions.is_empty() {
            query.push((
                "game_versions",
                serde_json::to_string(game_versions).expect("strings serialize"),
            ));
        }
        self.get_cached(
            &format!("/v2/project/{project}/version"),
            &query,
            token,
            cache,
        )
        .await
    }

    /// A project's full page, with `author` set to the owning organization
    /// or the team owner (best effort). With a token, the user's own
    /// non-public projects work too; don't persist that cache.
    pub async fn project_page(
        &self,
        id: &str,
        token: Option<&str>,
        cache: Option<CachePolicy>,
    ) -> Result<ProjectPage, ApiError> {
        debug_assert!(token.is_none() || cache.is_none_or(|c| !c.persist));
        let mut page: ProjectPage = self
            .get_cached(&format!("/v2/project/{id}"), &[], token, cache)
            .await?;
        page.author = match &page.organization {
            Some(org) => self
                .get_cached::<OrganizationName>(
                    &format!("/v3/organization/{org}"),
                    &[],
                    None,
                    cache,
                )
                .await
                .ok()
                .map(|o| o.name),
            None => None,
        };
        if page.author.is_none() {
            let members: Vec<TeamMember> = self
                .get_cached(&format!("/v2/project/{id}/members"), &[], token, cache)
                .await
                .unwrap_or_default();
            page.author = members
                .iter()
                .find(|m| m.is_owner)
                .or_else(|| members.first())
                .map(|m| m.user.username.clone());
        }
        Ok(page)
    }

    pub async fn version(&self, id: &str, cache: Option<CachePolicy>) -> Result<Version, ApiError> {
        self.get_cached(&format!("/v2/version/{id}"), &[], None, cache)
            .await
    }

    /// Several projects in one request. Unknown ids are left out.
    pub async fn projects(
        &self,
        ids: &[&str],
        cache: Option<CachePolicy>,
    ) -> Result<Vec<Project>, ApiError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = serde_json::to_string(ids).expect("strings serialize");
        self.get_cached("/v2/projects", &[("ids", ids)], None, cache)
            .await
    }

    /// The versions files with these SHA-1 hashes belong to, keyed by hash.
    /// Files Modrinth doesn't know are left out. Never cached.
    pub async fn versions_from_hashes(
        &self,
        sha1s: &[String],
    ) -> Result<std::collections::HashMap<String, Version>, ApiError> {
        if sha1s.is_empty() {
            return Ok(Default::default());
        }
        let response = self
            .http
            .post(format!("{}/v2/version_files", self.base))
            .json(&serde_json::json!({ "hashes": sha1s, "algorithm": "sha1" }))
            .send()
            .await?;
        let body = check_status(response).await?.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    /// The newest version compatible with `loaders` and `game_versions` for
    /// each project the hashed files belong to, keyed by hash. Empty filters
    /// are left out of the request. Never cached.
    pub async fn latest_versions_from_hashes(
        &self,
        sha1s: &[String],
        loaders: &[&str],
        game_versions: &[&str],
    ) -> Result<std::collections::HashMap<String, Version>, ApiError> {
        if sha1s.is_empty() {
            return Ok(Default::default());
        }
        let mut body = serde_json::json!({ "hashes": sha1s, "algorithm": "sha1" });
        if !loaders.is_empty() {
            body["loaders"] = serde_json::json!(loaders);
        }
        if !game_versions.is_empty() {
            body["game_versions"] = serde_json::json!(game_versions);
        }
        let response = self
            .http
            .post(format!("{}/v2/version_files/update", self.base))
            .json(&body)
            .send()
            .await?;
        let body = check_status(response).await?.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    /// GETs `path`, serving from cache when the entry is fresh enough. If the
    /// request fails and any cached copy exists, that copy is returned instead.
    async fn get_cached<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
        token: Option<&str>,
        cache: Option<CachePolicy>,
    ) -> Result<T, ApiError> {
        let url = format!("{}{path}", self.base);
        let key = cache_key(&url, query);

        if let Some(policy) = cache {
            if let Some(body) = self.cache.get(&key, policy.max_age) {
                match serde_json::from_str(&body) {
                    Ok(value) => return Ok(value),
                    Err(e) => tracing::debug!("ignoring undecodable cache entry: {e}"),
                }
            }
        }

        let result = async {
            let mut request = self.http.get(&url).query(query);
            if let Some(token) = token {
                // Raw token, no "Bearer" prefix.
                request = request.header("Authorization", token);
            }
            let response = request.send().await?;
            let body = check_status(response).await?.text().await?;
            let value = serde_json::from_str(&body)?;
            Ok::<_, ApiError>((value, body))
        }
        .await;

        match result {
            Ok((value, body)) => {
                if let Some(policy) = cache {
                    self.cache.put(&key, body, policy.persist);
                }
                Ok(value)
            }
            Err(e @ (ApiError::Http(_) | ApiError::RateLimited { .. })) if cache.is_some() => {
                match self
                    .cache
                    .get_stale(&key)
                    .and_then(|b| serde_json::from_str(&b).ok())
                {
                    Some(value) => {
                        tracing::warn!("serving stale cache for {path}: {e}");
                        Ok(value)
                    }
                    None => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }
}

async fn check_status(response: Response) -> Result<Response, ApiError> {
    let status = response.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        return Err(ApiError::RateLimited { retry_after });
    }
    if status == StatusCode::UNAUTHORIZED {
        return Err(ApiError::Unauthorized);
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::Status {
            status: status.as_u16(),
            body,
        });
    }
    Ok(response)
}

fn cache_key(url: &str, query: &[(&str, String)]) -> String {
    let mut key = url.to_owned();
    for (k, v) in query {
        key.push_str(&format!("&{k}={v}"));
    }
    key
}

#[cfg(test)]
mod tests;
