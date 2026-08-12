#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::{cmp::Ordering, fmt};

pub const MAX_MANIFEST_BYTES: usize = 16 * 1024;
pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateManifest {
    pub version: Version,
    pub target: String,
    pub archive: String,
    pub length: u64,
    pub sha256: [u8; 32],
}

impl UpdateManifest {
    pub fn decode(bytes: &[u8]) -> Result<Self, UpdateError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(UpdateError::ManifestTooLarge(bytes.len()));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| UpdateError::Encoding)?;
        let mut lines = text.lines();
        if lines.next() != Some("LMUPDATE1") {
            return Err(UpdateError::Signature);
        }
        let version = one_field(&mut lines, "version ")?.parse()?;
        let target = portable_component("target", one_field(&mut lines, "target ")?)?;
        let archive = portable_component("archive", one_field(&mut lines, "archive ")?)?;
        let length = one_field(&mut lines, "length ")?
            .parse()
            .map_err(|_| UpdateError::Length)?;
        if length == 0 || length > MAX_ARCHIVE_BYTES {
            return Err(UpdateError::Length);
        }
        let sha256 = decode_digest(one_field(&mut lines, "sha256 ")?)?;
        if lines.next().is_some() {
            return Err(UpdateError::TrailingFields);
        }
        Ok(Self {
            version,
            target,
            archive,
            length,
            sha256,
        })
    }

    pub fn verify_archive(
        &self,
        current: Version,
        target: &str,
        archive: &[u8],
    ) -> Result<(), UpdateError> {
        if self.target != target {
            return Err(UpdateError::Target {
                expected: target.to_owned(),
                actual: self.target.clone(),
            });
        }
        if self.version <= current {
            return Err(UpdateError::NotNewer {
                current,
                offered: self.version,
            });
        }
        if u64::try_from(archive.len()).ok() != Some(self.length) {
            return Err(UpdateError::ArchiveLength);
        }
        let actual: [u8; 32] = Sha256::digest(archive).into();
        if actual != self.sha256 {
            return Err(UpdateError::Digest);
        }
        Ok(())
    }
}

fn one_field<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    prefix: &str,
) -> Result<&'a str, UpdateError> {
    lines
        .next()
        .and_then(|line| line.strip_prefix(prefix))
        .filter(|value| !value.is_empty())
        .ok_or(UpdateError::Fields)
}

fn portable_component(label: &'static str, value: &str) -> Result<String, UpdateError> {
    if value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(UpdateError::Component(label));
    }
    Ok(value.to_owned())
}

fn decode_digest(value: &str) -> Result<[u8; 32], UpdateError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(UpdateError::DigestEncoding);
    }
    let mut digest = [0_u8; 32];
    for (target, pair) in digest.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        let text = std::str::from_utf8(pair).map_err(|_| UpdateError::DigestEncoding)?;
        *target = u8::from_str_radix(text, 16).map_err(|_| UpdateError::DigestEncoding)?;
    }
    Ok(digest)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl std::str::FromStr for Version {
    type Err = UpdateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.strip_prefix('v').unwrap_or(value);
        let mut parts = value.split('.');
        let parse = |part: Option<&str>| {
            let part = part.ok_or(UpdateError::Version)?;
            if part.is_empty() || (part.len() > 1 && part.starts_with('0')) {
                return Err(UpdateError::Version);
            }
            part.parse::<u32>().map_err(|_| UpdateError::Version)
        };
        let version = Self {
            major: parse(parts.next())?,
            minor: parse(parts.next())?,
            patch: parse(parts.next())?,
        };
        if parts.next().is_some() {
            return Err(UpdateError::Version);
        }
        Ok(version)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateError {
    ManifestTooLarge(usize),
    Encoding,
    Signature,
    Fields,
    TrailingFields,
    Component(&'static str),
    Version,
    Length,
    DigestEncoding,
    ArchiveLength,
    Digest,
    Target { expected: String, actual: String },
    NotNewer { current: Version, offered: Version },
}

impl fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "update verification failed: {self:?}")
    }
}

impl std::error::Error for UpdateError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str, target: &str, archive: &[u8]) -> Vec<u8> {
        format!(
            "LMUPDATE1\nversion {version}\ntarget {target}\narchive bundle.tar.gz\nlength {}\nsha256 {:x}\n",
            archive.len(), Sha256::digest(archive)
        )
        .into_bytes()
    }

    #[test]
    fn exact_newer_platform_archive_is_accepted() {
        let archive = b"bounded release archive";
        let parsed = UpdateManifest::decode(&manifest("v1.2.3", "x86_64-test", archive)).unwrap();
        parsed
            .verify_archive("1.2.2".parse().unwrap(), "x86_64-test", archive)
            .unwrap();
        assert_eq!(parsed.version.to_string(), "1.2.3");
    }

    #[test]
    fn stale_wrong_target_length_and_digest_are_rejected() {
        let archive = b"archive";
        let parsed = UpdateManifest::decode(&manifest("2.0.0", "target-a", archive)).unwrap();
        assert!(matches!(
            parsed.verify_archive("2.0.0".parse().unwrap(), "target-a", archive),
            Err(UpdateError::NotNewer { .. })
        ));
        assert!(matches!(
            parsed.verify_archive("1.0.0".parse().unwrap(), "target-b", archive),
            Err(UpdateError::Target { .. })
        ));
        assert_eq!(
            parsed.verify_archive("1.0.0".parse().unwrap(), "target-a", b"short"),
            Err(UpdateError::ArchiveLength)
        );
        assert_eq!(
            parsed.verify_archive("1.0.0".parse().unwrap(), "target-a", b"xxxxxxx"),
            Err(UpdateError::Digest)
        );
    }

    #[test]
    fn malformed_or_ambiguous_manifests_fail_closed() {
        for bytes in [
            b"LMUPDATE0\n".as_slice(),
            b"LMUPDATE1\nversion 01.2.3\ntarget ok\narchive a\nlength 1\nsha256 00\n",
            b"LMUPDATE1\nversion 1.2.3\ntarget ../bad\narchive a\nlength 1\nsha256 0000000000000000000000000000000000000000000000000000000000000000\n",
            b"LMUPDATE1\nversion 1.2.3\ntarget ok\narchive a\nlength 1\nsha256 0000000000000000000000000000000000000000000000000000000000000000\nextra x\n",
        ] {
            assert!(UpdateManifest::decode(bytes).is_err());
        }
        assert!(matches!(
            UpdateManifest::decode(&vec![b'x'; MAX_MANIFEST_BYTES + 1]),
            Err(UpdateError::ManifestTooLarge(_))
        ));
    }
}
