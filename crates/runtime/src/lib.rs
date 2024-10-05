
#[allow(unused)]
#[macro_use]
extern crate tracing;

use tokio::signal::unix::{signal, SignalKind};
use tokio_util::sync::CancellationToken;

pub mod args;
pub mod utils;
pub mod log;
pub mod template;


type BoxedTask = Box<dyn FnOnce(CancellationToken) -> Box<dyn std::future::Future<Output = ()> + Send + 'static>>;

pub fn handler<Func, Fut>(f: Func) -> BoxedTask
where
    Func: FnOnce(CancellationToken) -> Fut + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static
{
    Box::new(move |c| Box::new(f(c)))
}

struct RunHandleInner {
    reload_channel: (flume::Sender<()>, flume::Receiver<()>),
    shutdown_channel: (flume::Sender<()>, flume::Receiver<()>),
}

#[derive(Clone)]
pub struct RunHandle(std::sync::Arc<RunHandleInner>);
impl RunHandle {
    pub fn new() -> Self {
        RunHandle(std::sync::Arc::new(RunHandleInner {
            reload_channel: flume::unbounded(),
            shutdown_channel: flume::unbounded(),
        }))
    }
    pub fn signal_reload(&self) {
        self.0.reload_channel.0.send(()).ok();
    }
    pub fn signal_shutdown(&self) {
        self.0.shutdown_channel.0.send(()).ok();
    }
}

fn log_task_exit(result: Option<Result<&'_ str, tokio::task::JoinError>>) {
    match result {
        Some(Ok(ident)) => info!("task {} exited", ident),
        Some(Err(e)) => warn!("task exited with failure: {}", e),
        None => warn!("remaining tasks list is empty?"),
    }
}

#[tracing::instrument(skip_all)]
pub async fn run(
    handle: RunHandle,
    tasks: Vec<(&'static str, BoxedTask)>,
    mut reload: Box<dyn FnMut()>,
) -> Result<(), std::io::Error> {
    let mut join_set = tokio::task::JoinSet::new();
    let mut remaining_tasks = tasks.len();
    let cancel = CancellationToken::new();

    for (ident, task) in tasks {
        let cancel_child = cancel.child_token();
        let future = task(cancel_child);
        let span = tracing::info_span!("task", name=ident).or_current();
        join_set.spawn(async move {
            log::instrument(span, Box::into_pin(future)).await;
            ident
        });
    }

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sighup = signal(SignalKind::hangup())?;

    let reload_rx = &handle.0.reload_channel.1;
    let shutdown_rx = &handle.0.shutdown_channel.1;

    enum Action {
        Exit(&'static str),
        Reload(&'static str),
        Continue,
    }

    loop {
        let action = tokio::select! {
            // Signal handlers
            _ = sighup.recv()  => Action::Reload("Received SIGHUP"),
            _ = sigint.recv()  => Action::Exit("Received SIGINT"),
            _ = sigterm.recv() => Action::Exit("Received SIGTERM"),

            // Requests through RunHandle
            _ = reload_rx.recv_async() => Action::Reload("Received reload request"),
            _ = shutdown_rx.recv_async() => Action::Exit("Received shutdown request"),

            // join_next is cancel-safe
            result = join_set.join_next() => {
                log_task_exit(result);
                remaining_tasks -= 1;
                Action::Exit("Task exited")
            },
        };

        match action {
            Action::Continue => (),
            Action::Reload(msg) => {
                info!("{msg}, reloading config");
                reload();
            },
            Action::Exit(msg) => {
                warn!("{msg}, starting shutdown");
                break;
            },
        }
    }

    log::instrument(tracing::info_span!("shut down").or_current(), async {
        info!("Starting to shut down");
        cancel.cancel();

        while remaining_tasks > 0 {
            let action = tokio::select! {
                _ = sigint.recv()  => Action::Exit("Received second SIGINT"),
                _ = sigterm.recv() => Action::Exit("Received second SIGTERM"),

                // join_next is cancel-safe
                result = join_set.join_next() => {
                    log_task_exit(result);
                    remaining_tasks -= 1;
                    Action::Continue
                },
            };
            match action {
                Action::Reload(_) => (),
                Action::Continue => (),
                Action::Exit(msg) => {
                    warn!("{msg}, exiting immediately");
                    break;
                },
            }
        }

        info!("Exiting");
    }).await;

    Ok(())
}

pub async fn cancellable<F, T>(cancel: &tokio_util::sync::CancellationToken, f: F) -> Option<T>
    where F: std::future::Future<Output = T>
{
    tokio::select! {
        v = f => Some(v),
        _ = cancel.cancelled() => None,
    }
}

