use crate::{atomic_output::write_new, oracle_input::read_rom};
use lm_oracle::observe_exanimation_slot_options;
use lm_project::{ExAnimationSlotOptionRomLayout, Project};
use lm_rom::RomImage;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    let crate::command_types::Command::ExAnimationSlotOptionsObserve {
        rom,
        mapper,
        pointer,
        output,
    } = command
    else {
        return Ok(false);
    };
    if rom == output {
        return Err("ExAnimation slot-option observation output must differ from ROM input".into());
    }
    let project = Project::new(RomImage::from_bytes(read_rom(rom)?)?);
    let loaded = project.load_exanimation_slot_options(ExAnimationSlotOptionRomLayout {
        mapper: *mapper,
        pointer_offset: *pointer,
    })?;
    let observation = observe_exanimation_slot_options(&loaded.table)?;
    write_new(output, observation.to_text().as_bytes())?;
    println!("observed-exanimation-slot-options: 7");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::Mapper;
    use std::path::PathBuf;

    #[test]
    fn aliases_are_rejected_before_file_access() {
        let same = PathBuf::from("same");
        assert!(
            execute_command(
                &crate::command_types::Command::ExAnimationSlotOptionsObserve {
                    rom: same.clone(),
                    mapper: Mapper::LoRom,
                    pointer: 0,
                    output: same,
                }
            )
            .is_err()
        );
    }
}
