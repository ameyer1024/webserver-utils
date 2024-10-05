
use tracing_subscriber::filter::Targets;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::layer::{Layer, SubscriberExt};

use crate::utils::parse::{take_char, take_while};


#[inline]
pub async fn instrument<F, O>(span: tracing::Span, f: F) -> O where F: std::future::Future<Output = O> {
    use tracing::Instrument;
    f.instrument(span).await
}

#[macro_export]
macro_rules! instrument {
    ($name:expr; $future:expr) => {
        $crate::instrument!(@ [$name] [] ; $future)
    };
    ($name:expr, $($tt:tt)*) => {
        $crate::instrument!(@ [$name] [] $($tt)*)
    };
    (@ [$name:expr] [$($captured:tt)*]) => {
        compile_error!("missing semicolon, needs future to instrument")
    };
    (@ [$name:expr] [$($captured:tt)*] ; $($rest:tt)*) => {
        $crate::log::instrument(::tracing::info_span!($name, $($captured)*).or_current(), $($rest)*)
    };
    (@ [$name:expr] [$($captured:tt)*] $next:tt $($rest:tt)*) => {
        $crate::instrument!(@ [$name] [$($captured)* $next] $($rest)*)
    };
}


pub fn ansi_to_html(msg: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(msg.len());
    let msg = crate::template::sanitize_html_to_text(msg); // TODO: this should be done *after* processing
    let mut msg = &msg[..];
    let mut count = 0;
    while let Some(i) = msg.find("\x1b[") {
        out.push_str(&msg[..i]);

        let mut new_msg = &msg[i + 2 ..];
        let (params, interm, end);

        (params, new_msg) = take_while(new_msg, |c| matches!(c, '\x30'..='\x3F'));
        (interm, new_msg) = take_while(new_msg, |c| matches!(c, '\x20'..='\x2F'));
        (end, new_msg) = take_char(new_msg);

        if let Some(c @ '\x40'..='\x7E') = end {
            msg = new_msg;
            match (params, interm, c) {
                ("0", "", 'm') => {
                    for _ in 0..count { out.push_str("</span>"); }
                    count = 0;
                }
                _ => {
                    write!(out, "<span class='ansi-{}{}{}'>", params, interm, c).unwrap();
                    count += 1;
                }
            }
        } else {
            out.push_str(&msg[i .. i + 2]);
            msg = &msg[i + 2 ..]
        }
    }
    out.push_str(&msg);
    for _ in 0..count {
        out.push_str("</span>");
    }
    out
}

pub struct AnsiHtmlWriter {
    buffer: Vec<u8>,
    broadcast: tokio::sync::broadcast::Sender<std::sync::Arc<str>>,
}
impl AnsiHtmlWriter {
    pub fn from_channel(tx: tokio::sync::broadcast::Sender<std::sync::Arc<str>>) -> Self {
        Self {
            buffer: Vec::new(),
            broadcast: tx,
        }
    }
}
impl std::io::Write for AnsiHtmlWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(newline) = buf.iter().position(|c| *c == b'\n') {
            self.buffer.extend(&buf[..newline]);
            let msg = std::str::from_utf8(&self.buffer)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
            self.broadcast.send(ansi_to_html(msg).into()).ok();
            self.buffer.clear();
            Ok(newline + 1)
        } else {
            self.buffer.extend(buf);
            Ok(buf.len())
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub static LOG_LISTENER: std::sync::OnceLock<tokio::sync::broadcast::Sender<std::sync::Arc<str>>> = std::sync::OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub enum LoggerError {
    #[error("Error parsing RUST_LOG env var into targets specifier")]
    InvalidLogEnv(tracing_subscriber::filter::ParseError),
    #[error("Log listener was already set? (setup_logger called twice)")]
    AlreadySet,
    #[error("Setting tracing listener failed")]
    SetFailed(tracing::subscriber::SetGlobalDefaultError),
}

pub fn setup_logger(crate_name: &'static str) -> Result<tokio::sync::broadcast::Receiver<std::sync::Arc<str>>, LoggerError> {
    let env_targets = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| format!("{}=trace,runtime=debug,tower_http=debug,warn", crate_name));
    let env_filter = env_targets.parse::<Targets>().map_err(LoggerError::InvalidLogEnv)?;

    let (tx, rx) = tokio::sync::broadcast::channel(10);
    LOG_LISTENER.set(tx.clone()).map_err(|_| LoggerError::AlreadySet)?;

    let subscriber = Registry::default()
        // .with(tracing_subscriber::fmt::layer().with_filter(env_filter.clone()))
        .with(tracing_tree::HierarchicalLayer::new(2)
            .with_targets(true)
            .with_bracketed_fields(true)
            .with_filter(env_filter.clone())
        )

        // .with(tracing_subscriber::fmt::layer()
            // .with_ansi(false)
            // .fmt_fields(tracing_subscriber::fmt::format::PrettyFields::new().with_ansi(false))
            // .with_writer(move || AnsiHtmlWriter::from_channel(tx.clone()))
            // .with_filter(env_filter.clone()))
        ;

    tracing::subscriber::set_global_default(subscriber)
        .map_err(LoggerError::SetFailed)?;

    Ok(rx)
}
