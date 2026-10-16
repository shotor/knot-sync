use std::time::Duration;

use anyhow::bail;
use tokio::time::{MissedTickBehavior, interval, sleep, timeout};
use tracing::{error, info, warn};

use crate::config::Config;
use crate::directory::{Directory, Watch};
use crate::dns::{Knot, fqdn};
use crate::records::{Changes, diff, wanted, wanted_labels};
use crate::zones::{fingerprint, read_zones};

const RETRY: Duration = Duration::from_secs(30);
/// Changes arrive in bursts (one per attribute value and replica); one reconcile covers them.
const SETTLE: Duration = Duration::from_secs(2);

/// Runs forever: a session per directory connection, a new one after any failure.
pub async fn run(config: Config) {
  loop {
    if let Err(error) = session(&config).await {
      warn!(error = format!("{error:#}"), retry_in = ?RETRY, "session ended");
    }

    sleep(RETRY).await;
  }
}

/// One directory connection: a full reconcile first, then one on every change in the directory,
/// on every change of the zone files and every interval. The persistent search cannot resume, so
/// a lost connection ends the session and the next one starts with a full reconcile again. Without
/// the directory nothing is reconciled, since the contact records would look unwanted.
async fn session(config: &Config) -> anyhow::Result<()> {
  let mut directory = Directory::connect(config).await?;
  let mut watch = directory.watch().await?;
  let mut ticks = interval(config.reconcile_interval);
  ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
  let mut polls = interval(config.zone_poll);
  polls.set_missed_tick_behavior(MissedTickBehavior::Delay);
  let mut seen = None;

  info!(base = %config.ldap_base_dn, "watching the directory");

  loop {
    tokio::select! {
      change = watch.next() => {
        if change?.is_none() {
          bail!("the persistent search ended");
        }

        settle(&mut watch).await?;
        reconcile_logged(config, &mut directory, &mut seen, "change").await;
      }
      _ = ticks.tick() => {
        reconcile_logged(config, &mut directory, &mut seen, "interval").await;
      }
      _ = polls.tick() => {
        match fingerprint(&config.zone_dir) {
          Ok(current) if seen.is_some_and(|seen| seen != current) => {
            reconcile_logged(config, &mut directory, &mut seen, "zone files").await;
          }
          Ok(_) => {}
          Err(error) => warn!(error = format!("{error:#}"), "zone files unreadable"),
        }
      }
    }
  }
}

async fn settle(watch: &mut Watch) -> anyhow::Result<()> {
  while let Ok(change) = timeout(SETTLE, watch.next()).await {
    if change?.is_none() {
      bail!("the persistent search ended");
    }
  }

  Ok(())
}

async fn reconcile_logged(
  config: &Config,
  directory: &mut Directory,
  seen: &mut Option<u64>,
  cause: &str,
) {
  match reconcile(config, directory).await {
    Ok(current) => *seen = Some(current),
    Err(error) => error!(cause, error = format!("{error:#}"), "reconcile failed"),
  }
}

/// Every zone file against its zone as served, with the contact records the directory's
/// addresses ask for. Nothing is sent before every zone is planned, so a refused pass changes no
/// zone. Returns the zone files' fingerprint the pass worked from.
pub async fn reconcile(config: &Config, directory: &mut Directory) -> anyhow::Result<u64> {
  let current = fingerprint(&config.zone_dir)?;
  let zones = read_zones(&config.zone_dir)?;
  let addresses = directory.addresses().await?;
  let mx_host = fqdn(&config.mx_host)?;

  let mut knot = Knot::connect(config).await?;
  let mut plans: Vec<(String, Changes)> = Vec::new();
  let mut deletes = 0;

  for zone in &zones {
    let origin = zone.origin.to_string();
    let labels = wanted_labels(addresses.iter().map(String::as_str), &origin);
    let wanted = wanted(&zone.origin, &zone.rrsets, &labels, &mx_host)?;
    let present = knot.present(&zone.origin).await?;
    let changes = diff(&wanted, &present);

    deletes += changes.deleted_records(&present);
    plans.push((origin, changes));
  }

  if deletes > config.max_deletes {
    bail!(
      "refusing to delete {deletes} records, more than {}",
      config.max_deletes
    );
  }

  for (zone, (origin, changes)) in zones.iter().zip(&plans) {
    if changes.is_empty() {
      continue;
    }

    knot.apply(&zone.origin, changes).await?;

    info!(
      zone = %origin,
      added = ?changes.add.iter().map(|(name, kind, _, _)| format!("{name} {kind}")).collect::<Vec<_>>(),
      removed = ?changes.remove.iter().map(|(name, kind, _)| format!("{name} {kind}")).collect::<Vec<_>>(),
      deleted = ?changes.delete.iter().map(|(name, kind)| format!("{name} {kind}")).collect::<Vec<_>>(),
      "zone updated"
    );
  }

  info!(
    zones = zones.len(),
    addresses = addresses.len(),
    "reconciled"
  );

  Ok(current)
}
