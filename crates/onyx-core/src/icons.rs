//! Disk cache for project icons, so lists of installed content don't fetch
//! every icon from the CDN each time they open.

use std::path::PathBuf;

use sha1::{Digest, Sha1};

/// Icons bigger than this aren't cached (or served).
const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum IconError {
    #[error("not a Modrinth CDN URL")]
    NotAllowed,
    #[error("could not download the icon: {0}")]
    Http(#[from] reqwest::Error),
    #[error("the icon request returned HTTP {0}")]
    Status(u16),
    #[error("the icon is too big or not an image")]
    Invalid,
}

pub struct IconCache {
    dir: Option<PathBuf>,
    http: reqwest::Client,
}

impl IconCache {
    /// `dir: None` still serves icons, it just doesn't keep them.
    pub fn new(dir: Option<PathBuf>, http: reqwest::Client) -> Self {
        Self { dir, http }
    }

    /// The icon at `url` and its MIME type, from disk when we have it.
    /// Only `https://cdn.modrinth.com/` URLs are fetched.
    pub async fn get(&self, url: &str) -> Result<(Vec<u8>, &'static str), IconError> {
        let parsed = reqwest::Url::parse(url).map_err(|_| IconError::NotAllowed)?;
        if parsed.scheme() != "https" || parsed.host_str() != Some("cdn.modrinth.com") {
            return Err(IconError::NotAllowed);
        }
        let file = self
            .dir
            .as_ref()
            .map(|dir| dir.join(crate::download::hex(&Sha1::digest(url.as_bytes()))));
        if let Some(file) = &file
            && let Ok(bytes) = tokio::fs::read(file).await
            && let Some(mime) = sniff(&bytes)
        {
            return Ok((bytes, mime));
        }

        let response = self.http.get(parsed).send().await?;
        if !response.status().is_success() {
            return Err(IconError::Status(response.status().as_u16()));
        }
        let bytes = response.bytes().await?.to_vec();
        let mime = sniff(&bytes)
            .filter(|_| bytes.len() <= MAX_BYTES)
            .ok_or(IconError::Invalid)?;
        if let Some(file) = file {
            // Losing the cache only costs a re-download.
            if let Some(dir) = file.parent() {
                let _ = tokio::fs::create_dir_all(dir).await;
            }
            let temp = file.with_extension("part");
            if tokio::fs::write(&temp, &bytes).await.is_ok() {
                let _ = tokio::fs::rename(&temp, &file).await;
            }
        }
        Ok((bytes, mime))
    }
}

/// Image type from the first bytes. Anything else isn't served.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    let start = |magic: &[u8]| bytes.starts_with(magic);
    if start(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if start(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if start(b"GIF87a") || start(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && start(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
        let head = head.trim_start_matches('\u{feff}').trim_start();
        (head.starts_with("<svg") || head.starts_with("<?xml")).then_some("image/svg+xml")
    }
}

#[cfg(test)]
mod tests {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    use super::*;

    #[tokio::test]
    async fn only_modrinth_cdn_urls_are_fetched() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(b"\x89PNG\r\n\x1a\nxx".to_vec()),
            )
            .expect(0)
            .mount(&server)
            .await;
        let cache = IconCache::new(None, reqwest::Client::new());
        for url in [
            server.uri() + "/icon.png",
            "http://cdn.modrinth.com/a.png".into(),
            "https://cdn.modrinth.com.evil.example/a.png".into(),
            "file:///etc/passwd".into(),
        ] {
            assert!(
                matches!(cache.get(&url).await, Err(IconError::NotAllowed)),
                "{url}"
            );
        }
    }

    #[test]
    fn sniffs_image_types() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Some("image/png"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff(b"  <svg xmlns=\"\"/>"), Some("image/svg+xml"));
        assert_eq!(sniff(b"<html>"), None);
    }
}
