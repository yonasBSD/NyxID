use axum::{
    body::Body,
    http::{Request, header},
    middleware::Next,
    response::Response,
};

/// Middleware that adds security-related HTTP headers to every response.
///
/// Headers added:
/// - Strict-Transport-Security (HSTS)
/// - X-Content-Type-Options
/// - X-Frame-Options
/// - Content-Security-Policy
/// - Referrer-Policy
/// - Permissions-Policy
/// - X-XSS-Protection
pub async fn security_headers_middleware(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // HSTS: enforce HTTPS for 1 year, including subdomains
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains; preload"
            .parse()
            .unwrap(),
    );

    // Prevent MIME-type sniffing
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());

    // Prevent framing (clickjacking protection)
    headers.insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());

    // Content Security Policy — only set if the handler hasn't already provided one
    // (e.g. oauth_success_page sets a custom CSP allowing inline style/script).
    if !headers.contains_key(header::CONTENT_SECURITY_POLICY) {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; frame-ancestors 'none'"
                .parse()
                .unwrap(),
        );
    }

    // Control referrer information
    headers.insert(
        header::REFERRER_POLICY,
        "strict-origin-when-cross-origin".parse().unwrap(),
    );

    // Restrict browser features
    headers.insert(
        "permissions-policy".parse::<header::HeaderName>().unwrap(),
        "camera=(), microphone=(), geolocation=(), interest-cohort=()"
            .parse()
            .unwrap(),
    );

    // Legacy XSS protection (for older browsers)
    headers.insert(
        "x-xss-protection".parse::<header::HeaderName>().unwrap(),
        "1; mode=block".parse().unwrap(),
    );

    // Prevent caching of API responses (SEC-6: protects credential endpoints)
    headers.insert(
        header::CACHE_CONTROL,
        "no-store, no-cache, must-revalidate".parse().unwrap(),
    );
    headers.insert(header::PRAGMA, "no-cache".parse().unwrap());

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::{Router, routing::get};
    use tower::ServiceExt;

    async fn ok_handler() -> StatusCode {
        StatusCode::OK
    }

    async fn custom_csp_handler() -> Response {
        let mut resp = Response::new(Body::empty());
        resp.headers_mut().insert(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'self'".parse().unwrap(),
        );
        resp
    }

    #[tokio::test]
    async fn injects_all_security_headers() {
        let app = Router::new()
            .route("/test", get(ok_handler))
            .layer(axum::middleware::from_fn(security_headers_middleware));
        let resp = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(
            resp.headers()
                .contains_key(header::STRICT_TRANSPORT_SECURITY)
        );
        assert!(resp.headers().contains_key(header::X_CONTENT_TYPE_OPTIONS));
        assert!(resp.headers().contains_key(header::X_FRAME_OPTIONS));
        assert!(resp.headers().contains_key(header::CONTENT_SECURITY_POLICY));
        assert!(resp.headers().contains_key(header::REFERRER_POLICY));
        assert!(resp.headers().contains_key(header::CACHE_CONTROL));
        assert!(resp.headers().contains_key(header::PRAGMA));
        assert_eq!(
            resp.headers().get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );
        assert_eq!(resp.headers().get(header::X_FRAME_OPTIONS).unwrap(), "DENY");
    }

    #[tokio::test]
    async fn preserves_handler_csp() {
        let app = Router::new()
            .route("/csp", get(custom_csp_handler))
            .layer(axum::middleware::from_fn(security_headers_middleware));
        let resp = app
            .oneshot(Request::builder().uri("/csp").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            resp.headers().get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "default-src 'self'"
        );
    }
}
