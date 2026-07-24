use super::{RevisionPatchTemplate, RevisionPatchTemplateError};
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite};
use lm_rom::{Mapper, Region, SupportedGame};

pub(super) fn decode(bytes: &[u8]) -> Result<RevisionPatchTemplate, RevisionPatchTemplateError> {
    if bytes.len() > RevisionPatchTemplate::MAX_FILE_LEN {
        return Err(RevisionPatchTemplateError::TooLarge {
            actual: bytes.len(),
            maximum: RevisionPatchTemplate::MAX_FILE_LEN,
        });
    }
    let mut input = Input { bytes, offset: 0 };
    if input.take(8)? != RevisionPatchTemplate::MAGIC {
        return Err(RevisionPatchTemplateError::WrongMagic);
    }
    let game = decode_game(input.byte()?)?;
    let region = decode_region(input.byte()?)?;
    let revision = input.byte()?;
    let mapper = decode_mapper(input.byte()?)?;
    let name_len = usize::from(input.u16()?);
    let payload_count = usize::from(input.u16()?);
    let write_count = usize::from(input.u16()?);
    let name = String::from_utf8(input.take(name_len)?.to_vec())
        .map_err(|_| RevisionPatchTemplateError::InvalidName)?;
    let payloads = (0..payload_count)
        .map(|index| decode_payload(&mut input, index))
        .collect::<Result<Vec<_>, _>>()?;
    let writes = (0..write_count)
        .map(|index| decode_write(&mut input, index))
        .collect::<Result<Vec<_>, _>>()?;
    if input.offset != bytes.len() {
        return Err(RevisionPatchTemplateError::TrailingBytes(
            bytes.len() - input.offset,
        ));
    }
    let result = RevisionPatchTemplate {
        name,
        game,
        region,
        revision,
        mapper,
        payloads,
        writes,
    };
    validate(&result)?;
    if encode(&result)? != bytes {
        return Err(RevisionPatchTemplateError::NonCanonical);
    }
    Ok(result)
}

pub(super) fn encode(
    template: &RevisionPatchTemplate,
) -> Result<Vec<u8>, RevisionPatchTemplateError> {
    validate(template)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(RevisionPatchTemplate::MAGIC);
    bytes.extend_from_slice(&[
        encode_game(template.game),
        encode_region(template.region),
        template.revision,
        encode_mapper(template.mapper),
    ]);
    push_u16(&mut bytes, template.name.len())?;
    push_u16(&mut bytes, template.payloads.len())?;
    push_u16(&mut bytes, template.writes.len())?;
    bytes.extend_from_slice(template.name.as_bytes());
    for payload in &template.payloads {
        push_u32(&mut bytes, payload.bytes.len())?;
        push_u16(&mut bytes, payload.fixups.len())?;
        bytes.extend_from_slice(&payload.bytes);
        encode_fixups(&mut bytes, &payload.fixups)?;
    }
    for write in &template.writes {
        push_u32(&mut bytes, write.offset)?;
        push_u32(&mut bytes, write.replacement.len())?;
        push_u16(&mut bytes, write.fixups.len())?;
        bytes.extend_from_slice(&write.expected);
        bytes.extend_from_slice(&write.replacement);
        encode_fixups(&mut bytes, &write.fixups)?;
    }
    if bytes.len() > RevisionPatchTemplate::MAX_FILE_LEN {
        return Err(RevisionPatchTemplateError::TooLarge {
            actual: bytes.len(),
            maximum: RevisionPatchTemplate::MAX_FILE_LEN,
        });
    }
    Ok(bytes)
}

fn decode_payload(
    input: &mut Input<'_>,
    index: usize,
) -> Result<PatchPayload, RevisionPatchTemplateError> {
    let len =
        usize::try_from(input.u32()?).map_err(|_| RevisionPatchTemplateError::NumberOutOfRange)?;
    let fixup_count = usize::from(input.u16()?);
    let bytes = input.take(len)?.to_vec();
    let fixups = decode_fixups(input, fixup_count)?;
    if bytes.is_empty() {
        return Err(RevisionPatchTemplateError::EmptyPayload(index));
    }
    Ok(PatchPayload { bytes, fixups })
}

fn decode_write(
    input: &mut Input<'_>,
    index: usize,
) -> Result<PatchWrite, RevisionPatchTemplateError> {
    let offset =
        usize::try_from(input.u32()?).map_err(|_| RevisionPatchTemplateError::NumberOutOfRange)?;
    let len =
        usize::try_from(input.u32()?).map_err(|_| RevisionPatchTemplateError::NumberOutOfRange)?;
    let fixup_count = usize::from(input.u16()?);
    let expected = input.take(len)?.to_vec();
    let replacement = input.take(len)?.to_vec();
    let fixups = decode_fixups(input, fixup_count)?;
    if replacement.is_empty() {
        return Err(RevisionPatchTemplateError::EmptyWrite(index));
    }
    Ok(PatchWrite {
        offset,
        expected,
        replacement,
        fixups,
    })
}

fn decode_fixups(
    input: &mut Input<'_>,
    count: usize,
) -> Result<Vec<PatchFixup>, RevisionPatchTemplateError> {
    if count > RevisionPatchTemplate::MAX_FIXUPS {
        return Err(RevisionPatchTemplateError::TooManyFixups(count));
    }
    (0..count)
        .map(|_| {
            Ok(PatchFixup {
                offset: usize::try_from(input.u32()?)
                    .map_err(|_| RevisionPatchTemplateError::NumberOutOfRange)?,
                target_payload: usize::from(input.u16()?),
                target_addend: usize::try_from(input.u32()?)
                    .map_err(|_| RevisionPatchTemplateError::NumberOutOfRange)?,
                encoding: PatchFixupEncoding::Long24,
            })
        })
        .collect()
}

fn encode_fixups(
    bytes: &mut Vec<u8>,
    fixups: &[PatchFixup],
) -> Result<(), RevisionPatchTemplateError> {
    for fixup in fixups {
        push_u32(bytes, fixup.offset)?;
        push_u16(bytes, fixup.target_payload)?;
        push_u32(bytes, fixup.target_addend)?;
    }
    Ok(())
}

fn validate(template: &RevisionPatchTemplate) -> Result<(), RevisionPatchTemplateError> {
    if template.name.is_empty()
        || template.name.len() > RevisionPatchTemplate::MAX_NAME_LEN
        || template.name.trim() != template.name
        || template.name.contains(['\0', '\n', '\r'])
    {
        return Err(RevisionPatchTemplateError::InvalidName);
    }
    if template.payloads.len() > RevisionPatchTemplate::MAX_PAYLOADS {
        return Err(RevisionPatchTemplateError::TooManyPayloads(
            template.payloads.len(),
        ));
    }
    if template.writes.len() > RevisionPatchTemplate::MAX_WRITES {
        return Err(RevisionPatchTemplateError::TooManyWrites(
            template.writes.len(),
        ));
    }
    let payload_bytes = template
        .payloads
        .iter()
        .map(|payload| payload.bytes.len())
        .try_fold(0_usize, usize::checked_add)
        .ok_or(RevisionPatchTemplateError::NumberOutOfRange)?;
    let write_bytes = template
        .writes
        .iter()
        .try_fold(0_usize, |total, write| {
            total
                .checked_add(write.expected.len())
                .and_then(|value| value.checked_add(write.replacement.len()))
        })
        .ok_or(RevisionPatchTemplateError::NumberOutOfRange)?;
    let body = payload_bytes
        .checked_add(write_bytes)
        .ok_or(RevisionPatchTemplateError::NumberOutOfRange)?;
    if body > RevisionPatchTemplate::MAX_BODY_BYTES {
        return Err(RevisionPatchTemplateError::BodyTooLarge(body));
    }
    for (index, payload) in template.payloads.iter().enumerate() {
        if payload.bytes.is_empty() {
            return Err(RevisionPatchTemplateError::EmptyPayload(index));
        }
        validate_fixup_count(&payload.fixups)?;
        validate_fixups(
            &payload.fixups,
            payload.bytes.len(),
            index,
            &template.payloads,
        )?;
    }
    for (index, write) in template.writes.iter().enumerate() {
        if write.replacement.is_empty() {
            return Err(RevisionPatchTemplateError::EmptyWrite(index));
        }
        if write.expected.len() != write.replacement.len() {
            return Err(RevisionPatchTemplateError::WriteLengthMismatch(index));
        }
        validate_fixup_count(&write.fixups)?;
        validate_fixups(
            &write.fixups,
            write.replacement.len(),
            template.payloads.len() + index,
            &template.payloads,
        )?;
    }
    Ok(())
}

fn validate_fixups(
    fixups: &[PatchFixup],
    owner_len: usize,
    owner: usize,
    payloads: &[PatchPayload],
) -> Result<(), RevisionPatchTemplateError> {
    let mut previous_end = 0;
    let mut sorted = fixups.iter().enumerate().collect::<Vec<_>>();
    sorted.sort_by_key(|(_, fixup)| fixup.offset);
    for (index, fixup) in sorted {
        // LMPAT001 predates split operands and deliberately remains byte-compatible.
        if fixup.encoding != PatchFixupEncoding::Long24 {
            return Err(RevisionPatchTemplateError::InvalidFixup { owner, index });
        }
        let Some(end) = fixup.offset.checked_add(fixup.encoding.encoded_len()) else {
            return Err(RevisionPatchTemplateError::InvalidFixup { owner, index });
        };
        let valid_target = payloads
            .get(fixup.target_payload)
            .is_some_and(|payload| fixup.target_addend < payload.bytes.len());
        if end > owner_len || fixup.offset < previous_end || !valid_target {
            return Err(RevisionPatchTemplateError::InvalidFixup { owner, index });
        }
        previous_end = end;
    }
    Ok(())
}

fn validate_fixup_count(fixups: &[PatchFixup]) -> Result<(), RevisionPatchTemplateError> {
    if fixups.len() > RevisionPatchTemplate::MAX_FIXUPS {
        Err(RevisionPatchTemplateError::TooManyFixups(fixups.len()))
    } else {
        Ok(())
    }
}

fn push_u16(bytes: &mut Vec<u8>, value: usize) -> Result<(), RevisionPatchTemplateError> {
    bytes.extend_from_slice(
        &u16::try_from(value)
            .map_err(|_| RevisionPatchTemplateError::NumberOutOfRange)?
            .to_le_bytes(),
    );
    Ok(())
}

fn push_u32(bytes: &mut Vec<u8>, value: usize) -> Result<(), RevisionPatchTemplateError> {
    bytes.extend_from_slice(
        &u32::try_from(value)
            .map_err(|_| RevisionPatchTemplateError::NumberOutOfRange)?
            .to_le_bytes(),
    );
    Ok(())
}

const fn encode_game(value: SupportedGame) -> u8 {
    match value {
        SupportedGame::SuperMarioWorld => 0,
        SupportedGame::AllStarsAndWorld => 1,
    }
}

fn decode_game(value: u8) -> Result<SupportedGame, RevisionPatchTemplateError> {
    match value {
        0 => Ok(SupportedGame::SuperMarioWorld),
        1 => Ok(SupportedGame::AllStarsAndWorld),
        _ => Err(RevisionPatchTemplateError::UnknownGame(value)),
    }
}

const fn encode_region(value: Region) -> u8 {
    match value {
        Region::Japan => 0,
        Region::NorthAmerica => 1,
    }
}

fn decode_region(value: u8) -> Result<Region, RevisionPatchTemplateError> {
    match value {
        0 => Ok(Region::Japan),
        1 => Ok(Region::NorthAmerica),
        _ => Err(RevisionPatchTemplateError::UnknownRegion(value)),
    }
}

const fn encode_mapper(value: Mapper) -> u8 {
    match value {
        Mapper::LoRom => 0,
        Mapper::ExLoRom => 1,
        Mapper::Sa1 => 2,
    }
}

fn decode_mapper(value: u8) -> Result<Mapper, RevisionPatchTemplateError> {
    match value {
        0 => Ok(Mapper::LoRom),
        1 => Ok(Mapper::ExLoRom),
        2 => Ok(Mapper::Sa1),
        _ => Err(RevisionPatchTemplateError::UnknownMapper(value)),
    }
}

struct Input<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Input<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], RevisionPatchTemplateError> {
        let end = self
            .offset
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(RevisionPatchTemplateError::Truncated)?;
        let result = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(result)
    }

    fn byte(&mut self) -> Result<u8, RevisionPatchTemplateError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, RevisionPatchTemplateError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| RevisionPatchTemplateError::Truncated)?,
        ))
    }

    fn u32(&mut self) -> Result<u32, RevisionPatchTemplateError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| RevisionPatchTemplateError::Truncated)?,
        ))
    }
}
