use std::time::Duration;

use anyhow::Context;
use ldap3::controls::RawControl;
use ldap3::{Ldap, LdapConnAsync, LdapConnSettings, Scope, SearchEntry, SearchStream};

use crate::config::Config;
use crate::utils::secret::read_secret;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// The Netscape persistent search control: every change type (add, delete, modify, modDN),
/// changes only, no entry change notification controls.
const PERSISTENT_SEARCH_OID: &str = "2.16.840.1.113730.3.4.3";
const PERSISTENT_SEARCH_VALUE: [u8; 11] = [
  0x30, 0x09, 0x02, 0x01, 0x0f, 0x01, 0x01, 0xff, 0x01, 0x01, 0x00,
];

pub type Watch = SearchStream<'static, String, Vec<String>>;

pub struct Directory {
  ldap: Ldap,
  base_dn: String,
  attributes: Vec<String>,
  filter: String,
}

impl Directory {
  pub async fn connect(config: &Config) -> anyhow::Result<Self> {
    let settings = LdapConnSettings::new().set_conn_timeout(CONNECT_TIMEOUT);

    let (conn, mut ldap) = LdapConnAsync::with_settings(settings, &config.ldap_url)
      .await
      .with_context(|| format!("connect {}", config.ldap_url))?;

    ldap3::drive!(conn);

    let password = read_secret(&config.ldap_password_file).await?;

    ldap
      .simple_bind(&config.ldap_bind_dn, &password)
      .await?
      .success()
      .with_context(|| format!("bind as {}", config.ldap_bind_dn))?;

    Ok(Self {
      ldap,
      base_dn: config.ldap_base_dn.clone(),
      attributes: config.ldap_attributes.clone(),
      filter: presence_filter(&config.ldap_attributes),
    })
  }

  /// Every value of the address attributes under the base, from every entry that has one.
  pub async fn addresses(&mut self) -> anyhow::Result<Vec<String>> {
    let (entries, _) = self
      .ldap
      .search(
        &self.base_dn,
        Scope::Subtree,
        &self.filter,
        self.attributes.clone(),
      )
      .await?
      .success()
      .with_context(|| format!("search {}", self.filter))?;

    let attributes = &self.attributes;

    let addresses = entries
      .into_iter()
      .map(SearchEntry::construct)
      .flat_map(|entry| {
        entry
          .attrs
          .into_iter()
          .filter(|(name, _)| {
            attributes
              .iter()
              .any(|attribute| name.eq_ignore_ascii_case(attribute))
          })
          .flat_map(|(_, values)| values)
      })
      .collect();

    Ok(addresses)
  }

  /// A persistent search on the same filter; each entry it yields is a change, used as a nudge.
  /// It asks for no attributes, since the reconcile reads the whole state itself.
  pub async fn watch(&mut self) -> anyhow::Result<Watch> {
    let control = RawControl {
      ctype: PERSISTENT_SEARCH_OID.into(),
      crit: true,
      val: Some(PERSISTENT_SEARCH_VALUE.to_vec()),
    };

    let stream = self
      .ldap
      .with_controls(control)
      .streaming_search(
        &self.base_dn,
        Scope::Subtree,
        &self.filter,
        vec!["1.1".to_owned()],
      )
      .await
      .context("start the persistent search")?;

    Ok(stream)
  }
}

/// Matches every entry that has any of the attributes.
pub fn presence_filter(attributes: &[String]) -> String {
  let terms: String = attributes
    .iter()
    .map(|attribute| format!("({attribute}=*)"))
    .collect();

  if attributes.len() == 1 {
    terms
  } else {
    format!("(|{terms})")
  }
}

#[cfg(test)]
#[path = "directory__test.rs"]
mod directory_test;
