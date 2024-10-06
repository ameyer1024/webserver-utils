use axum::Router;

use runtime::instrument;
use runtime::utils::enclose;

use std::net::SocketAddr;

pub async fn run_server(
    cancel: tokio_util::sync::CancellationToken,
    bind: SocketAddr,
    app: Router,
) -> Result<(), std::io::Error> {
    let handle = axum_server::Handle::new();

    tokio::task::spawn(instrument!("shutdown task"; enclose!([clone handle] async move {
        cancel.cancelled().await;

        let timeout = std::time::Duration::from_secs(8);
        info!("Attempting graceful webserver shutdown with {}s timeout", timeout.as_secs_f32());
        handle.graceful_shutdown(Some(timeout));
    })));

    axum_server::bind(bind)
        .handle(handle)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;
    Ok(())
}
