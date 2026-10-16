use super::*;

#[test]
fn splits_and_trims_the_list() {
  assert_eq!(
    attribute_list(" mailAliases , mailMaskedAddresses ,").unwrap(),
    vec!["mailAliases", "mailMaskedAddresses"]
  );
}

#[test]
fn refuses_an_empty_list() {
  assert!(attribute_list(" , ").is_err());
}

#[test]
fn refuses_names_that_would_change_the_filter() {
  assert!(attribute_list("mailAliases=*)(uid").is_err());
  assert!(attribute_list("1mail").is_err());
}
