use knot_sync::config::Config;
use knot_sync::logging::setup::init_tracing;
use knot_sync::sync::run;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  dotenvy::dotenv().ok();

  let config = Config::from_env()?;

  init_tracing(&config.log_format)?;

  info!(
    ldap_url = %config.ldap_url,
    dns_server = %config.dns_server,
    zone_dir = %config.zone_dir.display(),
    "starting"
  );

  run(config).await;

  Ok(())
}
