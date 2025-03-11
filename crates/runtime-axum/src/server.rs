use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;
use tower_http::timeout::TimeoutLayer;

pub async fn run_server(
    cancel: tokio_util::sync::CancellationToken,
    bind: SocketAddr,
    app: Router,
) -> Result<(), std::io::Error> {
    let shutdown_timeout = std::time::Duration::from_secs(8);
    let shutdown_signal = async move {
        cancel.cancelled().await;
        info!(
            "Attempting graceful webserver shutdown with {}s timeout",
            shutdown_timeout.as_secs_f32()
        );
    };

    let app = app.layer(TimeoutLayer::new(shutdown_timeout));

    let listener = TcpListener::bind(bind).await?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal)
    .await?;

    Ok(())
}
