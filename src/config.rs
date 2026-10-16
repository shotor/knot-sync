use std::path::PathBuf;
use std::time::Duration;

use anyhow::bail;

use crate::utils::env::{parse_or, required_value, value_or};

#[derive(Debug, Clone)]
pub struct Config {
  pub log_format: LogFormat,
  pub ldap_url: String,
  pub ldap_bind_dn: String,
  /// The bind password is read from this file at connect time, never from the environment.
  pub ldap_password_file: PathBuf,
  pub ldap_base_dn: String,
  /// The address attributes read, plain addresses one per value: virtual aliases and masked
  /// email, whichever mapper the extension uses, never their JSON attributes.
  pub ldap_attributes: Vec<String>,
  /// `host:port` of the Knot root, resolved at every reconcile.
  pub dns_server: String,
  /// The zone files from Git, `<zone>.zone` each; every zone with a file is this tool's.
  pub zone_dir: PathBuf,
  /// How often the zone files are checked for a change, which reconciles at once.
  pub zone_poll: Duration,
  pub tsig_key_name: String,
  /// The base64 TSIG secret is read from this file, never from the environment.
  pub tsig_key_file: PathBuf,
  pub mx_host: String,
  pub reconcile_interval: Duration,
  /// A pass that would delete more records than this, over every zone, is refused.
  pub max_deletes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
  Pretty,
  Json,
}

impl Config {
  pub fn from_env() -> anyhow::Result<Self> {
    let log_format = match value_or("KNOT_SYNC_LOG_FORMAT", "pretty")
      .to_ascii_lowercase()
      .as_str()
    {
      "pretty" => LogFormat::Pretty,
      "json" => LogFormat::Json,
      other => bail!("KNOT_SYNC_LOG_FORMAT must be pretty or json, got {other}"),
    };

    let ldap_url = required_value("KNOT_SYNC_LDAP_URL")?;
    let ldap_bind_dn = required_value("KNOT_SYNC_LDAP_BIND_DN")?;
    let ldap_password_file = PathBuf::from(required_value("KNOT_SYNC_LDAP_PASSWORD_FILE")?);
    let ldap_base_dn = required_value("KNOT_SYNC_LDAP_BASE_DN")?;
    let ldap_attributes = attribute_list(&value_or(
      "KNOT_SYNC_LDAP_ATTRIBUTES",
      "mailAliases,mailMaskedAddresses",
    ))?;
    let dns_server = required_value("KNOT_SYNC_DNS_SERVER")?;
    let zone_dir = PathBuf::from(required_value("KNOT_SYNC_ZONE_DIR")?);
    let zone_poll = Duration::from_secs(parse_or("KNOT_SYNC_ZONE_POLL_SECS", "30")?);
    let tsig_key_name = value_or("KNOT_SYNC_TSIG_KEY_NAME", "knot-sync");
    let tsig_key_file = PathBuf::from(required_value("KNOT_SYNC_TSIG_KEY_FILE")?);
    let mx_host = value_or("KNOT_SYNC_MX_HOST", "mail.shotor.com");
    let reconcile_interval = Duration::from_secs(parse_or("KNOT_SYNC_INTERVAL_SECS", "900")?);
    let max_deletes = parse_or("KNOT_SYNC_MAX_DELETES", "5")?;

    Ok(Self {
      log_format,
      ldap_url,
      ldap_bind_dn,
      ldap_password_file,
      ldap_base_dn,
      ldap_attributes,
      dns_server,
      zone_dir,
      zone_poll,
      tsig_key_name,
      tsig_key_file,
      mx_host,
      reconcile_interval,
      max_deletes,
    })
  }
}

/// Attribute names go into the search filter, so only plain LDAP names pass.
pub fn attribute_list(value: &str) -> anyhow::Result<Vec<String>> {
  let attributes: Vec<String> = value
    .split(',')
    .map(str::trim)
    .filter(|name| !name.is_empty())
    .map(str::to_owned)
    .collect();

  if attributes.is_empty() {
    bail!("KNOT_SYNC_LDAP_ATTRIBUTES names no attribute");
  }

  for name in &attributes {
    let plain = name.starts_with(|c: char| c.is_ascii_alphabetic())
      && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');

    if !plain {
      bail!("KNOT_SYNC_LDAP_ATTRIBUTES: not an attribute name: {name}");
    }
  }

  Ok(attributes)
}

#[cfg(test)]
#[path = "config__test.rs"]
mod config_test;
