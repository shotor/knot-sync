use std::fmt::Display;
use std::str::FromStr;

use anyhow::bail;

pub fn value_or(name: &str, default: &str) -> String {
  std::env::var(name).unwrap_or_else(|_| default.into())
}

pub fn optional_value(name: &str) -> Option<String> {
  std::env::var(name)
    .ok()
    .filter(|value| !value.trim().is_empty())
}

pub fn required_value(name: &str) -> anyhow::Result<String> {
  match optional_value(name) {
    Some(value) => Ok(value),
    None => bail!("{name} is required"),
  }
}

pub fn parse_or<T>(name: &str, default: &str) -> anyhow::Result<T>
where
  T: FromStr,
  T::Err: Display,
{
  value_or(name, default)
    .parse()
    .map_err(|error| anyhow::anyhow!("{name}: {error}"))
}
