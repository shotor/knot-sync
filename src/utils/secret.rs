use std::path::Path;

use anyhow::Context;

/// Reads a secret from its file at the moment it is needed, so a rotated Secret is picked up at
/// the next connect without a restart.
pub async fn read_secret(path: &Path) -> anyhow::Result<String> {
  let value = tokio::fs::read_to_string(path)
    .await
    .with_context(|| format!("read {}", path.display()))?;

  Ok(value.trim().to_owned())
}
