use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Hash of a file's bytes when it was read. No public constructor:
/// the only way to get a `Version` is to read or write the file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Version(blake3::Hash);

impl Version {
    pub(crate) fn of(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes))
    }

    /// Whether a version an agent read earlier is still the one on disk.
    #[must_use]
    pub fn matches(&self, expected: &VersionHex) -> bool {
        self.0 == expected.0
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.to_hex().as_str())
    }
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// A version as text, from `--if-version` or JSON. It can only be compared
/// with a [`Version`]; it can't stand in for one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VersionHex(blake3::Hash);

#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
#[error("a version is the 64 hex characters printed by `tk show`")]
pub struct VersionHexError;

impl FromStr for VersionHex {
    type Err = VersionHexError;

    fn from_str(s: &str) -> Result<Self, VersionHexError> {
        blake3::Hash::from_hex(s)
            .map(Self)
            .map_err(|_| VersionHexError)
    }
}

/// Every version read can be passed back as the expected one.
impl From<Version> for VersionHex {
    fn from(v: Version) -> Self {
        Self(v.0)
    }
}

impl fmt::Display for VersionHex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.to_hex().as_str())
    }
}

impl<'de> Deserialize<'de> for VersionHex {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
