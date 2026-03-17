use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "dashboard/dist/"]
struct Assets;

pub fn static_handler() -> axum::routing::MethodRouter {
    get(|uri: Uri| async move {
        let path = uri.path().trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };

        match Assets::get(path) {
            Some(content) => {
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                let cache_control = if path == "index.html" {
                    "no-cache"
                } else {
                    "public, max-age=31536000, immutable"
                };
                Response::builder()
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .header(header::CACHE_CONTROL, cache_control)
                    .header("Content-Security-Policy",
                        "default-src 'self'; connect-src 'self' ws://localhost:* ws://127.0.0.1:*; \
                         style-src 'self' 'unsafe-inline'; font-src 'self'")
                    .header("X-Content-Type-Options", "nosniff")
                    .header("X-Frame-Options", "DENY")
                    .body(Body::from(content.data))
                    .expect("valid headers")
            }
            None => StatusCode::NOT_FOUND.into_response(),
        }
    })
}
