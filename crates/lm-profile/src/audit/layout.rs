use super::RevisionProfileAuditError;
use crate::RevisionProfile;
use lm_project::{
    ExpandedLevelSettingsLayout, GraphicsRomLayout, LevelPointerTable, SpritePointerTable,
};
use std::ops::Range;

pub(super) fn table_span(
    domain: &'static str,
    table: LevelPointerTable,
) -> Result<Range<usize>, RevisionProfileAuditError> {
    let end = table
        .entries
        .checked_sub(1)
        .and_then(|last| last.checked_mul(table.stride))
        .and_then(|last| last.checked_add(3))
        .and_then(|len| table.offset.checked_add(len))
        .ok_or(RevisionProfileAuditError::PointerOffset { domain, index: 0 })?;
    Ok(table.offset..end)
}

pub(super) fn expanded_settings_span(
    layout: ExpandedLevelSettingsLayout,
) -> Result<Range<usize>, RevisionProfileAuditError> {
    let end = layout
        .entries
        .checked_sub(1)
        .and_then(|last| last.checked_mul(layout.stride))
        .and_then(|last| last.checked_add(lm_level::ExpandedLevelSettingsRecord::ENCODED_LEN))
        .and_then(|len| layout.table_offset.checked_add(len))
        .ok_or(RevisionProfileAuditError::PointerOffset {
            domain: "expanded_settings",
            index: 0,
        })?;
    Ok(layout.table_offset..end)
}

pub(super) fn overlaps(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

pub(super) fn sprite_spans(
    layout: SpritePointerTable,
) -> Result<Vec<(&'static str, Range<usize>)>, RevisionProfileAuditError> {
    let component = |domain,
                     table: LevelPointerTable,
                     width|
     -> Result<(&'static str, Range<usize>), RevisionProfileAuditError> {
        let end = table
            .entries
            .checked_sub(1)
            .and_then(|last| last.checked_mul(table.stride))
            .and_then(|last| last.checked_add(width))
            .and_then(|len| table.offset.checked_add(len))
            .ok_or(RevisionProfileAuditError::PointerOffset { domain, index: 0 })?;
        Ok((domain, table.offset..end))
    };
    match layout {
        SpritePointerTable::Contiguous(table) => Ok(vec![component("level.sprites", table, 3)?]),
        SpritePointerTable::SplitSharedBank {
            low_words,
            bank_offset,
        } => Ok(vec![
            component("level.sprites", low_words, 2)?,
            (
                "level.sprites.bank",
                bank_offset
                    ..bank_offset.checked_add(1).ok_or(
                        RevisionProfileAuditError::PointerOffset {
                            domain: "level.sprites.bank",
                            index: 0,
                        },
                    )?,
            ),
        ]),
        SpritePointerTable::SplitBankTable { low_words, banks } => Ok(vec![
            component("level.sprites", low_words, 2)?,
            component("level.sprites.banks", banks, 1)?,
        ]),
    }
}

pub(super) fn graphics_spans(
    layout: GraphicsRomLayout,
) -> Result<Vec<(&'static str, Range<usize>)>, RevisionProfileAuditError> {
    let Some(planes) = layout.split_pointer_planes else {
        return Ok(vec![("graphics", table_span("graphics", layout.pointers)?)]);
    };
    let component = |domain: &'static str,
                     offset: usize|
     -> Result<(&'static str, Range<usize>), RevisionProfileAuditError> {
        let end = planes
            .entries
            .checked_sub(1)
            .and_then(|last| last.checked_mul(planes.stride))
            .and_then(|last| last.checked_add(1))
            .and_then(|len| offset.checked_add(len))
            .ok_or(RevisionProfileAuditError::PointerOffset { domain, index: 0 })?;
        Ok((domain, offset..end))
    };
    Ok(vec![
        component("graphics.low", planes.low_offset)?,
        component("graphics.high", planes.high_offset)?,
        component("graphics.bank", planes.bank_offset)?,
    ])
}

pub(super) fn tables(profile: &RevisionProfile) -> Vec<(&'static str, LevelPointerTable)> {
    let mut tables = vec![
        ("level.layer1", profile.level.layer1),
        (
            "level.sprites",
            profile.level.sprites.low_or_contiguous_table(),
        ),
        ("map16.graphics", profile.map16.graphics),
        ("map16.acts_like", profile.map16.acts_like),
        ("graphics", profile.graphics.pointers),
        ("palette", profile.palette.pointers),
        ("exanimation", profile.exanimation.pointers),
        ("overworld.layer1", profile.overworld.layers.layer1),
        ("overworld.layer2", profile.overworld.layers.layer2),
        (
            "overworld.event_sources",
            profile.overworld.event_reveals.sources,
        ),
        (
            "overworld.event_destinations",
            profile.overworld.event_reveals.destinations,
        ),
        ("overworld.endpoints", profile.overworld.endpoints.pointers),
        ("overworld.messages", profile.overworld.messages.pointers),
        ("overworld.sprites", profile.overworld.sprites.pointers),
        ("overworld.palette", profile.overworld.palette.pointers),
        ("overworld.animation", profile.overworld.animation.pointers),
    ];
    if let Some(layer2) = profile.layer2 {
        tables.push(("level.layer2", layer2.pointers));
    }
    tables
}
