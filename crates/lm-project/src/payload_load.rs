mod policy;
mod readers;

pub use policy::PayloadReadPolicy;
use policy::validate_read_policy;

use crate::Project;
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError, SnesPointer24};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedPayload {
    pub pointer: SnesPointer24,
    pub pc_offset: usize,
    pub block: Option<RatsBlock>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum PayloadLoadError {
    Rom(RomError),
    PointerNotTagged {
        payload_offset: usize,
    },
    EmptyTerminator,
    InvalidBankSize(usize),
    MissingTerminator {
        payload_offset: usize,
        searched: usize,
    },
    TaggedLengthMismatch {
        payload_offset: usize,
        actual: usize,
        expected: usize,
    },
}

impl fmt::Display for PayloadLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "payload load failed: {self:?}")
    }
}

impl std::error::Error for PayloadLoadError {}

impl From<RomError> for PayloadLoadError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl Project {
    /// Resolves a three-byte SNES pointer and reads a bounded payload using `policy`.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid pointer, invalid tag, out-of-range fixed payload, or a
    /// terminated payload whose terminator is not found before its configured limit.
    pub fn load_payload(
        &self,
        pointer_offset: usize,
        mapper: Mapper,
        policy: &PayloadReadPolicy,
    ) -> Result<LoadedPayload, PayloadLoadError> {
        validate_read_policy(policy)?;
        let encoded = self.rom.read(pointer_offset, 3)?;
        let pointer = SnesPointer24::decode(encoded).map_err(|_| {
            PayloadLoadError::Rom(RomError::RangeOutOfBounds {
                offset: pointer_offset,
                len: 3,
                image_len: self.rom.logical_len(),
            })
        })?;
        self.load_payload_from_pointer(pointer, mapper, policy)
    }

    /// Resolves an already-decoded SNES pointer and reads a bounded payload using `policy`.
    ///
    /// This is used by ROM layouts whose pointer bytes live in split low-word and bank tables.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid policy or mapping, an invalid RATS tag, an out-of-range
    /// payload, or a missing bounded terminator.
    pub fn load_payload_from_pointer(
        &self,
        pointer: SnesPointer24,
        mapper: Mapper,
        policy: &PayloadReadPolicy,
    ) -> Result<LoadedPayload, PayloadLoadError> {
        validate_read_policy(policy)?;
        let payload_offset = pointer.to_pc(mapper)?;
        match policy {
            PayloadReadPolicy::Tagged => self.load_tagged_at(pointer, payload_offset),
            PayloadReadPolicy::Fixed { len } => {
                let bytes = self.rom.read(payload_offset, *len)?.to_vec();
                Ok(LoadedPayload {
                    pointer,
                    pc_offset: payload_offset,
                    block: None,
                    bytes,
                })
            }
            PayloadReadPolicy::Terminated {
                terminator,
                maximum_len,
                bank_size,
            } => self.load_terminated_at(
                pointer,
                payload_offset,
                terminator,
                *maximum_len,
                *bank_size,
            ),
            PayloadReadPolicy::TaggedOrTerminated {
                terminator,
                maximum_len,
                bank_size,
            } => self
                .load_tagged_at(pointer, payload_offset)
                .or_else(|error| {
                    if matches!(error, PayloadLoadError::PointerNotTagged { .. }) {
                        self.load_terminated_at(
                            pointer,
                            payload_offset,
                            terminator,
                            *maximum_len,
                            *bank_size,
                        )
                    } else {
                        Err(error)
                    }
                }),
            PayloadReadPolicy::TaggedOrFixed { len } => self
                .load_tagged_at(pointer, payload_offset)
                .and_then(|payload| {
                    if payload.bytes.len() == *len {
                        Ok(payload)
                    } else {
                        Err(PayloadLoadError::TaggedLengthMismatch {
                            payload_offset,
                            actual: payload.bytes.len(),
                            expected: *len,
                        })
                    }
                })
                .or_else(|error| {
                    if matches!(error, PayloadLoadError::PointerNotTagged { .. }) {
                        let bytes = self.rom.read(payload_offset, *len)?.to_vec();
                        Ok(LoadedPayload {
                            pointer,
                            pc_offset: payload_offset,
                            block: None,
                            bytes,
                        })
                    } else {
                        Err(error)
                    }
                }),
            PayloadReadPolicy::Bounded {
                maximum_len,
                bank_size,
            } => self.load_bounded_at(pointer, payload_offset, *maximum_len, *bank_size),
            PayloadReadPolicy::TaggedOrBounded {
                maximum_len,
                bank_size,
            } => self
                .load_tagged_at(pointer, payload_offset)
                .or_else(|error| {
                    if matches!(error, PayloadLoadError::PointerNotTagged { .. }) {
                        self.load_bounded_at(pointer, payload_offset, *maximum_len, *bank_size)
                    } else {
                        Err(error)
                    }
                }),
        }
    }

    /// Loads a payload that immediately follows a valid RATS tag.
    ///
    /// # Errors
    ///
    /// Returns an error when the pointer is invalid or does not target a RATS payload.
    pub fn load_tagged_payload(
        &self,
        pointer_offset: usize,
        mapper: Mapper,
    ) -> Result<LoadedPayload, PayloadLoadError> {
        self.load_payload(pointer_offset, mapper, &PayloadReadPolicy::Tagged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::RomImage;

    #[test]
    fn tagged_or_terminated_loads_clean_rom_data() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        bytes[0x100..0x104].copy_from_slice(&[1, 2, 3, 0xff]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let loaded = project
            .load_payload(
                0x20,
                Mapper::LoRom,
                &PayloadReadPolicy::TaggedOrTerminated {
                    terminator: vec![0xff],
                    maximum_len: 0x8000,
                    bank_size: Some(0x8000),
                },
            )
            .unwrap();
        assert_eq!(loaded.pc_offset, 0x100);
        assert_eq!(loaded.bytes, [1, 2, 3, 0xff]);
        assert!(loaded.block.is_none());
    }

    #[test]
    fn terminated_load_does_not_cross_a_bank() {
        let mut bytes = vec![0; 0x10000];
        bytes[0x20..0x23].copy_from_slice(&[0xfe, 0xff, 0x80]);
        bytes[0x8000] = 0xff;
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert!(matches!(
            project.load_payload(
                0x20,
                Mapper::LoRom,
                &PayloadReadPolicy::Terminated {
                    terminator: vec![0xff],
                    maximum_len: 0x8000,
                    bank_size: Some(0x8000),
                }
            ),
            Err(PayloadLoadError::MissingTerminator { searched: 2, .. })
        ));
    }

    #[test]
    fn tagged_policy_rejects_an_untagged_pointer() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert!(matches!(
            project.load_tagged_payload(0x20, Mapper::LoRom),
            Err(PayloadLoadError::PointerNotTagged { .. })
        ));
    }

    #[test]
    fn tagged_or_fixed_requires_exact_tagged_payload_length() {
        let mut bytes = vec![0xff; 0x8000];
        let block = lm_rats::FreeSpaceAllocator::new(
            &mut bytes,
            lm_rats::AllocationPolicy {
                search: 0x100..0x8000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![lm_rats::ProtectedRange(0x20..0x23)],
            },
        )
        .allocate(&[1, 2, 3])
        .unwrap();
        let pointer = lm_rom::pc_to_snes(Mapper::LoRom, block.payload.start)
            .unwrap()
            .to_le_bytes();
        bytes[0x20..0x23].copy_from_slice(&pointer[..3]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert!(matches!(
            project.load_payload(
                0x20,
                Mapper::LoRom,
                &PayloadReadPolicy::TaggedOrFixed { len: 4 },
            ),
            Err(PayloadLoadError::TaggedLengthMismatch {
                actual: 3,
                expected: 4,
                ..
            })
        ));
        assert_eq!(
            project
                .load_payload(
                    0x20,
                    Mapper::LoRom,
                    &PayloadReadPolicy::TaggedOrFixed { len: 3 },
                )
                .unwrap()
                .bytes,
            [1, 2, 3]
        );
    }

    #[test]
    fn zero_bank_size_is_a_typed_error_for_every_bounded_policy() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        bytes[0x100] = 0xff;
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        for policy in [
            PayloadReadPolicy::Terminated {
                terminator: vec![0xff],
                maximum_len: 0x8000,
                bank_size: Some(0),
            },
            PayloadReadPolicy::TaggedOrTerminated {
                terminator: vec![0xff],
                maximum_len: 0x8000,
                bank_size: Some(0),
            },
            PayloadReadPolicy::Bounded {
                maximum_len: 0x8000,
                bank_size: Some(0),
            },
            PayloadReadPolicy::TaggedOrBounded {
                maximum_len: 0x8000,
                bank_size: Some(0),
            },
        ] {
            assert_eq!(
                project.load_payload(0x20, Mapper::LoRom, &policy),
                Err(PayloadLoadError::InvalidBankSize(0))
            );
        }
    }

    #[test]
    fn tagged_success_cannot_hide_an_invalid_fallback_policy() {
        let mut bytes = vec![0xff; 0x8000];
        let block = lm_rats::FreeSpaceAllocator::new(
            &mut bytes,
            lm_rats::AllocationPolicy {
                search: 0x100..0x8000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![lm_rats::ProtectedRange(0x20..0x23)],
            },
        )
        .allocate(&[1, 2, 3])
        .unwrap();
        let pointer = lm_rom::pc_to_snes(Mapper::LoRom, block.payload.start)
            .unwrap()
            .to_le_bytes();
        bytes[0x20..0x23].copy_from_slice(&pointer[..3]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());

        assert_eq!(
            project.load_payload(
                0x20,
                Mapper::LoRom,
                &PayloadReadPolicy::TaggedOrTerminated {
                    terminator: Vec::new(),
                    maximum_len: 0x8000,
                    bank_size: Some(0x8000),
                }
            ),
            Err(PayloadLoadError::EmptyTerminator)
        );
        assert_eq!(
            project.load_payload(
                0x20,
                Mapper::LoRom,
                &PayloadReadPolicy::TaggedOrBounded {
                    maximum_len: 0x8000,
                    bank_size: Some(0),
                }
            ),
            Err(PayloadLoadError::InvalidBankSize(0))
        );
    }
}
