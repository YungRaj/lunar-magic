use crate::oracle_input::read_bounded;
use lm_level::{MwlFile, MwlSectionKind};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_CORPUS_FILES: usize = 4096;
const SECTION_KINDS: [MwlSectionKind; MwlFile::SECTION_COUNT] = [
    MwlSectionKind::LevelHeader,
    MwlSectionKind::Layer1,
    MwlSectionKind::Layer2,
    MwlSectionKind::Sprites,
    MwlSectionKind::Palette,
    MwlSectionKind::SecondaryExits,
    MwlSectionKind::ExAnimation,
    MwlSectionKind::ExpandedHeader,
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct LengthRange {
    minimum: usize,
    maximum: usize,
}

pub fn audit(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.file_type().is_dir() {
        return Err(format!("MWL corpus root is not a directory: {}", root.display()).into());
    }

    let mut paths = collect_paths(root)?;
    paths.sort();
    if paths.is_empty() {
        return Err(format!("MWL corpus contains no .mwl files: {}", root.display()).into());
    }

    let mut versions = BTreeMap::<u16, usize>::new();
    let mut flags = BTreeMap::<u32, usize>::new();
    let mut ranges = [LengthRange::default(); MwlFile::SECTION_COUNT];
    for (file_index, path) in paths.iter().enumerate() {
        let bytes = read_bounded(path, MwlFile::MAX_FILE_BYTES)?;
        let file = MwlFile::decode(&bytes)
            .map_err(|error| format!("cannot decode {}: {error}", path.display()))?;
        let encoded = file
            .encode()
            .map_err(|error| format!("cannot re-encode {}: {error}", path.display()))?;
        if encoded != bytes {
            return Err(format!(
                "MWL is not an exact canonical round trip: {}",
                path.display()
            )
            .into());
        }
        *versions.entry(file.version).or_default() += 1;
        *flags.entry(file.flags).or_default() += 1;
        for (index, kind) in SECTION_KINDS.into_iter().enumerate() {
            let len = file.section(kind).len();
            let range = &mut ranges[index];
            if file_index == 0 {
                range.minimum = len;
            } else {
                range.minimum = range.minimum.min(len);
            }
            range.maximum = range.maximum.max(len);
        }
    }

    println!("mwl-files: {}", paths.len());
    println!("exact-round-trips: {}", paths.len());
    for (version, count) in versions {
        println!("version-{version:#06x}: {count}");
    }
    for (flag, count) in flags {
        println!("flags-{flag:#010x}: {count}");
    }
    for (kind, range) in SECTION_KINDS.into_iter().zip(ranges) {
        println!(
            "section-{kind:?}: {:#x}..={:#x}",
            range.minimum, range.maximum
        );
    }
    Ok(())
}

fn collect_paths(root: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mwl"))
        {
            if paths.len() == MAX_CORPUS_FILES {
                return Err(format!(
                    "MWL corpus exceeds the {MAX_CORPUS_FILES}-file limit: {}",
                    root.display()
                )
                .into());
            }
            paths.push(path);
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::MwlSectionKind;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-mwl-corpus-{}-{}-{name}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn accepts_exact_canonical_files_and_ignores_other_extensions() {
        let root = directory("valid");
        fs::create_dir(&root).unwrap();
        let mut first = MwlFile::default();
        first.set_section(MwlSectionKind::Layer1, vec![1, 2, 3]);
        fs::write(root.join("Level 000.mwl"), first.encode().unwrap()).unwrap();
        fs::write(
            root.join("Level 001.MWL"),
            MwlFile::default().encode().unwrap(),
        )
        .unwrap();
        fs::write(root.join("notes.txt"), b"ignored").unwrap();
        audit(&root).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_empty_malformed_and_noncanonical_corpora() {
        let empty = directory("empty");
        fs::create_dir(&empty).unwrap();
        assert!(audit(&empty).is_err());
        fs::remove_dir(empty).unwrap();

        let malformed = directory("malformed");
        fs::create_dir(&malformed).unwrap();
        fs::write(malformed.join("bad.mwl"), b"LM").unwrap();
        assert!(audit(&malformed).is_err());
        fs::remove_dir_all(malformed).unwrap();

        let noncanonical = directory("noncanonical");
        fs::create_dir(&noncanonical).unwrap();
        let mut bytes = MwlFile::default().encode().unwrap();
        bytes.extend_from_slice(b"trailing");
        fs::write(noncanonical.join("trailing.mwl"), bytes).unwrap();
        assert!(audit(&noncanonical).is_err());
        fs::remove_dir_all(noncanonical).unwrap();
    }
}
