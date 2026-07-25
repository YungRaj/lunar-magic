use super::{LEGACY_MANIFEST, MANIFEST, MAX_CASES, MAX_DIRECTORIES};
use crate::oracle_input::read_bounded;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn discover(root: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    if !fs::symlink_metadata(root)?.file_type().is_dir() {
        return Err(format!("oracle suite root is not a directory: {}", root.display()).into());
    }
    let mut pending = vec![root.to_path_buf()];
    let mut cases = Vec::new();
    let mut visited = 0_usize;
    while let Some(directory) = pending.pop() {
        visited = visited.saturating_add(1);
        if visited > MAX_DIRECTORIES {
            return Err(format!("oracle suite exceeds {MAX_DIRECTORIES} directories").into());
        }
        let mut children = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        if manifest_path(&directory)?.is_some() {
            cases.push(directory.clone());
            if cases.len() > MAX_CASES {
                return Err(format!("oracle suite exceeds {MAX_CASES} cases").into());
            }
        }
        for child in children.into_iter().rev() {
            if child.file_type()?.is_dir() {
                pending.push(child.path());
            }
        }
    }
    cases.sort();
    Ok(cases)
}

pub(super) fn manifest_path(
    directory: &Path,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let canonical = directory.join(MANIFEST);
    if optional_regular_file(&canonical)? {
        return Ok(Some(canonical));
    }
    let legacy = directory.join(LEGACY_MANIFEST);
    if optional_regular_file(&legacy)? {
        Ok(Some(legacy))
    } else {
        Ok(None)
    }
}

pub(super) fn optional_regular_file(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(format!("oracle fixture is not a regular file: {}", path.display()).into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn read_regular_bounded(
    path: &Path,
    maximum: usize,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if !optional_regular_file(path)? {
        return Err(format!("oracle fixture is missing: {}", path.display()).into());
    }
    Ok(read_bounded(path, maximum)?)
}

pub(super) fn relative_name(root: &Path, directory: &Path) -> String {
    directory
        .strip_prefix(root)
        .ok()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(directory)
        .display()
        .to_string()
}
