//! Revision-specific layout contract for Lunar Magic's main Layer 3 runtime.
//!
//! This module records addresses and entry points, not Lunar Magic's embedded payload.  The
//! corresponding routines in [`crate::layer3_runtime`] are independently generated from their
//! recovered behavior.

/// Size of the single RATS allocation used by the main Layer 3 runtime.
pub const SMW_US_V1_LAYER3_MAIN_PAYLOAD_LEN: usize = 0x4c0;

/// One externally visible entry in the main Layer 3 allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layer3MainEntry {
    /// Logical PC offset of the hook in a headerless SMW US v1 ROM.
    pub hook_offset: usize,
    /// Offset of the entry routine within the allocated payload.
    pub payload_offset: usize,
    /// Opcode installed at the hook (`JSL` or `JML`).
    pub hook_opcode: u8,
    /// Number of bytes replaced at the hook, including any continuation byte.
    pub replaced_len: usize,
}

/// Entry reached from the level-mode initialization hook.
pub const SMW_US_V1_LAYER3_MODE_DISPATCH_ENTRY: Layer3MainEntry = Layer3MainEntry {
    hook_offset: 0x0000_201f,
    payload_offset: 0,
    hook_opcode: 0x22,
    replaced_len: 6,
};

/// Entry reached from the `$12` status initialization hook.
pub const SMW_US_V1_LAYER3_STATUS_ENTRY: Layer3MainEntry = Layer3MainEntry {
    hook_offset: 0x0000_2153,
    payload_offset: 0x480,
    hook_opcode: 0x22,
    replaced_len: 4,
};

/// Entry reached from the `$1693/$1694` initialization hook.
pub const SMW_US_V1_LAYER3_MODE_VALUE_ENTRY: Layer3MainEntry = Layer3MainEntry {
    hook_offset: 0x0000_94b6,
    payload_offset: 0x4a0,
    hook_opcode: 0x5c,
    replaced_len: 5,
};

/// Entry reached from the level Layer 3 dispatcher.
pub const SMW_US_V1_LAYER3_LEVEL_DISPATCH_ENTRY: Layer3MainEntry = Layer3MainEntry {
    hook_offset: 0x0002_c40c,
    payload_offset: 0x417,
    hook_opcode: 0x22,
    replaced_len: 5,
};

/// Every direct ROM entry into the allocation, in hook-address order.
pub const SMW_US_V1_LAYER3_MAIN_ENTRIES: [Layer3MainEntry; 4] = [
    SMW_US_V1_LAYER3_MODE_DISPATCH_ENTRY,
    SMW_US_V1_LAYER3_STATUS_ENTRY,
    SMW_US_V1_LAYER3_MODE_VALUE_ENTRY,
    SMW_US_V1_LAYER3_LEVEL_DISPATCH_ENTRY,
];

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{Mapper, snes_to_pc};
    use std::{fs, path::PathBuf};

    #[test]
    fn retained_wine_installation_uses_every_recovered_entry() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rom = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let header_len = 0x200;

        let first = SMW_US_V1_LAYER3_MAIN_ENTRIES[0];
        let hook = first.hook_offset + header_len;
        let target = u32::from_le_bytes([rom[hook + 1], rom[hook + 2], rom[hook + 3], 0]);
        let payload_pc = snes_to_pc(Mapper::LoRom, target).unwrap();

        assert_eq!(payload_pc, 0x0008_1a0d);
        assert_eq!(
            &rom[payload_pc + header_len - 8..payload_pc + header_len - 4],
            b"STAR"
        );
        assert_eq!(
            u16::from_le_bytes([
                rom[payload_pc + header_len - 4],
                rom[payload_pc + header_len - 3]
            ]),
            u16::try_from(SMW_US_V1_LAYER3_MAIN_PAYLOAD_LEN - 1).unwrap()
        );

        for entry in SMW_US_V1_LAYER3_MAIN_ENTRIES {
            let hook = entry.hook_offset + header_len;
            assert_eq!(rom[hook], entry.hook_opcode);
            let target = u32::from_le_bytes([rom[hook + 1], rom[hook + 2], rom[hook + 3], 0]);
            assert_eq!(
                snes_to_pc(Mapper::LoRom, target).unwrap(),
                payload_pc + entry.payload_offset
            );
            assert!(entry.payload_offset < SMW_US_V1_LAYER3_MAIN_PAYLOAD_LEN);
        }
    }
}
