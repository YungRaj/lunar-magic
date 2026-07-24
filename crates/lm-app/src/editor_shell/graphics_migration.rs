use lm_app::AppState;
use lm_project::{GraphicsCompression, GraphicsMigrationOptions};
use lm_rom::RomImage;
use std::ops::Range;

pub(crate) fn migrate_graphics_compression(
    app: &mut AppState,
    target: GraphicsCompression,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    if search.start >= search.end {
        return Err("graphics recompression search range must be nonempty".into());
    }
    let profiled = app.profiled_controller_snapshot()?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())?;
    let policy = profiled.profile.allocation_policy_for_rom(
        search,
        &image,
        profiled.snapshot.identity.internal_header_offset,
    )?;
    let command = lm_app::Command::MigrateGraphicsCompression {
        expected_revision: profiled.snapshot.revision,
        source: profiled.profile.graphics,
        target,
        options: GraphicsMigrationOptions {
            allocation: policy,
            reuse_identical: true,
            erase_fill: 0xff,
            checksum_field: profiled.snapshot.identity.internal_header_offset + 0x1c,
        },
    };
    app.dispatch(command)?;
    Ok(())
}
