
#[allow(unused)]
#[macro_use]
extern crate tracing;

use axum::extract;
use std::sync::Arc;

pub mod layers;
pub mod server;


pub struct ServerState<T> {
    pub cookie_key: axum_extra::extract::cookie::Key,
    pub app: Arc<T>,
}
impl<T> extract::FromRef<ServerState<T>> for axum_extra::extract::cookie::Key {
    fn from_ref(state: &ServerState<T>) -> Self {
        state.cookie_key.clone()
    }
}
impl<T> Clone for ServerState<T> {
    fn clone(&self) -> Self {
        ServerState {
            cookie_key: self.cookie_key.clone(),
            app: Arc::clone(&self.app),
        }
    }
}
impl<T> std::ops::Deref for ServerState<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.app
    }
}
impl<T> ServerState<T> {
    pub fn new(key: axum_extra::extract::cookie::Key, app: Arc<T>) -> Self {
        Self {
            cookie_key: key,
            app,
        }
    }
}

// pub async fn start_webserver<T>(
//     bind: SocketAddr,
//     cancel: tokio_util::sync::CancellationToken,
//     app: Arc<T>,
// ) -> Result<(), anyhow::Error> {

//     let state = ServerState {
//         cookie_key: axum_extra::extract::cookie::Key::generate(),
//         app: app,
//     };

//     let app = Router::new()
//         .nest_service("/assets", layers::make_assets_router("assets".as_ref()))
//         .nest_service("/", main_api(state))
//         .layer(layers::cross_origin_layer())
//         .layer(tower_http::catch_panic::CatchPanicLayer::new())
//         .layer(layers::make_trace_layer())
//     ;

//     info!("web server listening on {}", bind);

//     run_server(cancel, bind, app).await?;

//     info!("webserver exiting");

//     Ok(())
// }

pub struct ExtractUserAgent(pub Option<String>);

#[axum::async_trait]
impl<S> axum::extract::FromRequestParts<S> for ExtractUserAgent where S: Send + Sync {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut axum::http::request::Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(user_agent) = parts.headers.get(axum::http::header::USER_AGENT) {
            let ua = String::from_utf8_lossy(user_agent.as_bytes()).into_owned();
            Ok(ExtractUserAgent(Some(ua)))
        } else {
            Ok(ExtractUserAgent(None))
        }
    }
}

