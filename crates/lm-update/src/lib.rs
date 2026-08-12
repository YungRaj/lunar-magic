#![forbid(unsafe_code)]

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::BTreeSet,
    fmt, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_MANIFEST_BYTES: usize = 16 * 1024;
pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_EXTRACTED_BYTES: u64 = 768 * 1024 * 1024;
const TAR_BLOCK: usize = 512;

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
        self.verify_offer(current, target)?;
        if u64::try_from(archive.len()).ok() != Some(self.length) {
            return Err(UpdateError::ArchiveLength);
        }
        let actual: [u8; 32] = Sha256::digest(archive).into();
        if actual != self.sha256 {
            return Err(UpdateError::Digest);
        }
        Ok(())
    }

    pub fn verify_archive_reader(
        &self,
        current: Version,
        target: &str,
        mut archive: impl Read,
    ) -> Result<(), UpdateError> {
        self.verify_offer(current, target)?;
        let mut hasher = Sha256::new();
        let mut remaining = self.length;
        let mut buffer = [0_u8; 64 * 1024];
        while remaining != 0 {
            let request = usize::try_from(remaining.min(buffer.len() as u64))
                .map_err(|_| UpdateError::ArchiveRead("read size overflow".into()))?;
            let read = archive
                .read(&mut buffer[..request])
                .map_err(|error| UpdateError::ArchiveRead(error.to_string()))?;
            if read == 0 {
                return Err(UpdateError::ArchiveLength);
            }
            hasher.update(&buffer[..read]);
            remaining -= u64::try_from(read).unwrap_or(u64::MAX);
        }
        let mut trailing = [0_u8; 1];
        if archive
            .read(&mut trailing)
            .map_err(|error| UpdateError::ArchiveRead(error.to_string()))?
            != 0
        {
            return Err(UpdateError::ArchiveLength);
        }
        if <[u8; 32]>::from(hasher.finalize()) != self.sha256 {
            return Err(UpdateError::Digest);
        }
        Ok(())
    }

    /// Verifies and durably stages an update archive without replacing an existing path.
    /// Verification completes before the destination is opened.
    pub fn stage_archive(
        &self,
        current: Version,
        target: &str,
        archive: &[u8],
        directory: &Path,
    ) -> Result<PathBuf, UpdateError> {
        self.verify_archive(current, target, archive)?;
        self.stage_archive_reader(current, target, std::io::Cursor::new(archive), directory)
    }

    /// Streams one verified archive into durable create-new storage without buffering it whole.
    pub fn stage_archive_reader(
        &self,
        current: Version,
        target: &str,
        mut archive: impl Read,
        directory: &Path,
    ) -> Result<PathBuf, UpdateError> {
        self.verify_offer(current, target)?;
        let metadata = fs::metadata(directory).map_err(|error| UpdateError::StageIo {
            operation: "inspect destination directory",
            message: error.to_string(),
        })?;
        if !metadata.is_dir() {
            return Err(UpdateError::StageDirectory);
        }
        let destination = directory.join(&self.archive);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|error| UpdateError::StageIo {
                operation: "create staged archive",
                message: error.to_string(),
            })?;
        let mut hasher = Sha256::new();
        let mut remaining = self.length;
        let mut buffer = [0_u8; 64 * 1024];
        let write_result = (|| {
            while remaining != 0 {
                let request = usize::try_from(remaining.min(buffer.len() as u64))
                    .map_err(|_| std::io::Error::other("update read size overflow"))?;
                let read = archive.read(&mut buffer[..request])?;
                if read == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "update archive ended before its declared length",
                    ));
                }
                file.write_all(&buffer[..read])?;
                hasher.update(&buffer[..read]);
                remaining -= u64::try_from(read).unwrap_or(u64::MAX);
            }
            let mut trailing = [0_u8; 1];
            if archive.read(&mut trailing)? != 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "update archive exceeds its declared length",
                ));
            }
            file.sync_all()
        })();
        if let Err(error) = write_result {
            drop(file);
            let _cleanup = fs::remove_file(&destination);
            return Err(UpdateError::StageIo {
                operation: "write staged archive",
                message: error.to_string(),
            });
        }
        if <[u8; 32]>::from(hasher.finalize()) != self.sha256 {
            drop(file);
            let _cleanup = fs::remove_file(&destination);
            return Err(UpdateError::Digest);
        }
        drop(file);
        let reopened = match fs::read(&destination) {
            Ok(reopened) => reopened,
            Err(error) => {
                let _cleanup = fs::remove_file(&destination);
                return Err(UpdateError::StageIo {
                    operation: "reopen staged archive",
                    message: error.to_string(),
                });
            }
        };
        if let Err(error) = self.verify_archive(current, target, &reopened) {
            let _cleanup = fs::remove_file(&destination);
            return Err(error);
        }
        Ok(destination)
    }

    fn verify_offer(&self, current: Version, target: &str) -> Result<(), UpdateError> {
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
        Ok(())
    }

    /// Extracts a staged portable bundle into one brand-new versioned directory.
    pub fn extract_staged_archive(
        &self,
        staged_archive: &Path,
        install_root: &Path,
    ) -> Result<PathBuf, UpdateError> {
        let prefix = format!("lunar-magic-rust-{}-{}", self.version, self.target);
        let destination = install_root.join(&prefix);
        fs::create_dir(&destination).map_err(|error| UpdateError::InstallIo(error.to_string()))?;
        let result = self.extract_into(staged_archive, &destination, &prefix);
        if let Err(error) = result {
            let _cleanup = fs::remove_dir_all(&destination);
            return Err(error);
        }
        Ok(destination)
    }

    fn extract_into(
        &self,
        staged_archive: &Path,
        destination: &Path,
        prefix: &str,
    ) -> Result<(), UpdateError> {
        let file = fs::File::open(staged_archive)
            .map_err(|error| UpdateError::InstallIo(error.to_string()))?;
        let mut tar = GzDecoder::new(file);
        let mut seen = BTreeSet::new();
        let mut total = 0_u64;
        loop {
            let mut header = [0_u8; TAR_BLOCK];
            tar.read_exact(&mut header)
                .map_err(|error| UpdateError::InstallArchive(error.to_string()))?;
            if header.iter().all(|byte| *byte == 0) {
                let mut second = [0_u8; TAR_BLOCK];
                tar.read_exact(&mut second)
                    .map_err(|error| UpdateError::InstallArchive(error.to_string()))?;
                if second.iter().any(|byte| *byte != 0) {
                    return Err(UpdateError::InstallArchive("invalid tar terminator".into()));
                }
                break;
            }
            validate_tar_checksum(&header)?;
            if header[156] != b'0' && header[156] != 0 {
                return Err(UpdateError::InstallArchive("non-regular tar entry".into()));
            }
            let name = tar_text(&header[..100])?;
            let child = name
                .strip_prefix(prefix)
                .and_then(|suffix| suffix.strip_prefix('/'))
                .filter(|child| !child.is_empty() && !child.contains('/'))
                .ok_or_else(|| UpdateError::InstallArchive("entry escapes bundle prefix".into()))?;
            portable_component("bundle entry", child)?;
            if !seen.insert(child.to_owned()) {
                return Err(UpdateError::InstallArchive("duplicate tar entry".into()));
            }
            let size = tar_octal(&header[124..136])?;
            total = total.checked_add(size).ok_or(UpdateError::InstallLimit)?;
            if size > MAX_ARCHIVE_BYTES || total > MAX_EXTRACTED_BYTES {
                return Err(UpdateError::InstallLimit);
            }
            let path = destination.join(child);
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|error| UpdateError::InstallIo(error.to_string()))?;
            copy_exact(&mut tar, &mut output, size)?;
            output
                .sync_all()
                .map_err(|error| UpdateError::InstallIo(error.to_string()))?;
            let padding = (TAR_BLOCK as u64 - size % TAR_BLOCK as u64) % TAR_BLOCK as u64;
            discard_exact(&mut tar, padding)?;
        }
        let suffix = if self.target.contains("windows") {
            ".exe"
        } else {
            ""
        };
        for required in [
            format!("lm-native{suffix}"),
            format!("lm-cli{suffix}"),
            format!("lm-libretro{suffix}"),
            "RELEASE-MANIFEST.txt".into(),
        ] {
            if !seen.contains(&required) {
                return Err(UpdateError::InstallArchive(format!(
                    "missing required entry {required}"
                )));
            }
        }
        Ok(())
    }
}

fn tar_text(field: &[u8]) -> Result<&str, UpdateError> {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    std::str::from_utf8(&field[..end])
        .map_err(|_| UpdateError::InstallArchive("non-UTF-8 tar name".into()))
}

fn tar_octal(field: &[u8]) -> Result<u64, UpdateError> {
    let text = tar_text(field)?.trim().trim_start_matches('0');
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|_| UpdateError::InstallArchive("invalid tar size".into()))
}

fn validate_tar_checksum(header: &[u8; TAR_BLOCK]) -> Result<(), UpdateError> {
    let expected = tar_octal(&header[148..156])?;
    let actual: u64 = header
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            if (148..156).contains(&index) {
                u64::from(b' ')
            } else {
                u64::from(*byte)
            }
        })
        .sum();
    if expected != actual {
        return Err(UpdateError::InstallArchive("invalid tar checksum".into()));
    }
    Ok(())
}

fn copy_exact(
    input: &mut impl Read,
    output: &mut impl Write,
    mut remaining: u64,
) -> Result<(), UpdateError> {
    let mut buffer = [0_u8; 64 * 1024];
    while remaining != 0 {
        let request = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| UpdateError::InstallLimit)?;
        let read = input
            .read(&mut buffer[..request])
            .map_err(|error| UpdateError::InstallArchive(error.to_string()))?;
        if read == 0 {
            return Err(UpdateError::InstallArchive("truncated tar entry".into()));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| UpdateError::InstallIo(error.to_string()))?;
        remaining -= u64::try_from(read).unwrap_or(u64::MAX);
    }
    Ok(())
}

fn discard_exact(input: &mut impl Read, size: u64) -> Result<(), UpdateError> {
    copy_exact(input, &mut std::io::sink(), size)
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
    ArchiveRead(String),
    InstallArchive(String),
    InstallIo(String),
    InstallLimit,
    StageDirectory,
    StageIo {
        operation: &'static str,
        message: String,
    },
    Target {
        expected: String,
        actual: String,
    },
    NotNewer {
        current: Version,
        offered: Version,
    },
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
    use flate2::{Compression, write::GzEncoder};

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

    #[test]
    fn verified_archive_stages_exactly_and_never_replaces_a_collision() {
        let directory = tempfile::tempdir().unwrap();
        let archive = b"complete verified archive";
        let parsed = UpdateManifest::decode(&manifest("2.0.0", "target-a", archive)).unwrap();
        let path = parsed
            .stage_archive(
                "1.0.0".parse().unwrap(),
                "target-a",
                archive,
                directory.path(),
            )
            .unwrap();
        assert_eq!(path, directory.path().join("bundle.tar.gz"));
        assert_eq!(fs::read(&path).unwrap(), archive);
        assert!(matches!(
            parsed.stage_archive(
                "1.0.0".parse().unwrap(),
                "target-a",
                archive,
                directory.path(),
            ),
            Err(UpdateError::StageIo { .. })
        ));
        assert_eq!(fs::read(path).unwrap(), archive);
    }

    #[test]
    fn verification_failure_creates_no_staged_file() {
        let directory = tempfile::tempdir().unwrap();
        let archive = b"complete verified archive";
        let parsed = UpdateManifest::decode(&manifest("2.0.0", "target-a", archive)).unwrap();
        assert_eq!(
            parsed.stage_archive(
                "1.0.0".parse().unwrap(),
                "target-a",
                b"tampered archive bytes",
                directory.path(),
            ),
            Err(UpdateError::ArchiveLength)
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);

        let not_directory = directory.path().join("ordinary-file");
        fs::write(&not_directory, b"preserve").unwrap();
        assert_eq!(
            parsed.stage_archive(
                "1.0.0".parse().unwrap(),
                "target-a",
                archive,
                &not_directory,
            ),
            Err(UpdateError::StageDirectory)
        );
        assert_eq!(fs::read(not_directory).unwrap(), b"preserve");
    }

    #[test]
    fn streamed_staging_rejects_truncation_trailing_bytes_and_equal_length_tampering() {
        let archive = b"complete verified archive";
        let parsed = UpdateManifest::decode(&manifest("2.0.0", "target-a", archive)).unwrap();
        for rejected in [
            archive[..archive.len() - 1].to_vec(),
            [archive.as_slice(), b"x"].concat(),
            vec![b'x'; archive.len()],
        ] {
            let directory = tempfile::tempdir().unwrap();
            assert!(
                parsed
                    .stage_archive_reader(
                        "1.0.0".parse().unwrap(),
                        "target-a",
                        std::io::Cursor::new(rejected),
                        directory.path(),
                    )
                    .is_err()
            );
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn streaming_preflight_matches_byte_verification_and_rejects_framing() {
        let archive = b"complete verified archive";
        let parsed = UpdateManifest::decode(&manifest("2.0.0", "target-a", archive)).unwrap();
        parsed
            .verify_archive_reader(
                "1.0.0".parse().unwrap(),
                "target-a",
                std::io::Cursor::new(archive),
            )
            .unwrap();
        for rejected in [
            archive[..archive.len() - 1].to_vec(),
            [archive.as_slice(), b"x"].concat(),
            vec![b'x'; archive.len()],
        ] {
            assert!(
                parsed
                    .verify_archive_reader(
                        "1.0.0".parse().unwrap(),
                        "target-a",
                        std::io::Cursor::new(rejected),
                    )
                    .is_err()
            );
        }
    }

    fn bundle(entries: &[(&str, &[u8])]) -> Vec<u8> {
        fn octal(field: &mut [u8], value: u64) {
            let text = format!("{value:o}");
            field.fill(b'0');
            let start = field.len() - text.len() - 1;
            field[start..start + text.len()].copy_from_slice(text.as_bytes());
            field[field.len() - 1] = 0;
        }
        let mut tar = Vec::new();
        for (name, bytes) in entries {
            let mut header = [0_u8; TAR_BLOCK];
            header[..name.len()].copy_from_slice(name.as_bytes());
            octal(&mut header[100..108], 0o644);
            octal(&mut header[124..136], bytes.len() as u64);
            header[148..156].fill(b' ');
            header[156] = b'0';
            header[257..263].copy_from_slice(b"ustar\0");
            header[263..265].copy_from_slice(b"00");
            let sum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
            let checksum = format!("{sum:06o}");
            header[148..154].copy_from_slice(checksum.as_bytes());
            header[154] = 0;
            header[155] = b' ';
            tar.extend_from_slice(&header);
            tar.extend_from_slice(bytes);
            tar.resize(tar.len().next_multiple_of(TAR_BLOCK), 0);
        }
        tar.resize(tar.len() + TAR_BLOCK * 2, 0);
        let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
        gzip.write_all(&tar).unwrap();
        gzip.finish().unwrap()
    }

    #[test]
    fn portable_bundle_extracts_into_one_create_new_version_directory() {
        let prefix = "lunar-magic-rust-2.0.0-target-a";
        let bytes = bundle(&[
            (&format!("{prefix}/lm-native"), b"native"),
            (&format!("{prefix}/lm-cli"), b"cli"),
            (&format!("{prefix}/lm-libretro"), b"backend"),
            (&format!("{prefix}/RELEASE-MANIFEST.txt"), b"manifest"),
        ]);
        let source = tempfile::tempdir().unwrap();
        let archive = source.path().join("bundle.tar.gz");
        fs::write(&archive, bytes).unwrap();
        let root = tempfile::tempdir().unwrap();
        let parsed = UpdateManifest::decode(&manifest("2.0.0", "target-a", b"x")).unwrap();
        let installed = parsed
            .extract_staged_archive(&archive, root.path())
            .unwrap();
        assert_eq!(fs::read(installed.join("lm-native")).unwrap(), b"native");
        assert!(
            parsed
                .extract_staged_archive(&archive, root.path())
                .is_err()
        );
        assert_eq!(fs::read(installed.join("lm-native")).unwrap(), b"native");
    }

    #[test]
    fn invalid_bundle_path_or_missing_runtime_cleans_install_directory() {
        let parsed = UpdateManifest::decode(&manifest("2.0.0", "target-a", b"x")).unwrap();
        for entries in [
            vec![("../escape", b"bad".as_slice())],
            vec![(
                "lunar-magic-rust-2.0.0-target-a/lm-native",
                b"only".as_slice(),
            )],
        ] {
            let source = tempfile::tempdir().unwrap();
            let archive = source.path().join("bundle.tar.gz");
            fs::write(&archive, bundle(&entries)).unwrap();
            let root = tempfile::tempdir().unwrap();
            assert!(
                parsed
                    .extract_staged_archive(&archive, root.path())
                    .is_err()
            );
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        }
    }
}
