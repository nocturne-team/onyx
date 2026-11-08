//! Lives in its own test binary because the download limits are process-wide.

use std::sync::Arc;
use std::time::{Duration, Instant};

use onyx_core::download::{self, Download, Progress};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn speed_limit_paces_downloads() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; 64 * 1024]))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let items = (0..4)
        .map(|i| Download {
            url: format!("{}/{i}", server.uri()),
            path: dir.path().join(format!("{i}.bin")),
            sha1: None,
            size: None,
        })
        .collect();

    // 256 KiB at 128 KiB/s: one second of burst, then about a second of waiting.
    download::set_limits(4, Some(128 * 1024));
    let start = Instant::now();
    download::download_all(
        &reqwest::Client::new(),
        items,
        &Arc::new(Progress::default()),
        download::concurrency(),
    )
    .await
    .unwrap();
    let took = start.elapsed();
    assert!(took >= Duration::from_millis(900), "took {took:?}");
    assert!(took < Duration::from_secs(5), "took {took:?}");
}
