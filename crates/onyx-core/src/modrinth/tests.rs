use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

const SEARCH_BODY: &str = r#"{
  "hits": [{
    "project_id": "AANobbMI",
    "project_type": "mod",
    "slug": "sodium",
    "author": "jellysquid3",
    "title": "Sodium",
    "description": "The fastest rendering optimization mod",
    "categories": ["fabric", "optimization"],
    "display_categories": ["fabric", "optimization"],
    "versions": ["1.20.1", "1.21"],
    "downloads": 55000000,
    "follows": 30000,
    "icon_url": "https://cdn.modrinth.com/data/AANobbMI/icon.png",
    "date_created": "2021-01-03T00:53:34.185936Z",
    "date_modified": "2026-10-30T12:00:00.000000Z",
    "latest_version": "1.21",
    "license": "LicenseRef-Polyform-Shield-1.0.0",
    "client_side": "required",
    "server_side": "unsupported",
    "gallery": ["https://cdn.modrinth.com/data/AANobbMI/images/a.png"],
    "featured_gallery": null,
    "color": 8703084
  }],
  "offset": 0,
  "limit": 1,
  "total_hits": 1234
}"#;

fn mods_params() -> SearchParams {
    SearchParams {
        project_type: Some(ProjectType::Mod),
        limit: 1,
        ..Default::default()
    }
}

#[test]
fn query_builds_facets_as_and_of_or_groups() {
    let params = SearchParams {
        query: Some("  sodium ".into()),
        project_type: Some(ProjectType::Mod),
        loaders: vec!["fabric".into(), "quilt".into()],
        game_versions: vec!["1.20.1".into()],
        index: SearchIndex::Downloads,
        offset: 40,
        limit: 500,
    };
    let query = params.to_query();
    let get = |k: &str| {
        query
            .iter()
            .find(|(key, _)| *key == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(get("query"), Some("sodium"));
    assert_eq!(
        get("facets"),
        Some(
            r#"[["project_type:mod"],["categories:fabric","categories:quilt"],["versions:1.20.1"]]"#
        )
    );
    assert_eq!(get("index"), Some("downloads"));
    assert_eq!(get("offset"), Some("40"));
    assert_eq!(get("limit"), Some("100"));
}

#[test]
fn empty_query_and_no_filters_send_no_query_or_facets() {
    let query = SearchParams {
        query: Some("   ".into()),
        ..Default::default()
    }
    .to_query();
    assert!(query.iter().all(|(k, _)| *k != "query" && *k != "facets"));
}

#[tokio::test]
async fn search_sends_user_agent_and_decodes_hits() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/search"))
        .and(header("user-agent", USER_AGENT))
        .and(query_param("facets", r#"[["project_type:mod"]]"#))
        .respond_with(ResponseTemplate::new(200).set_body_string(SEARCH_BODY))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::with_base(server.uri(), ResponseCache::new(None));
    let results = client.search(&mods_params(), None).await.unwrap();

    assert_eq!(results.total_hits, 1234);
    let hit = &results.hits[0];
    assert_eq!(hit.title, "Sodium");
    assert_eq!(hit.author, "jellysquid3");
    assert_eq!(hit.featured_gallery, None);
    assert_eq!(hit.gallery.len(), 1);
}

#[tokio::test]
async fn cached_search_only_hits_the_network_once() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SEARCH_BODY))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::with_base(server.uri(), ResponseCache::new(None));
    for _ in 0..3 {
        client
            .search(&mods_params(), Some(CachePolicy::SHORT))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn uncached_search_always_hits_the_network() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SEARCH_BODY))
        .expect(2)
        .mount(&server)
        .await;

    let client = Client::with_base(server.uri(), ResponseCache::new(None));
    for _ in 0..2 {
        client.search(&mods_params(), None).await.unwrap();
    }
}

#[tokio::test]
async fn rate_limit_is_reported_with_reset_time() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/search"))
        .respond_with(ResponseTemplate::new(429).insert_header("x-ratelimit-reset", "17"))
        .mount(&server)
        .await;

    let client = Client::with_base(server.uri(), ResponseCache::new(None));
    let err = client.search(&mods_params(), None).await.unwrap_err();
    assert!(matches!(err, ApiError::RateLimited { retry_after: 17 }));
}

#[tokio::test]
async fn stale_cache_is_served_when_rate_limited() {
    let server = MockServer::start().await;
    let client = Client::with_base(server.uri(), ResponseCache::new(None));
    let expired = Some(CachePolicy {
        max_age: Duration::ZERO,
        persist: false,
    });

    Mock::given(path("/v2/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SEARCH_BODY))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    client.search(&mods_params(), expired).await.unwrap();

    Mock::given(path("/v2/search"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    // A zero max age is never fresh, the request gets a 429, and the stale copy wins.
    let results = client.search(&mods_params(), expired).await.unwrap();
    assert_eq!(results.hits[0].title, "Sodium");
}

#[tokio::test]
async fn server_errors_are_not_masked_by_cache() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/search"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad facets"))
        .mount(&server)
        .await;

    let client = Client::with_base(server.uri(), ResponseCache::new(None));
    let err = client
        .search(&mods_params(), Some(CachePolicy::SHORT))
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::Status { status: 400, .. }));
}

#[tokio::test]
async fn user_projects_sends_raw_token_and_decodes_statuses() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/user/u1/projects"))
        .and(header("authorization", "mro_secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"[
              {"id":"p1","slug":"my-mod","title":"My Mod","description":"wip","project_type":"mod",
               "status":"processing","requested_status":"approved","icon_url":null,"color":null,
               "downloads":0,"followers":0,"updated":"2026-11-01T00:00:00Z",
               "loaders":["fabric"],"game_versions":["1.21"],"gallery":[]},
              {"id":"p2","slug":null,"title":"Future","description":"","project_type":"mod",
               "status":"some-new-status","requested_status":null,"icon_url":null,"color":null,
               "downloads":5,"followers":1,"updated":"2026-11-01T00:00:00Z"}
            ]"#,
        ))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::with_base(server.uri(), ResponseCache::new(None));
    let projects = client
        .user_projects("u1", "mro_secret", None)
        .await
        .unwrap();
    assert_eq!(projects[0].status, ProjectStatus::Processing);
    assert_eq!(projects[0].requested_status.as_deref(), Some("approved"));
    assert_eq!(projects[1].status, ProjectStatus::Unknown);
}

#[tokio::test]
async fn expired_token_is_reported_as_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/user"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let client = Client::with_base(server.uri(), ResponseCache::new(None));
    let err = client.current_user("mro_old").await.unwrap_err();
    assert!(matches!(err, ApiError::Unauthorized));
}

#[tokio::test]
async fn all_user_projects_merges_organization_projects() {
    let server = MockServer::start().await;
    Mock::given(path("/v3/user/u1/organizations"))
        .and(header("authorization", "mro_secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"[{"id":"o1","slug":"nocturne","name":"nocturne","icon_url":null,"team_id":"t","members":[]}]"#,
        ))
        .mount(&server)
        .await;
    Mock::given(path("/v3/organization/o1/projects"))
        .and(header("authorization", "mro_secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"[
              {"id":"shared","slug":"bedbugs","name":"Bedbugs","summary":"bugs","project_types":["mod"],
               "status":"processing","requested_status":"unlisted","icon_url":null,"color":null,
               "downloads":23,"followers":1,"updated":"2026-10-18T15:07:18Z","loaders":["fabric"],
               "game_versions":["1.20.1"],"gallery":[]},
              {"id":"draft","slug":"callouts","name":"Callouts","summary":"","project_types":null,
               "status":"draft","requested_status":null,"icon_url":null,"color":null,
               "downloads":0,"followers":0,"updated":"2026-11-01T00:00:00Z","game_versions":null}
            ]"#,
        ))
        .mount(&server)
        .await;
    Mock::given(path("/v2/user/u1/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"[
              {"id":"shared","slug":"bedbugs","title":"Bedbugs","description":"bugs","project_type":"mod",
               "status":"processing","requested_status":"unlisted","icon_url":null,"color":null,
               "downloads":23,"followers":1,"updated":"2026-10-18T15:07:18Z"},
              {"id":"mine","slug":"chatcore","title":"ChatCore","description":"","project_type":"mod",
               "status":"private","requested_status":null,"icon_url":null,"color":null,
               "downloads":0,"followers":0,"updated":"2026-11-01T00:00:00Z"}
            ]"#,
        ))
        .mount(&server)
        .await;

    let client = Client::with_base(server.uri(), ResponseCache::new(None));
    let projects = client
        .all_user_projects("u1", "mro_secret", None)
        .await
        .unwrap();
    let summary: Vec<_> = projects
        .iter()
        .map(|p| {
            (
                p.id.as_str(),
                p.project_type.as_str(),
                p.organization_name.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("shared", "mod", Some("nocturne")),
            ("draft", "project", Some("nocturne")),
            ("mine", "mod", None),
        ]
    );
    assert_eq!(projects[0].title, "Bedbugs");
}

#[tokio::test]
async fn project_page_author_from_organization_or_owner() {
    let server = MockServer::start().await;
    let project = |organization: serde_json::Value| {
        serde_json::json!({
            "id": "AANobbMI", "slug": "sodium", "title": "Sodium", "description": "Fast",
            "body": "# Sodium", "project_type": "mod", "icon_url": null, "color": null,
            "downloads": 1, "followers": 2, "published": "2021-01-03T00:00:00Z",
            "updated": "2026-11-08T00:00:00Z", "license": {"id": "MIT", "name": "MIT", "url": null},
            "source_url": null, "issues_url": null, "wiki_url": null, "discord_url": null,
            "gallery": [{"url": "https://cdn.modrinth.com/x.png", "featured": true, "title": "Shot"}],
            "team": "team1", "organization": organization
        })
    };
    Mock::given(method("GET"))
        .and(path("/v2/project/withorg"))
        .respond_with(ResponseTemplate::new(200).set_body_json(project("org1".into())))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v3/organization/org1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"name": "CaffeineMC"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/project/solo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(project(serde_json::Value::Null)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/project/solo/members"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"user": {"id": "a", "username": "helper", "avatar_url": null}, "is_owner": false},
            {"user": {"id": "b", "username": "jellysquid3", "avatar_url": null}, "is_owner": true}
        ])))
        .mount(&server)
        .await;

    let client = Client::with_base(server.uri(), ResponseCache::new(None));
    let page = client.project_page("withorg", None, None).await.unwrap();
    assert_eq!(page.author.as_deref(), Some("CaffeineMC"));
    assert_eq!(page.gallery[0].title.as_deref(), Some("Shot"));
    let page = client.project_page("solo", None, None).await.unwrap();
    assert_eq!(page.author.as_deref(), Some("jellysquid3"));
}
