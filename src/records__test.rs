use super::*;

fn name(value: &str) -> Name {
  Name::from_ascii(value).unwrap().to_lowercase()
}

fn labels(values: &[&str]) -> BTreeSet<String> {
  values.iter().map(|value| (*value).to_owned()).collect()
}

fn a(value: &str) -> RData {
  RData::A(value.parse().unwrap())
}

fn txt(value: &str) -> RData {
  RData::TXT(TXT::new(vec![value.to_owned()]))
}

fn rrsets(entries: Vec<(&str, RecordType, u32, Vec<RData>)>) -> Rrsets {
  entries
    .into_iter()
    .map(|(owner, record_type, ttl, data)| ((name(owner), record_type), Rrset { ttl, data }))
    .collect()
}

#[test]
fn wants_one_label_below_the_zone() {
  let addresses = [
    "shop@a1b2.shtr.eu",
    "news@C3D4.SHTR.EU",
    "me@shtr.eu",
    "me@shtr.nl",
    "x@deep.a1b2.shtr.eu",
    "x@a1b2.shtr.eu.",
  ];

  assert_eq!(
    wanted_labels(addresses, "shtr.eu."),
    labels(&["a1b2", "c3d4"])
  );
}

#[test]
fn never_wants_underscore_names() {
  let addresses = [
    "x@_dmarc.shtr.eu",
    "x@_acme-challenge.shtr.eu",
    "x@-a.shtr.eu",
  ];

  assert!(wanted_labels(addresses, "shtr.eu").is_empty());
}

#[test]
fn ignores_values_without_an_at() {
  assert!(wanted_labels(["a1b2.shtr.eu"], "shtr.eu").is_empty());
}

#[test]
fn leaves_knot_and_cert_manager_their_records() {
  assert!(!is_managed(&name("shtr.eu."), RecordType::SOA));
  assert!(!is_managed(&name("shtr.eu."), RecordType::DNSKEY));
  assert!(!is_managed(&name("a1b2.shtr.eu."), RecordType::RRSIG));
  assert!(!is_managed(
    &name("_acme-challenge.shtr.eu."),
    RecordType::TXT
  ));
  assert!(!is_managed(
    &name("_ACME-Challenge.www.shtr.eu."),
    RecordType::TXT
  ));
  assert!(is_managed(
    &name("_acme-challenge.shtr.eu."),
    RecordType::CNAME
  ));
  assert!(is_managed(&name("_dmarc.shtr.eu."), RecordType::TXT));
  assert!(is_managed(&name("shtr.eu."), RecordType::NS));
}

#[test]
fn adds_contact_records_beside_the_file() {
  let file = rrsets(vec![(
    "www.shtr.eu.",
    RecordType::A,
    3600,
    vec![a("192.0.2.1")],
  )]);
  let wanted = wanted(
    &name("shtr.eu."),
    &file,
    &labels(&["a1b2"]),
    &name("mail.shotor.com."),
  )
  .unwrap();

  assert_eq!(wanted.len(), 3);
  assert_eq!(
    wanted[&(name("a1b2.shtr.eu."), RecordType::MX)],
    Rrset {
      ttl: CONTACT_TTL,
      data: vec![RData::MX(MX::new(10, name("mail.shotor.com.")))],
    }
  );
  assert_eq!(
    wanted[&(name("a1b2.shtr.eu."), RecordType::TXT)].data,
    vec![txt(SPF)]
  );
}

#[test]
fn the_file_decides_at_its_own_names() {
  let file = rrsets(vec![(
    "mail.shtr.eu.",
    RecordType::A,
    3600,
    vec![a("192.0.2.1")],
  )]);
  let wanted = wanted(
    &name("shtr.eu."),
    &file,
    &labels(&["mail"]),
    &name("mail.shotor.com."),
  )
  .unwrap();

  assert_eq!(wanted, file);
}

#[test]
fn drops_unmanaged_records_from_the_file() {
  let file = rrsets(vec![
    (
      "shtr.eu.",
      RecordType::NS,
      3600,
      vec![RData::NS(hickory_client::proto::rr::rdata::NS(name(
        "defsn0.shtr.nl.",
      )))],
    ),
    (
      "_acme-challenge.shtr.eu.",
      RecordType::TXT,
      60,
      vec![txt("x")],
    ),
  ]);
  let wanted = wanted(
    &name("shtr.eu."),
    &file,
    &labels(&[]),
    &name("mail.shotor.com."),
  )
  .unwrap();

  assert_eq!(wanted.len(), 1);
}

#[test]
fn nothing_to_do_when_in_step() {
  let wanted = rrsets(vec![(
    "www.shtr.eu.",
    RecordType::A,
    300,
    vec![a("192.0.2.1"), a("192.0.2.2")],
  )]);
  let present = rrsets(vec![
    (
      "www.shtr.eu.",
      RecordType::A,
      300,
      vec![a("192.0.2.2"), a("192.0.2.1")],
    ),
    ("shtr.eu.", RecordType::DNSKEY, 300, vec![]),
    (
      "_acme-challenge.shtr.eu.",
      RecordType::TXT,
      60,
      vec![txt("token")],
    ),
  ]);

  assert!(diff(&wanted, &present).is_empty());
}

#[test]
fn changes_an_rrset_record_by_record() {
  let wanted = rrsets(vec![(
    "www.shtr.eu.",
    RecordType::A,
    300,
    vec![a("192.0.2.1"), a("192.0.2.3")],
  )]);
  let present = rrsets(vec![(
    "www.shtr.eu.",
    RecordType::A,
    300,
    vec![a("192.0.2.1"), a("192.0.2.2")],
  )]);

  assert_eq!(
    diff(&wanted, &present),
    Changes {
      add: vec![(
        name("www.shtr.eu."),
        RecordType::A,
        300,
        vec![a("192.0.2.3")]
      )],
      remove: vec![(name("www.shtr.eu."), RecordType::A, vec![a("192.0.2.2")])],
      delete: vec![],
    }
  );
}

#[test]
fn a_new_ttl_adds_every_record_again() {
  let wanted = rrsets(vec![(
    "www.shtr.eu.",
    RecordType::A,
    600,
    vec![a("192.0.2.1")],
  )]);
  let present = rrsets(vec![(
    "www.shtr.eu.",
    RecordType::A,
    300,
    vec![a("192.0.2.1")],
  )]);

  assert_eq!(
    diff(&wanted, &present),
    Changes {
      add: vec![(
        name("www.shtr.eu."),
        RecordType::A,
        600,
        vec![a("192.0.2.1")]
      )],
      remove: vec![],
      delete: vec![],
    }
  );
}

#[test]
fn deletes_what_nothing_wants_and_counts_it() {
  let wanted = rrsets(vec![]);
  let present = rrsets(vec![
    (
      "dead.shtr.eu.",
      RecordType::MX,
      300,
      vec![RData::MX(MX::new(10, name("mail.shotor.com.")))],
    ),
    (
      "old.shtr.eu.",
      RecordType::A,
      300,
      vec![a("192.0.2.1"), a("192.0.2.2")],
    ),
    (
      "_acme-challenge.shtr.eu.",
      RecordType::TXT,
      60,
      vec![txt("token")],
    ),
    ("shtr.eu.", RecordType::SOA, 300, vec![]),
  ]);
  let changes = diff(&wanted, &present);

  assert_eq!(
    changes.delete,
    vec![
      (name("dead.shtr.eu."), RecordType::MX),
      (name("old.shtr.eu."), RecordType::A),
    ]
  );
  assert_eq!(changes.deleted_records(&present), 3);
}

#[test]
fn puts_a_changed_dmarc_back() {
  let wanted = rrsets(vec![(
    "_dmarc.shtr.eu.",
    RecordType::TXT,
    300,
    vec![txt("v=DMARC1; p=reject")],
  )]);
  let present = rrsets(vec![(
    "_dmarc.shtr.eu.",
    RecordType::TXT,
    300,
    vec![txt("v=DMARC1; p=none")],
  )]);
  let changes = diff(&wanted, &present);

  assert_eq!(changes.add.len(), 1);
  assert_eq!(changes.remove.len(), 1);
}
