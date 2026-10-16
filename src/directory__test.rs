use super::*;

fn names(values: &[&str]) -> Vec<String> {
  values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn one_attribute_is_a_plain_presence_filter() {
  assert_eq!(presence_filter(&names(&["mailAliases"])), "(mailAliases=*)");
}

#[test]
fn several_attributes_are_joined_by_or() {
  assert_eq!(
    presence_filter(&names(&["mailAliases", "mailMaskedAddresses"])),
    "(|(mailAliases=*)(mailMaskedAddresses=*))"
  );
}
