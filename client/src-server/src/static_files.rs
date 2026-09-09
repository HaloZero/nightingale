use axum::{
    body::Body,
    http::{header, HeaderValue, Method, Request, Response, StatusCode, Uri},
};
use rust_embed::RustEmbed;

/// Frontend build output embedded into the server binary. The CI pipeline
/// (and `scripts/install.sh --from-source`) runs `pnpm build` ahead of
/// `cargo build -p server`, so the `dist/` directory is always present at
/// compile time.
#[derive(RustEmbed)]
#[folder = "../dist/"]
struct StaticAssets;

/// SPA-aware static handler: tries the requested path first, then falls back
/// to `index.html` so client-side routing keeps working under arbitrary URLs.
pub(crate) async fn handle(request: Request<Body>) -> Response<Body> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return method_not_allowed();
    }
    let uri = request.uri().clone();
    serve_path(&uri).unwrap_or_else(serve_index)
}

fn serve_path(uri: &Uri) -> Option<Response<Body>> {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return None;
    }
    let asset = StaticAssets::get(path)?;
    Some(asset_to_response(path, asset))
}

fn serve_index() -> Response<Body> {
    match StaticAssets::get("index.html") {
        Some(asset) => asset_to_response("index.html", asset),
        None => response(
            StatusCode::NOT_FOUND,
            Body::from("Frontend bundle missing. Did you run `pnpm build`?"),
        ),
    }
}

fn asset_to_response(path: &str, asset: rust_embed::EmbeddedFile) -> Response<Body> {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut response = response(StatusCode::OK, Body::from(asset.data.to_vec()));
    if let Ok(content_type) = HeaderValue::from_bytes(mime.as_ref().as_bytes()) {
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, content_type);
    }
    if path.starts_with("assets/") {
        // Vite emits content-hashed filenames under /assets, so they are safe
        // to cache aggressively. Everything else stays short-lived.
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
    response
}

fn response(status: StatusCode, body: Body) -> Response<Body> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response
}

fn method_not_allowed() -> Response<Body> {
    response(StatusCode::METHOD_NOT_ALLOWED, Body::empty())
}
