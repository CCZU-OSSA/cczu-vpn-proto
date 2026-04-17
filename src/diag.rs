use std::sync::OnceLock;

use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;

static TRACING_INITIALIZED: OnceLock<()> = OnceLock::new();

pub fn init_tracing(level: &str) -> Result<()> {
    if TRACING_INITIALIZED.get().is_some() {
        return Ok(());
    }

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(level))
        .context("failed to build tracing filter")?;

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_names(true)
        .compact()
        .try_init()
        .map_err(|err| anyhow::anyhow!("failed to initialize tracing subscriber: {err}"))?;

    let _ = TRACING_INITIALIZED.set(());
    Ok(())
}

pub fn try_init_tracing(level: &str) {
    let _ = init_tracing(level);
}
