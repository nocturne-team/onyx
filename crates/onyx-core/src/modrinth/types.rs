//! Modrinth API types. They deserialize from Modrinth's snake_case and
//! serialize to camelCase for the frontend.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectType {
    Mod,
    Modpack,
    Resourcepack,
    Shader,
}

impl ProjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
            Self::Resourcepack => "resourcepack",
            Self::Shader => "shader",
        }
    }
}

/// Sort order for `/search`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchIndex {
    #[default]
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

impl SearchIndex {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Follows => "follows",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SearchParams {
    pub query: Option<String>,
    pub project_type: Option<ProjectType>,
    /// Matches any of these (OR), e.g. `fabric`, `quilt`.
    pub loaders: Vec<String>,
    /// Matches any of these (OR), e.g. `1.20.1`.
    pub game_versions: Vec<String>,
    pub index: SearchIndex,
    pub offset: u32,
    pub limit: u32,
}

impl Default for SearchParams {
    fn default() -> Self {
        Self {
            query: None,
            project_type: None,
            loaders: Vec::new(),
            game_versions: Vec::new(),
            index: SearchIndex::default(),
            offset: 0,
            limit: 20,
        }
    }
}

impl SearchParams {
    /// Query string pairs for `/search`. Facets are a JSON array of OR-groups
    /// that are ANDed together.
    pub fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut facets: Vec<Vec<String>> = Vec::new();
        if let Some(project_type) = self.project_type {
            facets.push(vec![format!("project_type:{}", project_type.as_str())]);
        }
        if !self.loaders.is_empty() {
            facets.push(
                self.loaders
                    .iter()
                    .map(|l| format!("categories:{l}"))
                    .collect(),
            );
        }
        if !self.game_versions.is_empty() {
            facets.push(
                self.game_versions
                    .iter()
                    .map(|v| format!("versions:{v}"))
                    .collect(),
            );
        }

        let mut query = Vec::new();
        if let Some(q) = self
            .query
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
        {
            query.push(("query", q.to_owned()));
        }
        if !facets.is_empty() {
            query.push((
                "facets",
                serde_json::to_string(&facets).expect("strings always serialize"),
            ));
        }
        query.push(("index", self.index.as_str().to_owned()));
        query.push(("offset", self.offset.to_string()));
        query.push(("limit", self.limit.clamp(1, 100).to_string()));
        query
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub offset: u32,
    pub limit: u32,
    pub total_hits: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct SearchHit {
    pub project_id: String,
    pub slug: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Kept as a string so new project types don't break deserialization.
    pub project_type: String,
    pub author: String,
    pub icon_url: Option<String>,
    /// RGB color Modrinth extracted from the icon.
    pub color: Option<u32>,
    pub downloads: u64,
    pub follows: u64,
    /// ISO 8601.
    pub date_modified: String,
    #[serde(default)]
    pub display_categories: Vec<String>,
    #[serde(default)]
    pub versions: Vec<String>,
    pub featured_gallery: Option<String>,
    #[serde(default)]
    pub gallery: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct User {
    pub id: String,
    pub username: String,
    pub avatar_url: Option<String>,
}

/// Review/visibility state of a project. Unknown values from newer API
/// versions become `Unknown` instead of failing the whole response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    Approved,
    Archived,
    Rejected,
    Draft,
    Unlisted,
    Processing,
    Withheld,
    Scheduled,
    Private,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct GalleryImage {
    pub url: String,
    #[serde(default)]
    pub featured: bool,
}

/// A full project from `/project` and `/user/{id}/projects`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct Project {
    pub id: String,
    pub slug: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub project_type: String,
    pub status: ProjectStatus,
    /// What the owner asked for once review passes.
    pub requested_status: Option<String>,
    pub icon_url: Option<String>,
    pub color: Option<u32>,
    pub downloads: u64,
    pub followers: u64,
    /// ISO 8601.
    pub updated: String,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub gallery: Vec<GalleryImage>,
    /// Set by us, not Modrinth, for projects listed via an organization.
    #[serde(default)]
    pub organization_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct Organization {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub icon_url: Option<String>,
}

/// v3 shape of a project. Only used for organization listings, which v2
/// doesn't have; converted to [`Project`] right away.
#[derive(Debug, Deserialize)]
pub(crate) struct ProjectV3 {
    id: String,
    slug: Option<String>,
    name: String,
    #[serde(default)]
    summary: String,
    /// Null for drafts that haven't uploaded a version yet.
    project_types: Option<Vec<String>>,
    status: ProjectStatus,
    requested_status: Option<String>,
    icon_url: Option<String>,
    color: Option<u32>,
    downloads: u64,
    followers: u64,
    updated: String,
    #[serde(default)]
    loaders: Vec<String>,
    game_versions: Option<Vec<String>>,
    #[serde(default)]
    gallery: Vec<GalleryImage>,
}

impl From<ProjectV3> for Project {
    fn from(p: ProjectV3) -> Self {
        Self {
            id: p.id,
            slug: p.slug,
            title: p.name,
            description: p.summary,
            project_type: p
                .project_types
                .and_then(|types| types.into_iter().next())
                .unwrap_or_else(|| "project".to_owned()),
            status: p.status,
            requested_status: p.requested_status,
            icon_url: p.icon_url,
            color: p.color,
            downloads: p.downloads,
            followers: p.followers,
            updated: p.updated,
            loaders: p.loaders,
            game_versions: p.game_versions.unwrap_or_default(),
            gallery: p.gallery,
            organization_name: None,
        }
    }
}

/// A project version from `/project/{id}/version`, `/version/{id}` or
/// `/version_files`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    /// `release`, `beta` or `alpha`.
    pub version_type: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    pub files: Vec<VersionFile>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    pub date_published: String,
}

impl Version {
    /// The file marked primary, or the first one.
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct VersionFile {
    pub hashes: FileHashes,
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileHashes {
    pub sha1: String,
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct Dependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    pub file_name: Option<String>,
    /// `required`, `optional`, `incompatible` or `embedded`.
    pub dependency_type: String,
}
