//! Chip (IDF "target") names.

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::str::FromStr;

/// Targets ESP-IDF v6.0 supports as non-preview. Used for `idf target list`
/// and for "did you mean" hints. Which of these idf-next tests by default is
/// still an open question in the plan, so names outside this list that look
/// like a chip are accepted and left for the SDK to judge.
pub const KNOWN_CHIPS: &[&str] = &[
    "esp32", "esp32s2", "esp32s3", "esp32c2", "esp32c3", "esp32c5", "esp32c6", "esp32c61",
    "esp32h2", "esp32p4",
];

/// A chip name such as `esp32s3`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Chip(String);

impl Chip {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_known(&self) -> bool {
        KNOWN_CHIPS.contains(&self.0.as_str())
    }
}

impl FromStr for Chip {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = s.strip_prefix("esp32");
        let ok = rest.is_some_and(|r| {
            r.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        });
        if ok {
            Ok(Chip(s.to_owned()))
        } else {
            let lower = s.to_ascii_lowercase();
            let hint = if lower != s && lower.parse::<Chip>().is_ok() {
                format!(" (did you mean {lower:?}?)")
            } else {
                String::new()
            };
            Err(format!(
                "{s:?} is not a chip name; expected one like \"esp32s3\"{hint}"
            ))
        }
    }
}

impl fmt::Display for Chip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Chip {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert!("esp32".parse::<Chip>().is_ok());
        assert!("esp32c61".parse::<Chip>().is_ok());
        assert!("esp8266".parse::<Chip>().is_err());
        let err = "ESP32S3".parse::<Chip>().unwrap_err();
        assert!(err.contains("did you mean \"esp32s3\""), "{err}");
        assert!(KNOWN_CHIPS.iter().all(|c| c.parse::<Chip>().is_ok()));
    }
}
