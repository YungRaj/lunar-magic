//! SMW US revision-0 Lunar Magic 1.10 expanded overworld-message runtime.

use lm_overworld::{
    OverworldMessage, VanillaOverworldMessageError, decode_vanilla_overworld_messages,
};
use lm_project::{
    ExpandedOverworldMessageStorage, OverworldMessagePatchError, OverworldMessagePatchLocator,
    PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, Project, RelocatablePatchPlan,
};
use lm_rats::AllocationPolicy;
use lm_rom::{Mapper, RomError};

use crate::SMW_US_V1_CHECKSUM_FIELD;

pub const SMW_US_V1_OVERWORLD_MESSAGE_RUNTIME_OFFSET: usize = 0x01_bd90;
pub const SMW_US_V1_OVERWORLD_MESSAGE_HOOK_OFFSET: usize = 0x01_c080;
pub const SMW_US_V1_OVERWORLD_MESSAGE_SEARCH_START: usize = 0x08_0000;
pub const SMW_US_V1_OVERWORLD_MESSAGE_SEARCH_END: usize = 0x10_0000;
pub const SMW_US_V1_OVERWORLD_MESSAGE_SELECTOR_OFFSET: usize = 0x02_a590;
pub const SMW_US_V1_OVERWORLD_MESSAGE_POINTER_OFFSET: usize = 0x02_a5a7;
pub const SMW_US_V1_OVERWORLD_MESSAGE_TEXT_OFFSET: usize = 0x02_a5d9;
pub const SMW_US_V1_OVERWORLD_MESSAGE_TEXT_LEN: usize =
    0x03_0000 - SMW_US_V1_OVERWORLD_MESSAGE_TEXT_OFFSET;

pub const SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED: [u8; 7] =
    [0xf6, 0x15, 0x48, 0x22, 0xd2, 0xf7, 0x07];
const RUNTIME: [u8; 0x110] = [
    0xac, 0x26, 0x14, 0xc0, 0x03, 0xf0, 0x24, 0xae, 0xbf, 0x13, 0xe0, 0x28, 0xf0, 0x23, 0x88, 0xd0,
    0x28, 0xe0, 0x14, 0xf0, 0x0f, 0xc8, 0xe0, 0x45, 0xf0, 0x0a, 0xc8, 0xe0, 0x3f, 0xf0, 0x05, 0xc8,
    0xe0, 0x08, 0xd0, 0x03, 0x20, 0x44, 0xbc, 0xa0, 0x00, 0x80, 0x0e, 0xa2, 0x00, 0xa0, 0x01, 0x80,
    0x08, 0xa0, 0x00, 0xad, 0x7a, 0x18, 0xf0, 0x01, 0xc8, 0xc2, 0x30, 0x8a, 0x0a, 0x85, 0x00, 0x98,
    0x65, 0x00, 0x85, 0x00, 0x0a, 0x65, 0x00, 0xaa, 0xbf, 0x00, 0x00, 0x00, 0x85, 0x04, 0xbf, 0x01,
    0x00, 0x00, 0x85, 0x05, 0x8b, 0xf4, 0x7f, 0x7f, 0xab, 0xab, 0xac, 0x7b, 0x83, 0xa2, 0x0e, 0x00,
    0x64, 0x02, 0x8b, 0x4b, 0xab, 0xbd, 0x79, 0xbc, 0xab, 0x99, 0x7d, 0x83, 0xa9, 0x00, 0x23, 0x99,
    0x7f, 0x83, 0xda, 0xa2, 0x12, 0x00, 0xa9, 0x1f, 0x39, 0x24, 0x02, 0x30, 0x10, 0xe2, 0x20, 0xa7,
    0x04, 0xc9, 0xfe, 0xd0, 0x04, 0x85, 0x03, 0xa9, 0x1f, 0xc2, 0x20, 0xe6, 0x04, 0x99, 0x81, 0x83,
    0xc8, 0xc8, 0xca, 0xd0, 0xe4, 0xc8, 0xc8, 0xc8, 0xc8, 0xfa, 0xca, 0xca, 0x10, 0xc4, 0xa9, 0xff,
    0x00, 0x99, 0x7d, 0x83, 0x8c, 0x7b, 0x83, 0xab, 0x64, 0x22, 0x64, 0x24, 0xe2, 0x30, 0xa9, 0x01,
    0x8d, 0xd5, 0x13, 0x6b, 0xda, 0x98, 0x1a, 0x8d, 0xd2, 0x13, 0x3a, 0x0a, 0x0a, 0x0a, 0x0a, 0xaa,
    0x64, 0x00, 0xc2, 0x20, 0xa0, 0x1c, 0xbd, 0x9b, 0xb2, 0x99, 0x02, 0x02, 0xda, 0xa6, 0x00, 0xbd,
    0xdb, 0xb2, 0x99, 0x00, 0x02, 0xfa, 0xe8, 0xe8, 0xe6, 0x00, 0xe6, 0x00, 0x88, 0x88, 0x88, 0x88,
    0x10, 0xe4, 0x9c, 0x00, 0x04, 0xe2, 0x20, 0xfa, 0x60, 0x51, 0xa7, 0x51, 0x87, 0x51, 0x67, 0x51,
    0x47, 0x51, 0x27, 0x51, 0x07, 0x50, 0xe7, 0x50, 0xc7, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x4c, 0x4d, 0x10, 0x01,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldMessagePatchBuildError {
    InvalidMessageCount(usize),
    MessageContainsTerminator { index: usize },
}

impl std::fmt::Display for OverworldMessagePatchBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded overworld-message patch build failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldMessagePatchBuildError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1OverworldMessageStorage {
    Pristine,
    Expanded(ExpandedOverworldMessageStorage),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1OverworldMessages {
    pub messages: Vec<OverworldMessage>,
    pub storage: SmwUsV1OverworldMessageStorage,
}

#[derive(Debug)]
pub enum SmwUsV1OverworldMessageLoadError {
    Rom(RomError),
    Expanded(OverworldMessagePatchError),
    Vanilla(VanillaOverworldMessageError),
    Hook([u8; SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED.len()]),
}

impl std::fmt::Display for SmwUsV1OverworldMessageLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "SMW US overworld-message load failed: {self:?}")
    }
}

impl std::error::Error for SmwUsV1OverworldMessageLoadError {}

impl From<RomError> for SmwUsV1OverworldMessageLoadError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<OverworldMessagePatchError> for SmwUsV1OverworldMessageLoadError {
    fn from(value: OverworldMessagePatchError) -> Self {
        Self::Expanded(value)
    }
}

impl From<VanillaOverworldMessageError> for SmwUsV1OverworldMessageLoadError {
    fn from(value: VanillaOverworldMessageError) -> Self {
        Self::Vanilla(value)
    }
}

#[must_use]
pub const fn smw_us_v1_overworld_message_patch_locator() -> OverworldMessagePatchLocator {
    OverworldMessagePatchLocator {
        mapper: Mapper::LoRom,
        hook_offset: SMW_US_V1_OVERWORLD_MESSAGE_HOOK_OFFSET,
        runtime_offset: SMW_US_V1_OVERWORLD_MESSAGE_RUNTIME_OFFSET,
    }
}

#[must_use]
pub const fn smw_us_v1_overworld_message_runtime() -> &'static [u8; 0x110] {
    &RUNTIME
}

/// Loads either the exact pristine 97×2 logical message table or a recognized expanded runtime.
///
/// # Errors
///
/// Rejects foreign hooks, malformed vanilla fixed tables, and altered expanded ownership/runtime
/// state.
pub fn load_smw_us_v1_overworld_messages(
    project: &Project,
) -> Result<LoadedSmwUsV1OverworldMessages, SmwUsV1OverworldMessageLoadError> {
    let hook = project.rom.read(
        SMW_US_V1_OVERWORLD_MESSAGE_HOOK_OFFSET,
        SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED.len(),
    )?;
    if hook == SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED {
        let mapping = project
            .rom
            .read(SMW_US_V1_OVERWORLD_MESSAGE_SELECTOR_OFFSET, 23)?;
        let pointers = project
            .rom
            .read(SMW_US_V1_OVERWORLD_MESSAGE_POINTER_OFFSET, 50)?;
        let text = project.rom.read(
            SMW_US_V1_OVERWORLD_MESSAGE_TEXT_OFFSET,
            SMW_US_V1_OVERWORLD_MESSAGE_TEXT_LEN,
        )?;
        return Ok(LoadedSmwUsV1OverworldMessages {
            messages: decode_vanilla_overworld_messages(mapping, pointers, text)?,
            storage: SmwUsV1OverworldMessageStorage::Pristine,
        });
    }
    if hook.first() == Some(&0x22) {
        let loaded = project.load_expanded_overworld_messages_detected(
            smw_us_v1_overworld_message_patch_locator(),
        )?;
        return Ok(LoadedSmwUsV1OverworldMessages {
            messages: loaded.messages,
            storage: SmwUsV1OverworldMessageStorage::Expanded(loaded.storage),
        });
    }
    let mut actual = [0; SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED.len()];
    actual.copy_from_slice(hook);
    Err(SmwUsV1OverworldMessageLoadError::Hook(actual))
}

#[must_use]
pub fn smw_us_v1_overworld_message_allocation_policy() -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_OVERWORLD_MESSAGE_SEARCH_START..SMW_US_V1_OVERWORLD_MESSAGE_SEARCH_END,
    )
}

/// Builds Lunar Magic's expanded pointer table and its independently allocated 192-message pools.
///
/// # Errors
///
/// Requires an even 194–512 message count and rejects an embedded `$FE`, which is the native
/// string terminator and cannot be represented as a visible tile.
pub fn smw_us_v1_overworld_message_installation_plan(
    messages: &[OverworldMessage],
) -> Result<RelocatablePatchPlan, OverworldMessagePatchBuildError> {
    if !(194..=512).contains(&messages.len()) || messages.len() % 2 != 0 {
        return Err(OverworldMessagePatchBuildError::InvalidMessageCount(
            messages.len(),
        ));
    }
    let mut table = PatchPayload {
        bytes: vec![0; messages.len() * 3],
        fixups: Vec::with_capacity(messages.len()),
    };
    let mut payloads = vec![table.clone()];
    for (group_index, group) in messages.chunks(0xc0).enumerate() {
        let mut bytes = Vec::new();
        let mut offsets = Vec::with_capacity(group.len());
        let mut empty_offset = None;
        for (within_group, message) in group.iter().enumerate() {
            if message.0.contains(&0xfe) {
                return Err(OverworldMessagePatchBuildError::MessageContainsTerminator {
                    index: group_index * 0xc0 + within_group,
                });
            }
            let used = message
                .0
                .iter()
                .rposition(|byte| *byte != 0x1f)
                .map_or(0, |index| index + 1);
            if used == 0 {
                let offset = *empty_offset.get_or_insert_with(|| {
                    let offset = bytes.len();
                    bytes.push(0xfe);
                    offset
                });
                offsets.push(offset);
            } else {
                let offset = bytes.len();
                bytes.extend_from_slice(&message.0[..used]);
                if used < OverworldMessage::ENCODED_LEN {
                    bytes.push(0xfe);
                }
                offsets.push(offset);
            }
        }
        let payload_index = payloads.len();
        payloads.push(PatchPayload {
            bytes,
            fixups: Vec::new(),
        });
        for (within_group, offset) in offsets.into_iter().enumerate() {
            table.fixups.push(PatchFixup {
                offset: (group_index * 0xc0 + within_group) * 3,
                target_payload: payload_index,
                target_addend: offset,
                encoding: PatchFixupEncoding::Long24,
            });
        }
    }
    payloads[0] = table;
    Ok(RelocatablePatchPlan {
        description: "install expanded native overworld messages".into(),
        mapper: Mapper::LoRom,
        allocation: smw_us_v1_overworld_message_allocation_policy(),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads,
        writes: vec![
            PatchWrite {
                offset: SMW_US_V1_OVERWORLD_MESSAGE_RUNTIME_OFFSET,
                expected: vec![0xff; RUNTIME.len()],
                replacement: RUNTIME.to_vec(),
                fixups: vec![
                    PatchFixup {
                        offset: 0x49,
                        target_payload: 0,
                        target_addend: 0,
                        encoding: PatchFixupEncoding::Long24,
                    },
                    PatchFixup {
                        offset: 0x4f,
                        target_payload: 0,
                        target_addend: 1,
                        encoding: PatchFixupEncoding::Long24,
                    },
                ],
            },
            PatchWrite {
                offset: SMW_US_V1_OVERWORLD_MESSAGE_HOOK_OFFSET,
                expected: SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED.to_vec(),
                replacement: vec![0x22, 0x90, 0xbd, 0x03, 0x4c, 0x50, 0xb2],
                fixups: Vec::new(),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn embedded_resource_and_installation_reopen_exactly() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
        assert!(
            executable
                .windows(RUNTIME.len())
                .any(|window| window == RUNTIME)
        );

        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let messages: Vec<_> = (0_usize..200)
            .map(|index| {
                let mut bytes = [0x1f; OverworldMessage::ENCODED_LEN];
                bytes[0] = index.to_le_bytes()[0];
                bytes[1] = 0x40;
                OverworldMessage(bytes)
            })
            .collect();
        project
            .install_relocatable_patch(
                &smw_us_v1_overworld_message_installation_plan(&messages).unwrap(),
            )
            .unwrap();
        let loaded = project
            .load_expanded_overworld_messages_detected(smw_us_v1_overworld_message_patch_locator())
            .unwrap();
        assert_eq!(loaded.messages, messages);
        assert_eq!(loaded.storage.pointer_table_len, 600);
        assert_eq!(loaded.storage.message_pools.len(), 2);
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn pristine_messages_materialize_all_194_slots_and_survive_installation() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let loaded = load_smw_us_v1_overworld_messages(&project).unwrap();
        assert_eq!(loaded.messages.len(), 194);
        assert!(matches!(
            loaded.storage,
            SmwUsV1OverworldMessageStorage::Pristine
        ));
        assert!(
            loaded
                .messages
                .iter()
                .any(|message| { message.0.iter().any(|glyph| *glyph != 0x1f) })
        );
        project
            .install_relocatable_patch(
                &smw_us_v1_overworld_message_installation_plan(&loaded.messages).unwrap(),
            )
            .unwrap();
        let reopened = load_smw_us_v1_overworld_messages(&project).unwrap();
        assert_eq!(reopened.messages, loaded.messages);
        assert!(matches!(
            reopened.storage,
            SmwUsV1OverworldMessageStorage::Expanded(_)
        ));
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn installed_table_grows_repoints_reopens_and_undoes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut project = Project::open_supported(RomImage::from_bytes(original).unwrap()).unwrap();
        let initial = vec![OverworldMessage([0x1f; 144]); 200];
        project
            .install_relocatable_patch(
                &smw_us_v1_overworld_message_installation_plan(&initial).unwrap(),
            )
            .unwrap();
        let before_update = project.save_snapshot();
        let loaded = project
            .load_expanded_overworld_messages_detected(smw_us_v1_overworld_message_patch_locator())
            .unwrap();
        let mut grown = vec![OverworldMessage([0x1f; 144]); 400];
        for (index, message) in grown.iter_mut().enumerate() {
            message.0[0] = u8::try_from(index % 0xfd).unwrap();
        }
        assert!(
            project
                .save_installed_overworld_messages(
                    &grown,
                    &loaded.storage,
                    smw_us_v1_overworld_message_patch_locator(),
                    &smw_us_v1_overworld_message_allocation_policy(),
                    SMW_US_V1_CHECKSUM_FIELD,
                    0xff,
                )
                .unwrap()
        );
        assert_eq!(
            project
                .load_expanded_overworld_messages_detected(
                    smw_us_v1_overworld_message_patch_locator()
                )
                .unwrap()
                .messages,
            grown
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), before_update);
    }
}
