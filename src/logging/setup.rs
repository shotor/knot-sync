use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

use crate::config::LogFormat;

pub fn init_tracing(format: &LogFormat) -> anyhow::Result<()> {
  let filter =
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,knot_sync=info"));

  match format {
    LogFormat::Pretty => {
      tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().pretty().with_target(true))
        .init();
    }
    LogFormat::Json => {
      tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().json().with_current_span(true))
        .init();
    }
  }

  Ok(())
}
