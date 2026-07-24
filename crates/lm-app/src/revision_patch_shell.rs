use crate::{
    application_rom_commands::read_bounded_bytes, editor_shell::read_bounded_utf8,
    revision_patch_install_spec, spec_text,
};
use lm_app::{AppState, Command};
use lm_profile::RevisionPatchTemplate;
use std::path::Path;

pub(crate) fn install_revision_patch(
    app: &mut AppState,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "revision patch installation specification",
    )?;
    let spec = revision_patch_install_spec::parse(&text, spec_path)?;
    let template = RevisionPatchTemplate::decode(&read_bounded_bytes(
        &spec.template,
        RevisionPatchTemplate::MAX_FILE_LEN,
        "revision patch template",
    )?)?;
    let snapshot = app.controller_snapshot()?;
    app.dispatch(Command::InstallRevisionPatch {
        expected_revision: snapshot.revision,
        template: Box::new(template),
        search: spec.search,
        fill: spec.fill,
    })?;
    Ok(())
}
