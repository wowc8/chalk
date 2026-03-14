use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Returns the OS-specific log directory for LPA.
/// - macOS: ~/Library/Application Support/com.madison.lpa/logs
/// - Windows: %APPDATA%/com.madison.lpa/logs
/// - Linux: ~/.local/share/com.madison.lpa/logs
fn log_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("com.madison.lpa").join("logs")
}

/// Initialize the structured logging system.
/// Returns a guard that must be held for the lifetime of the application
/// to ensure all logs are flushed.
pub fn init() -> WorkerGuard {
    let log_dir = log_dir();
    std::fs::create_dir_all(&log_dir).expect("failed to create log directory");

    let file_appender = tracing_appender::rolling::daily(&log_dir, "lpa.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with(
            fmt::layer()
                .json()
                .with_target(true)
                .with_thread_ids(true)
                .with_writer(non_blocking),
        )
        .init();

    tracing::info!(log_dir = %log_dir.display(), "LPA logging initialized");

    guard
}
