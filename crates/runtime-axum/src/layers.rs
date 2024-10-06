use axum::{routing, extract, middleware};
use axum::http::{HeaderName, HeaderValue};
use tower_http::trace as tower_trace;
use std::net::SocketAddr;

// TODO: better error messages
// https://github.com/tokio-rs/axum/issues/1116


pub fn cross_origin_layer()
-> impl tower::Layer<
        routing::Route,
        Service = impl tower::Service<
            axum::http::Request<axum::body::Body>,
            Response = impl axum::response::IntoResponse,
            Error = impl Into<std::convert::Infallible>,
            Future = impl Send,
        > + Clone
    > + Clone
{
    middleware::from_fn(|req: extract::Request, next: middleware::Next| async move {
        ([
            (HeaderName::from_static("cross-origin-opener-policy"), HeaderValue::from_static("same-origin")),
            (HeaderName::from_static("cross-origin-embedder-policy"), HeaderValue::from_static("require-corp")),
        ], next.run(req).await)
    })
}


const DISABLE_CACHE_CSS: bool = false;

pub fn make_assets_router<F>(
    directory: &std::path::Path,
    fallback: F,
) -> impl axum::handler::Handler<(), ()>
    where
        F: tower::Service<
            axum::http::Request<axum::body::Body>,
            Response = axum::http::Response<axum::body::Body>,
            Error = core::convert::Infallible
        > + Clone + Send + Sync + 'static,
        <F as tower::Service<axum::http::Request<axum::body::Body>>>::Future: Send + 'static
{
    routing::get_service(
        tower_http::services::ServeDir::new(directory)
            .fallback(fallback)
    )
    // .layer(axum::error_handling::HandleErrorLayer::new(|error: _| async move {
    //     (StatusCode::INTERNAL_SERVER_ERROR, format!("Unhandled internal error: {}", error))
    // }))
    .layer(middleware::from_fn(|path: axum::http::uri::Uri, req: extract::Request, next: middleware::Next| async move {
        let response = next.run(req).await;
        if path.path().starts_with("/css") && DISABLE_CACHE_CSS {
            Ok(([(axum::http::header::CACHE_CONTROL, "no-cache")], response))
        } else if path.path().starts_with("/cdn") {
            Ok(([(axum::http::header::CACHE_CONTROL, "max-age=31536000, immutable")], response))
        } else {
            Err(response)
        }
    }))
}

// TODO: rebuild RAM assets server at some point
// (WIP at git commit 5856f44bf7e9476720dc1a96ed2b9a33b10750a4 in src/web.rs)


// Well, I guess it is supposed to be a tower...
pub fn make_trace_layer()
 -> impl tower::Layer<
        routing::Route,
        Service = impl tower::Service<
            axum::http::Request<axum::body::Body>,
            Response = impl axum::response::IntoResponse,
            Error = impl Into<std::convert::Infallible>,
            Future = impl Send,
        > + Clone
    > + Clone
{
    let custom_trace_layer = tower_trace::TraceLayer::new(
        tower_http::classify::SharedClassifier::new(tower_http::classify::ServerErrorsAsFailures::default())
    )
        .make_span_with(|request: &axum::http::Request<axum::body::Body>| {
            // Can't use extractors since this isn't async
            let connect_info = request.extensions().get::<extract::ConnectInfo<SocketAddr>>()
                .map(|c| c.0);
            let headers = request.headers();
            let agent = headers.get(axum::http::header::USER_AGENT);

            // Because almost nothing in the tracing ecosystem supports optional / late-initialized fields
            let span = match (connect_info, agent) {
                (Some(ip), Some(agent)) => tracing::debug_span!(
                    "request", method = %request.method(), uri = %request.uri(), version = ?request.version(),
                    ip = %ip, useragent = ?agent,
                ),
                (Some(ip), None) => tracing::debug_span!(
                    "request", method = %request.method(), uri = %request.uri(), version = ?request.version(),
                    ip = %ip, useragent = tracing::field::Empty,
                ),
                (None, Some(agent)) => tracing::debug_span!(
                    "request", method = %request.method(), uri = %request.uri(), version = ?request.version(),
                    ip = tracing::field::Empty, useragent = ?agent,
                ),
                (None, None) => tracing::debug_span!(
                    "request", method = %request.method(), uri = %request.uri(), version = ?request.version(),
                    ip = tracing::field::Empty, useragent = tracing::field::Empty,
                ),
            };
            span
        })
        .on_request(
            tower_trace::DefaultOnRequest::new()
        )
        .on_response(
            tower_trace::DefaultOnResponse::new()
                .latency_unit(tower_http::LatencyUnit::Micros)
        );
    custom_trace_layer
}

