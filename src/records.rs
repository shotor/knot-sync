use std::collections::{BTreeMap, BTreeSet};

use hickory_client::proto::rr::rdata::{MX, TXT};
use hickory_client::proto::rr::{Name, RData, RecordType};

pub const SPF: &str = "v=spf1 mx -all";
pub const MX_PREFERENCE: u16 = 10;
pub const CONTACT_TTL: u32 = 300;
const ACME_CHALLENGE: &[u8] = b"_acme-challenge";
/// ZONEMD (RFC 8976), which this hickory release does not name.
const ZONEMD: u16 = 63;

/// One RRset: its TTL and its records' data, each once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rrset {
  pub ttl: u32,
  pub data: Vec<RData>,
}

impl Rrset {
  pub fn push(&mut self, data: RData) {
    if !self.data.contains(&data) {
      self.data.push(data);
    }
  }

  fn same_data(&self, other: &Rrset) -> bool {
    self.data.len() == other.data.len() && self.data.iter().all(|data| other.data.contains(data))
  }
}

/// A zone's RRsets by owner and type, the owner lowercased.
pub type Rrsets = BTreeMap<(Name, RecordType), Rrset>;

/// What a reconcile changes in one zone: records added to an RRset, records taken out of one, and
/// RRsets removed whole.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
  pub add: Vec<(Name, RecordType, u32, Vec<RData>)>,
  pub remove: Vec<(Name, RecordType, Vec<RData>)>,
  pub delete: Vec<(Name, RecordType)>,
}

impl Changes {
  pub fn is_empty(&self) -> bool {
    self.add.is_empty() && self.remove.is_empty() && self.delete.is_empty()
  }

  /// The records this pass takes away, counted one by one: a whole RRset counts its records.
  pub fn deleted_records(&self, present: &Rrsets) -> usize {
    let removed: usize = self.remove.iter().map(|(_, _, data)| data.len()).sum();
    let deleted: usize = self
      .delete
      .iter()
      .map(|key| present.get(key).map_or(0, |rrset| rrset.data.len()))
      .sum();

    removed + deleted
  }
}

/// Knot's own records (the SOA with its serial, DNSSEC) and cert-manager's challenges are never
/// this tool's; everything else in a zone is.
pub fn is_managed(name: &Name, record_type: RecordType) -> bool {
  let unmanaged = matches!(
    record_type,
    RecordType::SOA
      | RecordType::DNSKEY
      | RecordType::RRSIG
      | RecordType::NSEC
      | RecordType::NSEC3
      | RecordType::NSEC3PARAM
      | RecordType::CDS
      | RecordType::CDNSKEY
      | RecordType::Unknown(ZONEMD)
  );

  let challenge = record_type == RecordType::TXT
    && name
      .iter()
      .next()
      .is_some_and(|label| label.eq_ignore_ascii_case(ACME_CHALLENGE));

  !unmanaged && !challenge
}

/// The labels whose `<label>.<zone>` the addresses ask for: one label below the zone, letters,
/// digits and hyphens only, so `_dmarc` or a deeper name can never be wanted.
pub fn wanted_labels<'a>(
  addresses: impl IntoIterator<Item = &'a str>,
  zone: &str,
) -> BTreeSet<String> {
  let suffix = format!(".{}", zone.trim_end_matches('.').to_ascii_lowercase());

  addresses
    .into_iter()
    .filter_map(|address| address.trim().rsplit_once('@'))
    .filter_map(|(_, domain)| {
      domain
        .trim_end_matches('.')
        .to_ascii_lowercase()
        .strip_suffix(&suffix)
        .map(str::to_owned)
    })
    .filter(|label| is_label(label))
    .collect()
}

pub fn is_label(label: &str) -> bool {
  !label.is_empty()
    && label.len() <= 63
    && !label.starts_with('-')
    && !label.ends_with('-')
    && label
      .bytes()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// The zone file's RRsets plus an MX and an SPF record for every contact label, except at a name
/// the zone file gives records of its own: there the file decides.
pub fn wanted(
  origin: &Name,
  file: &Rrsets,
  labels: &BTreeSet<String>,
  mx_host: &Name,
) -> anyhow::Result<Rrsets> {
  let mut wanted: Rrsets = file
    .iter()
    .filter(|((name, record_type), _)| is_managed(name, *record_type))
    .map(|(key, rrset)| (key.clone(), rrset.clone()))
    .collect();

  let named: BTreeSet<&Name> = file.keys().map(|(name, _)| name).collect();

  for label in labels {
    let name = Name::from_ascii(label)?
      .append_domain(origin)?
      .to_lowercase();

    if named.contains(&name) {
      continue;
    }

    let mx = RData::MX(MX::new(MX_PREFERENCE, mx_host.clone()));
    let txt = RData::TXT(TXT::new(vec![SPF.to_owned()]));

    wanted.insert(
      (name.clone(), RecordType::MX),
      Rrset {
        ttl: CONTACT_TTL,
        data: vec![mx],
      },
    );
    wanted.insert(
      (name, RecordType::TXT),
      Rrset {
        ttl: CONTACT_TTL,
        data: vec![txt],
      },
    );
  }

  Ok(wanted)
}

/// The difference between the wanted and the present RRsets, over the managed ones alone. A
/// changed RRset changes record by record, since the server ignores a delete of the apex NS RRset
/// (RFC 2136 3.4.2.4); a changed TTL adds every record again, which sets it.
pub fn diff(wanted: &Rrsets, present: &Rrsets) -> Changes {
  let mut changes = Changes::default();

  for ((name, record_type), rrset) in wanted {
    if !is_managed(name, *record_type) {
      continue;
    }

    match present.get(&(name.clone(), *record_type)) {
      None => changes
        .add
        .push((name.clone(), *record_type, rrset.ttl, rrset.data.clone())),
      Some(current) if current.ttl == rrset.ttl && current.same_data(rrset) => {}
      Some(current) => {
        let add: Vec<RData> = if current.ttl == rrset.ttl {
          rrset
            .data
            .iter()
            .filter(|data| !current.data.contains(data))
            .cloned()
            .collect()
        } else {
          rrset.data.clone()
        };

        let remove: Vec<RData> = current
          .data
          .iter()
          .filter(|data| !rrset.data.contains(data))
          .cloned()
          .collect();

        if !add.is_empty() {
          changes
            .add
            .push((name.clone(), *record_type, rrset.ttl, add));
        }

        if !remove.is_empty() {
          changes.remove.push((name.clone(), *record_type, remove));
        }
      }
    }
  }

  for (name, record_type) in present.keys() {
    if is_managed(name, *record_type) && !wanted.contains_key(&(name.clone(), *record_type)) {
      changes.delete.push((name.clone(), *record_type));
    }
  }

  changes
}

#[cfg(test)]
#[path = "records__test.rs"]
mod records_test;
