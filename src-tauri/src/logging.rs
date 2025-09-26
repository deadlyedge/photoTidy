use tracing_subscriber::{fmt, EnvFilter};

pub fn init_logging() {
    if tracing::dispatcher::has_been_set() {
        return;
    }

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,phototidy=debug"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
