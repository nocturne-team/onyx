//! The `onyx-icon` URI scheme: `onyx-icon://localhost/<percent-encoded CDN URL>`
//! (`http://onyx-icon.localhost/...` on Windows) serves a Modrinth icon from
//! the disk cache, downloading it the first time.

use std::sync::Arc;

use onyx_core::icons::IconCache;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Runtime, UriSchemeContext, UriSchemeResponder};

pub fn protocol<R: Runtime>(
    cache: Arc<IconCache>,
) -> impl Fn(UriSchemeContext<'_, R>, Request<Vec<u8>>, UriSchemeResponder) + Send + Sync + 'static
{
    move |_ctx, request, responder| {
        let cache = cache.clone();
        let encoded = request.uri().path().trim_start_matches('/').to_owned();
        tauri::async_runtime::spawn(async move {
            let url = percent_encoding::percent_decode_str(&encoded).decode_utf8_lossy();
            let response = match cache.get(&url).await {
                Ok((bytes, mime)) => Response::builder()
                    .header(header::CONTENT_TYPE, mime)
                    .header(header::CACHE_CONTROL, "max-age=604800")
                    .body(bytes),
                Err(e) => {
                    tracing::debug!("icon {url}: {e}");
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Vec::new())
                }
            };
            responder.respond(response.expect("static headers are valid"));
        });
    }
}
