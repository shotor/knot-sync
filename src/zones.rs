use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use hickory_client::proto::rr::{Name, RecordType};
use hickory_client::proto::serialize::txt::Parser;

use crate::records::{Rrset, Rrsets};

const SUFFIX: &str = ".zone";

/// One zone file from Git: `<zone>.zone`, its records as they should be served.
#[derive(Debug, Clone)]
pub struct ZoneFile {
  pub origin: Name,
  pub rrsets: Rrsets,
}

/// The zone files in the directory, one zone each, in name order. A ConfigMap mount keeps its
/// own entries under `..`-prefixed names, which are skipped.
pub fn zone_paths(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
  let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
    .with_context(|| format!("read {}", dir.display()))?
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|path| {
      path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| !name.starts_with('.') && name.ends_with(SUFFIX))
    })
    .collect();

  paths.sort();

  if paths.is_empty() {
    bail!("no {SUFFIX} files in {}", dir.display());
  }

  Ok(paths)
}

pub fn read_zones(dir: &Path) -> anyhow::Result<Vec<ZoneFile>> {
  zone_paths(dir)?
    .into_iter()
    .map(|path| {
      let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
      let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
      let zone = file_name.strip_suffix(SUFFIX).unwrap_or(file_name);

      parse_zone(&text, zone).with_context(|| format!("parse {}", path.display()))
    })
    .collect()
}

/// A hash of every zone file's name and contents, so a changed mount is noticed by polling.
pub fn fingerprint(dir: &Path) -> anyhow::Result<u64> {
  let mut hasher = DefaultHasher::new();

  for path in zone_paths(dir)? {
    path.hash(&mut hasher);
    std::fs::read(&path)
      .with_context(|| format!("read {}", path.display()))?
      .hash(&mut hasher);
  }

  Ok(hasher.finish())
}

pub fn parse_zone(text: &str, zone: &str) -> anyhow::Result<ZoneFile> {
  let origin = Name::from_ascii(format!("{}.", zone.trim_end_matches('.')))?.to_lowercase();
  let (_, records) = Parser::new(text, None, Some(origin.clone())).parse()?;
  let mut rrsets = Rrsets::new();

  for record_set in records.values() {
    let name = record_set.name().to_lowercase();

    if !origin.zone_of(&name) {
      bail!("{name} is outside {origin}");
    }

    let rrset = rrsets
      .entry((name, record_set.record_type()))
      .or_insert_with(|| Rrset {
        ttl: record_set.ttl(),
        data: Vec::new(),
      });

    for record in record_set.records_without_rrsigs() {
      rrset.push(record.data().clone());
    }
  }

  rrsets.retain(|_, rrset| !rrset.data.is_empty());

  if !rrsets.contains_key(&(origin.clone(), RecordType::NS)) {
    bail!("{origin} has no NS records at its apex");
  }

  Ok(ZoneFile { origin, rrsets })
}

#[cfg(test)]
#[path = "zones__test.rs"]
mod zones_test;
