use hickory_client::proto::rr::RData;

use super::*;

const ZONE: &str = r#"$TTL 3600
@        SOA  defsn0.shtr.nl. hostmaster.shotor.com. 1 3600 900 2419200 300
@        NS   defsn0.shtr.nl.
@        NS   fihel0.shtr.nl.
@        MX   10 mail.shotor.com.
WWW      A    192.0.2.1
www      A    192.0.2.2
_dmarc   300 TXT "v=DMARC1; p=reject"
"#;

fn name(value: &str) -> Name {
  Name::from_ascii(value).unwrap()
}

#[test]
fn reads_rrsets_by_lowercased_owner() {
  let zone = parse_zone(ZONE, "shtr.eu").unwrap();

  assert_eq!(zone.origin, name("shtr.eu."));
  assert_eq!(
    zone.rrsets[&(name("shtr.eu."), RecordType::NS)].data.len(),
    2
  );

  let www = &zone.rrsets[&(name("www.shtr.eu."), RecordType::A)];
  assert_eq!(www.ttl, 3600);
  assert_eq!(www.data.len(), 2);

  let dmarc = &zone.rrsets[&(name("_dmarc.shtr.eu."), RecordType::TXT)];
  assert_eq!(dmarc.ttl, 300);
  assert!(matches!(dmarc.data[0], RData::TXT(_)));
}

#[test]
fn takes_the_origin_from_the_file_name() {
  let zone = parse_zone(ZONE, "shotor.com.").unwrap();

  assert!(
    zone
      .rrsets
      .contains_key(&(name("www.shotor.com."), RecordType::A))
  );
}

#[test]
fn refuses_a_zone_without_apex_ns() {
  let text = "$TTL 3600\nwww A 192.0.2.1\n";

  assert!(parse_zone(text, "shtr.eu").is_err());
}

#[test]
fn refuses_a_broken_file() {
  assert!(parse_zone("$TTL 3600\n@ NS\n", "shtr.eu").is_err());
}

#[test]
fn lists_zone_files_only() {
  let dir = std::env::temp_dir().join(format!("knot-sync-zones-{}", std::process::id()));
  std::fs::create_dir_all(dir.join("..data")).unwrap();
  std::fs::write(dir.join("shtr.eu.zone"), ZONE).unwrap();
  std::fs::write(dir.join("notes.txt"), "").unwrap();
  std::fs::write(dir.join(".hidden.zone"), "").unwrap();

  let paths = zone_paths(&dir).unwrap();
  let zones = read_zones(&dir).unwrap();
  let before = fingerprint(&dir).unwrap();
  std::fs::write(
    dir.join("shtr.eu.zone"),
    ZONE.replace("192.0.2.2", "192.0.2.3"),
  )
  .unwrap();
  let after = fingerprint(&dir).unwrap();
  std::fs::remove_dir_all(&dir).unwrap();

  assert_eq!(paths, vec![dir.join("shtr.eu.zone")]);
  assert_eq!(zones[0].origin, name("shtr.eu."));
  assert_ne!(before, after);
}
