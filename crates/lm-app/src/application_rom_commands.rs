use crate::{portable_document_sessions::PortableDocumentSessions, tool_shell};
use lm_app::{AppState, Command, FrontendEffect, RevisionProfile, RomExpansionCommand};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub(crate) const MAX_ROM_BYTES: usize = 32 * 1024 * 1024;

pub(crate) fn read_bounded_bytes(
    path: &Path,
    maximum: usize,
    kind: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let path_metadata = fs::symlink_metadata(path)?;
    if !path_metadata.file_type().is_file() {
        return Err(format!("{kind} must be a regular file").into());
    }
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(format!("{kind} must be a regular file").into());
    }
    if metadata.len() > u64::try_from(maximum).unwrap_or(u64::MAX) {
        return Err(format!("{kind} exceeds the bounded file limit").into());
    }
    let mut bytes = Vec::new();
    file.take(u64::try_from(maximum).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(format!("{kind} exceeds the bounded file limit").into());
    }
    Ok(bytes)
}

pub(crate) fn open_and_print(
    app: &mut AppState,
    lines: &mut impl Iterator<Item = io::Result<String>>,
    path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    open(app, lines, path)?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn close_and_print(
    app: &mut AppState,
    lines: &mut impl Iterator<Item = io::Result<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    request_close(app, lines, false)?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn install_profile(
    app: &mut AppState,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let profile = RevisionProfile::read_from(fs::File::open(path)?)?;
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile)))?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn show_status(app: &AppState) {
    println!(
        "{} — {:?} — profile: {}",
        app.status,
        app.mode,
        app.revision_profile()
            .map_or("none", |profile| profile.name.as_str())
    );
}

pub(crate) fn select_asset(
    app: &mut AppState,
    kind: &str,
    value: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let command = match kind {
        "graphics" => Command::ShowGraphics(value),
        "palette" => Command::ShowPalette(value),
        "exanimation" => Command::ShowExAnimation(value),
        "layer3" => Command::ShowLayer3(value),
        _ => return Err(format!("unknown asset editor {kind}").into()),
    };
    app.dispatch(command)?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn expand_rom(
    app: &mut AppState,
    target_logical_len: usize,
    fill: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = app.controller_snapshot()?;
    app.dispatch(Command::ExpandRom(RomExpansionCommand {
        expected_revision: snapshot.revision,
        mapper: snapshot.identity.mapper,
        target_logical_len,
        fill,
        checksum_field: snapshot.identity.internal_header_offset + 0x1c,
    }))?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn show_recent(app: &AppState) {
    for (index, path) in app.recent_documents().paths().iter().enumerate() {
        println!("{index}: {}", path.display());
    }
}

pub(crate) fn open_recent(
    app: &mut AppState,
    lines: &mut impl Iterator<Item = io::Result<String>>,
    index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = app
        .recent_documents()
        .paths()
        .get(index)
        .cloned()
        .ok_or("recent-document index is out of range")?;
    open(app, lines, path)?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn request_close(
    app: &mut AppState,
    lines: &mut impl Iterator<Item = io::Result<String>>,
    quit_after: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let effects = app.dispatch(if quit_after {
        Command::Quit
    } else {
        Command::Close
    })?;
    if quit_after && effects.contains(&FrontendEffect::QuitApplication) {
        return Ok(true);
    }
    if !quit_after && effects.contains(&FrontendEffect::ProjectClosed) {
        return Ok(false);
    }
    if effects.contains(&FrontendEffect::ConfirmDiscardChanges { quit_after }) {
        print!("Discard unsaved changes? [y/N] ");
        io::stdout().flush()?;
        let answer = lines.next().transpose()?.unwrap_or_default();
        if answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes") {
            return Ok(app
                .discard_and_close(quit_after)
                .contains(&FrontendEffect::QuitApplication));
        }
    }
    Ok(false)
}

pub(crate) fn request_quit(
    app: &mut AppState,
    documents: &mut PortableDocumentSessions,
    lines: &mut impl Iterator<Item = io::Result<String>>,
) -> Result<bool, Box<dyn std::error::Error>> {
    if !request_close(app, lines, true)? {
        return Ok(false);
    }
    let dirty = documents.dirty_documents();
    if dirty.is_empty() {
        documents.discard_all();
        return Ok(true);
    }
    print!(
        "Discard unsaved portable documents ({})? [y/N] ",
        dirty.join(", ")
    );
    io::stdout().flush()?;
    let answer = lines.next().transpose()?.unwrap_or_default();
    if answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes") {
        documents.discard_all();
        return Ok(true);
    }
    Ok(false)
}

fn open(
    app: &mut AppState,
    lines: &mut impl Iterator<Item = io::Result<String>>,
    path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut effects = app.dispatch(Command::Open)?;
    if effects.contains(&FrontendEffect::ConfirmDiscardAndOpen) {
        print!("Discard unsaved changes and open another ROM? [y/N] ");
        io::stdout().flush()?;
        let answer = lines.next().transpose()?.unwrap_or_default();
        if !(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes")) {
            return Ok(());
        }
        effects = app.discard_and_request_open()?;
    }
    let request_id = effects
        .iter()
        .find_map(|effect| match effect {
            FrontendEffect::ChooseRom { request_id } => Some(*request_id),
            _ => None,
        })
        .ok_or("application did not issue an open request")?;
    match read_bounded_bytes(&path, MAX_ROM_BYTES, "ROM") {
        Ok(bytes) => {
            let event_effects = app.complete_open(request_id, bytes, Some(path))?;
            tool_shell::print_event_invocations(&event_effects);
        }
        Err(error) => {
            app.cancel_open(request_id)?;
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_rom_open_cancels_the_pending_request() {
        let path =
            std::env::temp_dir().join(format!("lm-app-oversized-open-{}.smc", std::process::id()));
        let _ = fs::remove_file(&path);
        fs::File::create(&path)
            .unwrap()
            .set_len(u64::try_from(MAX_ROM_BYTES + 1).unwrap())
            .unwrap();
        let mut app = AppState::default();
        let mut lines = std::iter::empty();
        assert!(open(&mut app, &mut lines, path.clone()).is_err());
        assert!(
            app.dispatch(Command::Open)
                .unwrap()
                .iter()
                .any(|effect| matches!(effect, FrontendEffect::ChooseRom { .. }))
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn bounded_reader_accepts_only_regular_files() {
        let directory =
            std::env::temp_dir().join(format!("lm-app-bounded-input-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        assert!(read_bounded_bytes(&directory, 16, "input").is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let target = directory.join("target");
            let link = directory.join("link");
            fs::write(&target, [1, 2, 3]).unwrap();
            symlink(&target, &link).unwrap();
            assert!(read_bounded_bytes(&link, 16, "input").is_err());
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
