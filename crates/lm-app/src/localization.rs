//! Typed, toolkit-independent user-interface localization.

use flate2::{Decompress, FlushDecompress, Status};
use std::collections::BTreeMap;

#[cfg(test)]
use crate::original_language_dialog::OriginalLanguageDialogControl;
use crate::original_language_dialog::{
    OriginalLanguageDialogTemplate, OriginalLanguageDialogTemplateError,
    decode_original_language_dialog_template,
};
use crate::original_language_dialog_map::ORIGINAL_LANGUAGE_DIALOG_RESOURCE_IDS;
use crate::original_language_validation::{
    RANGE_STRING_LENGTH_CEILINGS, SINGLE_STRING_LENGTH_CEILINGS,
};

const MAGIC: &[u8; 8] = b"LMLOC001";
const DIALOG_TEXT_MAGIC: &[u8; 8] = b"LMDLG001";
const MAX_LOCALE_BYTES: usize = 64;
const MAX_TEXT_BYTES: usize = 4096;
const MAX_DIALOG_TEXT_ENTRIES: usize = 4096;
const LEGACY_CHROME_KEY_COUNT: usize = 19;
const PREVIOUS_COMPLETE_KEY_COUNT: usize = 238;
const EARLIER_COMPLETE_KEY_COUNTS: [usize; 6] = [183, 184, 199, 201, 212, 230];
const MAX_ENCODED_BYTES: usize = MAGIC.len()
    + 2
    + MAX_LOCALE_BYTES
    + 2
    + UiTextKey::ALL.len() * (1 + 2 + MAX_TEXT_BYTES)
    + DIALOG_TEXT_MAGIC.len()
    + 2
    + MAX_DIALOG_TEXT_ENTRIES * (2 + 2 + 4 + 2 + MAX_TEXT_BYTES);
const ORIGINAL_LANGUAGE_MARKER: u32 = 0xc001_babe;
const ORIGINAL_LANGUAGE_METADATA_MAX_BYTES: usize = 0x410;
const ORIGINAL_LANGUAGE_TRAILER_BYTES: usize = 0x40;
const ORIGINAL_LANGUAGE_CHECKSUM_FROM_END: usize = 0x38;
const ORIGINAL_LANGUAGE_MAX_STRINGS: usize = 0x16ee;
const ORIGINAL_LANGUAGE_MAX_INFLATED_BYTES: usize = 32 * 1024 * 1024;

/// The four text fields published by an original Lunar Magic language DLL's resource `$DB6`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginalLanguageModuleMetadata {
    pub display_name: String,
    pub version: String,
    pub locale: String,
    pub code_page: String,
}

/// Validated UTF-8 strings decoded from an original language DLL's `$DAC/$DAD/$DAE` resources.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginalLanguageStringPool {
    strings: Vec<Option<String>>,
}

/// One localized Win32 dialog resource paired with its built-in Lunar Magic dialog ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginalLanguageDialogResource<'a> {
    pub original_id: u16,
    pub localized_id: u16,
    bytes: &'a [u8],
}

impl<'a> OriginalLanguageDialogResource<'a> {
    #[must_use]
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Decodes this resource's standard or extended Win32 template framing.
    ///
    /// # Errors
    ///
    /// Returns [`OriginalLanguageDialogTemplateError`] when the resource is malformed.
    pub fn decode(
        &self,
    ) -> Result<OriginalLanguageDialogTemplate, OriginalLanguageDialogTemplateError> {
        decode_original_language_dialog_template(self.bytes)
    }
}

impl OriginalLanguageStringPool {
    #[must_use]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<&str> {
        self.strings.get(index)?.as_deref()
    }

    /// Converts every evidence-backed original UI string into the complete typed Rust catalog.
    /// Rust-only workflows deliberately retain their built-in English text.
    ///
    /// # Errors
    ///
    /// Returns [`LocalizationError`] when the module locale or a converted value violates the
    /// bounded typed-catalog contract.
    pub fn to_catalog(
        &self,
        locale: impl Into<String>,
    ) -> Result<LocalizationCatalog, LocalizationError> {
        LocalizationCatalog::new(
            locale,
            UiTextKey::ALL.into_iter().map(|key| {
                let text = original_string_index(key)
                    .and_then(|index| self.get(index))
                    .map(|text| normalize_original_ui_text(key, text))
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| key.english().to_owned());
                (key, text)
            }),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OriginalLanguageModuleError {
    ModuleTooShort(usize),
    InvalidPortableExecutable(&'static str),
    MissingResource(u16),
    ResourceBounds,
    MalformedStringTables,
    Inflate(String),
    InflatedPoolTooLong(usize),
    InvalidStringUtf8(usize),
    InvalidCatalog(LocalizationError),
    WrongMarker,
    MetadataTooLong(usize),
    InvalidUtf8,
    MissingMetadataFields,
    ChecksumMismatch { stored: u32, computed: u32 },
}

impl std::fmt::Display for OriginalLanguageModuleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "original language module error: {self:?}")
    }
}

impl std::error::Error for OriginalLanguageModuleError {}

impl OriginalLanguageModuleMetadata {
    /// Decodes the original resource-type `$01F4`, IDs `$0DB7` and `$0DB6` metadata contract.
    ///
    /// # Errors
    ///
    /// Returns [`OriginalLanguageModuleError`] when the marker is absent, the metadata exceeds
    /// Lunar Magic's recovered bound, the text is not UTF-8, or fewer than four fields exist.
    pub fn decode(marker: &[u8], metadata: &[u8]) -> Result<Self, OriginalLanguageModuleError> {
        if marker.get(..4) != Some(&ORIGINAL_LANGUAGE_MARKER.to_le_bytes()) {
            return Err(OriginalLanguageModuleError::WrongMarker);
        }
        if metadata.len() > ORIGINAL_LANGUAGE_METADATA_MAX_BYTES {
            return Err(OriginalLanguageModuleError::MetadataTooLong(metadata.len()));
        }
        let metadata = metadata.strip_prefix(b"\xef\xbb\xbf").unwrap_or(metadata);
        let text =
            std::str::from_utf8(metadata).map_err(|_| OriginalLanguageModuleError::InvalidUtf8)?;
        let mut fields = text.split('\n').map(|field| {
            field
                .strip_suffix('\r')
                .unwrap_or(field)
                .split('\0')
                .next()
                .unwrap_or_default()
                .to_owned()
        });
        Ok(Self {
            display_name: fields
                .next()
                .ok_or(OriginalLanguageModuleError::MissingMetadataFields)?,
            version: fields
                .next()
                .ok_or(OriginalLanguageModuleError::MissingMetadataFields)?,
            locale: fields
                .next()
                .ok_or(OriginalLanguageModuleError::MissingMetadataFields)?,
            code_page: fields
                .next()
                .ok_or(OriginalLanguageModuleError::MissingMetadataFields)?,
        })
    }
}

/// Validates the encoded checksum used by original Lunar Magic language DLLs.
///
/// Only bytes before the final 64-byte trailer participate. The stored little-endian checksum is
/// 56 bytes from the end, exactly matching `ValidateLanguageModuleChecksum` at `$004D7010`.
///
/// # Errors
///
/// Returns [`OriginalLanguageModuleError`] for a short file or checksum mismatch.
pub fn validate_original_language_module_checksum(
    bytes: &[u8],
) -> Result<(), OriginalLanguageModuleError> {
    if bytes.len() < ORIGINAL_LANGUAGE_TRAILER_BYTES {
        return Err(OriginalLanguageModuleError::ModuleTooShort(bytes.len()));
    }
    let payload_end = bytes.len() - ORIGINAL_LANGUAGE_TRAILER_BYTES;
    let checksum_offset = bytes.len() - ORIGINAL_LANGUAGE_CHECKSUM_FROM_END;
    let stored = u32::from_le_bytes(
        bytes[checksum_offset..checksum_offset + 4]
            .try_into()
            .expect("the validated trailer contains four checksum bytes"),
    );
    let computed =
        bytes[..payload_end]
            .iter()
            .copied()
            .enumerate()
            .fold(0_u32, |sum, (offset, byte)| {
                let transformed = if offset & 2 == 0 {
                    if offset & 1 == 0 {
                        u32::from(byte.rotate_left(2) ^ 0x46)
                    } else {
                        0_u32.wrapping_sub(u32::from(byte.rotate_left(4) ^ 0x77))
                    }
                } else {
                    u32::from(
                        byte.wrapping_mul(0x80)
                            .wrapping_add(byte >> 1)
                            .wrapping_sub(0x17)
                            ^ 0x71,
                    )
                };
                sum.wrapping_add(transformed)
            });
    if stored != computed {
        return Err(OriginalLanguageModuleError::ChecksumMismatch { stored, computed });
    }
    Ok(())
}

/// Validates and decodes an original Lunar Magic language DLL without loading or executing it.
///
/// The PE reader accepts both PE32 and PE32+ images, follows only integer resource-directory IDs,
/// and extracts resource type `$01F4`, IDs `$0DB7` and `$0DB6` through bounded section mappings.
///
/// # Errors
///
/// Returns [`OriginalLanguageModuleError`] for a checksum failure, malformed PE headers or
/// resource directories, missing resources, or invalid language metadata.
pub fn decode_original_language_module(
    bytes: &[u8],
) -> Result<OriginalLanguageModuleMetadata, OriginalLanguageModuleError> {
    validate_original_language_module_checksum(bytes)?;
    let resources = PeResources::parse(bytes)?;
    let marker = resources.resource(0x0db7)?;
    let metadata = resources.resource(0x0db6)?;
    OriginalLanguageModuleMetadata::decode(marker, metadata)
}

/// Validates an original language DLL and decodes its complete bounded localized string pool.
///
/// Resource `$DAC` contains an offset-dependent obfuscated raw-DEFLATE stream. Resource `$DAD`
/// starts with the declared string count followed by offsets, while `$DAE` contains matching
/// lengths. The effective count is the recovered minimum of all three table bounds and 5,869.
///
/// # Errors
///
/// Returns [`OriginalLanguageModuleError`] for module/PE/resource validation failures, malformed
/// tables, incomplete DEFLATE input, excessive output, or invalid UTF-8 in an otherwise valid
/// string entry.
pub fn decode_original_language_module_strings(
    bytes: &[u8],
) -> Result<OriginalLanguageStringPool, OriginalLanguageModuleError> {
    validate_original_language_module_checksum(bytes)?;
    let resources = PeResources::parse(bytes)?;
    decode_original_language_string_resources(
        resources.resource(0x0dac)?,
        resources.resource(0x0dad)?,
        resources.resource(0x0dae)?,
    )
}

/// Validates an original DLL and returns every mapped type-5 dialog resource it actually contains.
/// Missing mapped dialogs are omitted, matching Lunar Magic's per-dialog built-in fallback.
///
/// # Errors
///
/// Returns [`OriginalLanguageModuleError`] for checksum, PE, directory, or resource-bound failures.
pub fn decode_original_language_module_dialogs(
    bytes: &[u8],
) -> Result<Vec<OriginalLanguageDialogResource<'_>>, OriginalLanguageModuleError> {
    validate_original_language_module_checksum(bytes)?;
    let resources = PeResources::parse(bytes)?;
    OriginalLanguageModuleMetadata::decode(
        resources.resource(0x0db7)?,
        resources.resource(0x0db6)?,
    )?;
    let mut dialogs = Vec::new();
    for &(original_id, localized_id) in ORIGINAL_LANGUAGE_DIALOG_RESOURCE_IDS {
        match resources.resource_of_type(5, localized_id) {
            Ok(bytes) => dialogs.push(OriginalLanguageDialogResource {
                original_id,
                localized_id,
                bytes,
            }),
            Err(OriginalLanguageModuleError::MissingResource(_)) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(dialogs)
}

/// Validates one original DLL and converts its supported strings into a complete typed catalog.
///
/// Original strings without an evidence-backed semantic equivalent are intentionally ignored;
/// Rust-only keys retain their built-in English values.
///
/// # Errors
///
/// Returns [`OriginalLanguageModuleError`] for module/resource failures or an invalid converted
/// catalog.
pub fn decode_original_language_module_catalog(
    bytes: &[u8],
) -> Result<(OriginalLanguageModuleMetadata, LocalizationCatalog), OriginalLanguageModuleError> {
    validate_original_language_module_checksum(bytes)?;
    let resources = PeResources::parse(bytes)?;
    let metadata = OriginalLanguageModuleMetadata::decode(
        resources.resource(0x0db7)?,
        resources.resource(0x0db6)?,
    )?;
    let strings = decode_original_language_string_resources(
        resources.resource(0x0dac)?,
        resources.resource(0x0dad)?,
        resources.resource(0x0dae)?,
    )?;
    let mut catalog = strings
        .to_catalog(metadata.locale.clone())
        .map_err(OriginalLanguageModuleError::InvalidCatalog)?;
    let dialogs: Vec<_> = ORIGINAL_LANGUAGE_DIALOG_RESOURCE_IDS
        .iter()
        .filter_map(|&(original_id, localized_id)| {
            let bytes = resources.resource_of_type(5, localized_id).ok()?;
            let template = decode_original_language_dialog_template(bytes).ok()?;
            Some((original_id, template))
        })
        .collect();
    for (dialog_id, template) in &dialogs {
        catalog.insert_original_dialog_template(*dialog_id, template);
    }
    apply_original_dialog_catalog_overrides(&mut catalog, dialogs);
    catalog
        .validate()
        .map_err(OriginalLanguageModuleError::InvalidCatalog)?;
    Ok((metadata, catalog))
}

fn apply_original_dialog_catalog_overrides(
    catalog: &mut LocalizationCatalog,
    dialogs: impl IntoIterator<Item = (u16, OriginalLanguageDialogTemplate)>,
) {
    for (dialog_id, template) in dialogs {
        let mappings: &[(u32, UiTextKey)] = match dialog_id {
            // LanguageSelectionDialogProc: the original localizes this chooser through its own
            // mapped template, making it the stable source for application-wide common actions.
            0x042b => &[(1, UiTextKey::CommonOk), (2, UiTextKey::CommonCancel)],
            // AboutDialogProc exposes these three semantic buttons directly.
            0x03f8 => &[
                (1, UiTextKey::AboutOk),
                (0x66, UiTextKey::AboutThirdPartyEnhancements),
                (0x67, UiTextKey::AboutLegalNotice),
            ],
            _ => continue,
        };
        for &(control_id, key) in mappings {
            let Some(text) = template
                .controls
                .iter()
                .find(|control| control.id == control_id)
                .and_then(|control| control.text.as_deref())
            else {
                continue;
            };
            let text = normalize_original_ui_text(key, text);
            if !text.is_empty() {
                catalog.entries.insert(key, text);
            }
        }
    }
}

fn original_string_index(key: UiTextKey) -> Option<usize> {
    Some(match key {
        UiTextKey::MenuFile => 0x000a,
        UiTextKey::MenuEdit => 0x000b,
        UiTextKey::MenuView => 0x000c,
        UiTextKey::MenuEditors => 0x000d,
        UiTextKey::MenuHelp => 0x0010,
        UiTextKey::FileOpen => 0x0011,
        UiTextKey::FileSave => 0x0014,
        UiTextKey::FileSaveAs => 0x0015,
        UiTextKey::FileExpandRom => 0x001f,
        UiTextKey::ToolsTestRomInEmulator => 0x001e,
        UiTextKey::FileOpenRecent => 0x0023,
        UiTextKey::FileQuit => 0x0024,
        UiTextKey::FileAnalyzeLevelUsage => 0x0032,
        UiTextKey::FileScanRom => 0x0033,
        UiTextKey::ToolsTestRomInEmulatorAction => 0x0036,
        UiTextKey::ToolsChooseEmulator => 0x0037,
        UiTextKey::FileCreateFullRestore => 0x004e,
        UiTextKey::FileRestoreRom => 0x004f,
        UiTextKey::FileCreateIpsPatch => 0x0050,
        UiTextKey::FileApplyIpsPatch => 0x0051,
        UiTextKey::EditUndo => 0x0055,
        UiTextKey::EditRedo => 0x0056,
        UiTextKey::EditCut => 0x0057,
        UiTextKey::EditCopy => 0x0058,
        UiTextKey::EditPaste => 0x0059,
        UiTextKey::ViewLayer1 => 0x006c,
        UiTextKey::ViewLayer2 => 0x006d,
        UiTextKey::ViewLayer3 => 0x006e,
        UiTextKey::ViewLayerSprites => 0x006f,
        UiTextKey::ViewSpecialWorldPassed => 0x0081,
        UiTextKey::HelpTopics => 0x0118,
        UiTextKey::HelpAbout => 0x0119,
        _ => return None,
    })
}

fn normalize_original_ui_text(key: UiTextKey, text: &str) -> String {
    let mut normalized = normalize_original_dialog_text(text);
    if key == UiTextKey::HelpAbout {
        normalized = normalized.replace("%s", "Lunar Magic Rust");
    }
    normalized
}

fn normalize_original_dialog_text(text: &str) -> String {
    let text = text.split('\t').next().unwrap_or(text).trim();
    let mut normalized = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '&' {
            normalized.push(character);
            continue;
        }
        if characters.peek() == Some(&'&') {
            characters.next();
            normalized.push('&');
        }
    }
    if normalized.ends_with("...") {
        normalized.truncate(normalized.len() - 3);
        normalized.push('…');
    }
    normalized
}

fn decode_original_language_string_resources(
    encoded_pool: &[u8],
    offsets: &[u8],
    lengths: &[u8],
) -> Result<OriginalLanguageStringPool, OriginalLanguageModuleError> {
    if offsets.len() < 4 {
        return Err(OriginalLanguageModuleError::MalformedStringTables);
    }
    let declared = usize::try_from(read_u32(offsets, 0)?)
        .map_err(|_| OriginalLanguageModuleError::MalformedStringTables)?;
    let count = declared
        .min((offsets.len() - 4) / 4)
        .min(lengths.len() / 4)
        .min(ORIGINAL_LANGUAGE_MAX_STRINGS);

    let mut compressed = encoded_pool.to_vec();
    for index in 1..compressed.len() {
        compressed[index] = (compressed[index] ^ 0x92)
            .wrapping_sub(compressed[index - 1])
            .wrapping_add(0x34);
    }
    let inflated = inflate_original_language_pool(&compressed)?;

    let mut strings = Vec::with_capacity(count);
    for index in 0..count {
        let table_offset = index
            .checked_mul(4)
            .ok_or(OriginalLanguageModuleError::MalformedStringTables)?;
        let offset = usize::try_from(read_u32(offsets, 4 + table_offset)?)
            .map_err(|_| OriginalLanguageModuleError::MalformedStringTables)?;
        let length = usize::try_from(read_u32(lengths, table_offset)?)
            .map_err(|_| OriginalLanguageModuleError::MalformedStringTables)?;
        let Some(end) = offset.checked_add(length) else {
            strings.push(None);
            continue;
        };
        if end >= inflated.len() || inflated[end] != 0 {
            strings.push(None);
            continue;
        }
        if !original_language_string_length_is_allowed(index, length) {
            strings.push(None);
            continue;
        }
        let string = std::str::from_utf8(&inflated[offset..end])
            .map_err(|_| OriginalLanguageModuleError::InvalidStringUtf8(index))?;
        strings.push(Some(string.to_owned()));
    }
    Ok(OriginalLanguageStringPool { strings })
}

fn original_language_string_length_is_allowed(index: usize, length: usize) -> bool {
    if SINGLE_STRING_LENGTH_CEILINGS
        .iter()
        .any(|&(guarded_index, ceiling)| guarded_index == index && length >= ceiling)
    {
        return false;
    }
    !RANGE_STRING_LENGTH_CEILINGS
        .iter()
        .any(|(range, ceiling)| range.contains(&index) && length >= *ceiling)
}

fn inflate_original_language_pool(
    compressed: &[u8],
) -> Result<Vec<u8>, OriginalLanguageModuleError> {
    inflate_original_language_pool_with_limit(compressed, ORIGINAL_LANGUAGE_MAX_INFLATED_BYTES)
}

fn inflate_original_language_pool_with_limit(
    compressed: &[u8],
    limit: usize,
) -> Result<Vec<u8>, OriginalLanguageModuleError> {
    let mut decoder = Decompress::new(false);
    let mut inflated = Vec::new();
    let mut input_offset = 0;
    loop {
        let mut output = [0_u8; 8_192];
        let before_input = decoder.total_in();
        let before_output = decoder.total_out();
        let status = decoder
            .decompress(
                &compressed[input_offset..],
                &mut output,
                FlushDecompress::Finish,
            )
            .map_err(|error| OriginalLanguageModuleError::Inflate(error.to_string()))?;
        let consumed = usize::try_from(decoder.total_in() - before_input)
            .map_err(|_| OriginalLanguageModuleError::Inflate("input count overflow".into()))?;
        let produced = usize::try_from(decoder.total_out() - before_output)
            .map_err(|_| OriginalLanguageModuleError::Inflate("output count overflow".into()))?;
        input_offset = input_offset
            .checked_add(consumed)
            .ok_or_else(|| OriginalLanguageModuleError::Inflate("input offset overflow".into()))?;
        let next_len = inflated
            .len()
            .checked_add(produced)
            .ok_or(OriginalLanguageModuleError::InflatedPoolTooLong(usize::MAX))?;
        if next_len > limit {
            return Err(OriginalLanguageModuleError::InflatedPoolTooLong(next_len));
        }
        inflated.extend_from_slice(&output[..produced]);
        if status == Status::StreamEnd {
            return Ok(inflated);
        }
        if consumed == 0 && produced == 0 {
            return Err(OriginalLanguageModuleError::Inflate(
                "incomplete DEFLATE stream".into(),
            ));
        }
    }
}

const MAX_PE_SECTIONS: usize = 96;
const MAX_RESOURCE_DIRECTORY_ENTRIES: usize = 4_096;
const PE_RESOURCE_TYPE: u16 = 0x01f4;

struct PeResources<'a> {
    bytes: &'a [u8],
    root: usize,
    size: usize,
    sections: usize,
    section_count: usize,
    size_of_headers: usize,
}

impl<'a> PeResources<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, OriginalLanguageModuleError> {
        if bytes.get(..2) != Some(b"MZ") {
            return Err(OriginalLanguageModuleError::InvalidPortableExecutable(
                "missing DOS signature",
            ));
        }
        let pe = usize::try_from(read_u32(bytes, 0x3c)?).map_err(|_| {
            OriginalLanguageModuleError::InvalidPortableExecutable("PE offset overflow")
        })?;
        if bytes.get(
            pe..pe
                .checked_add(4)
                .ok_or(OriginalLanguageModuleError::InvalidPortableExecutable(
                    "PE offset overflow",
                ))?,
        ) != Some(b"PE\0\0")
        {
            return Err(OriginalLanguageModuleError::InvalidPortableExecutable(
                "missing PE signature",
            ));
        }
        let section_count = usize::from(read_u16(bytes, pe + 6)?);
        if section_count == 0 || section_count > MAX_PE_SECTIONS {
            return Err(OriginalLanguageModuleError::InvalidPortableExecutable(
                "invalid section count",
            ));
        }
        let optional_size = usize::from(read_u16(bytes, pe + 20)?);
        let optional =
            pe.checked_add(24)
                .ok_or(OriginalLanguageModuleError::InvalidPortableExecutable(
                    "optional-header overflow",
                ))?;
        let optional_end = optional.checked_add(optional_size).ok_or(
            OriginalLanguageModuleError::InvalidPortableExecutable("optional-header overflow"),
        )?;
        if optional_end > bytes.len() {
            return Err(OriginalLanguageModuleError::InvalidPortableExecutable(
                "truncated optional header",
            ));
        }
        let directory_base = match read_u16(bytes, optional)? {
            0x010b => optional + 96,
            0x020b => optional + 112,
            _ => {
                return Err(OriginalLanguageModuleError::InvalidPortableExecutable(
                    "unknown optional-header magic",
                ));
            }
        };
        if read_u32(bytes, directory_base - 4)? < 3 {
            return Err(OriginalLanguageModuleError::InvalidPortableExecutable(
                "resource data directory is not declared",
            ));
        }
        let resource_directory = directory_base.checked_add(16).ok_or(
            OriginalLanguageModuleError::InvalidPortableExecutable("data-directory overflow"),
        )?;
        if resource_directory
            .checked_add(8)
            .is_none_or(|end| end > optional_end)
        {
            return Err(OriginalLanguageModuleError::InvalidPortableExecutable(
                "missing resource data directory",
            ));
        }
        let resource_rva = read_u32(bytes, resource_directory)?;
        let size = usize::try_from(read_u32(bytes, resource_directory + 4)?)
            .map_err(|_| OriginalLanguageModuleError::ResourceBounds)?;
        if resource_rva == 0 || size < 16 {
            return Err(OriginalLanguageModuleError::InvalidPortableExecutable(
                "empty resource data directory",
            ));
        }
        let sections = optional_end;
        let section_bytes = section_count
            .checked_mul(40)
            .and_then(|length| sections.checked_add(length))
            .ok_or(OriginalLanguageModuleError::ResourceBounds)?;
        if section_bytes > bytes.len() {
            return Err(OriginalLanguageModuleError::InvalidPortableExecutable(
                "truncated section table",
            ));
        }
        let size_of_headers = usize::try_from(read_u32(bytes, optional + 60)?)
            .map_err(|_| OriginalLanguageModuleError::ResourceBounds)?;
        let mut image = Self {
            bytes,
            root: 0,
            size,
            sections,
            section_count,
            size_of_headers,
        };
        image.root = image.map_rva(resource_rva, 16)?;
        image.relative_range(0, 16)?;
        Ok(image)
    }

    fn resource(&self, id: u16) -> Result<&'a [u8], OriginalLanguageModuleError> {
        self.resource_of_type(PE_RESOURCE_TYPE, id)
    }

    fn resource_of_type(
        &self,
        resource_type: u16,
        id: u16,
    ) -> Result<&'a [u8], OriginalLanguageModuleError> {
        let type_directory = self.find_id(0, resource_type, true)?;
        let language_directory = self.find_id(type_directory, id, true)?;
        let data_entry = self.first_data_entry(language_directory)?;
        let entry = self.relative_range(data_entry, 16)?;
        let data_rva = read_u32(entry, 0)?;
        let size = usize::try_from(read_u32(entry, 4)?)
            .map_err(|_| OriginalLanguageModuleError::ResourceBounds)?;
        let offset = self.map_rva(data_rva, size)?;
        self.bytes
            .get(offset..offset + size)
            .ok_or(OriginalLanguageModuleError::ResourceBounds)
    }

    fn find_id(
        &self,
        directory: usize,
        id: u16,
        require_directory: bool,
    ) -> Result<usize, OriginalLanguageModuleError> {
        let header = self.relative_range(directory, 16)?;
        let named = usize::from(read_u16(header, 12)?);
        let ids = usize::from(read_u16(header, 14)?);
        let count = named
            .checked_add(ids)
            .ok_or(OriginalLanguageModuleError::ResourceBounds)?;
        if count > MAX_RESOURCE_DIRECTORY_ENTRIES {
            return Err(OriginalLanguageModuleError::ResourceBounds);
        }
        let entries = directory
            .checked_add(16)
            .ok_or(OriginalLanguageModuleError::ResourceBounds)?;
        let table = self.relative_range(
            entries,
            count
                .checked_mul(8)
                .ok_or(OriginalLanguageModuleError::ResourceBounds)?,
        )?;
        for entry in table.chunks_exact(8).skip(named) {
            let name = read_u32(entry, 0)?;
            if name & 0x8000_0000 == 0 && name == u32::from(id) {
                let target = read_u32(entry, 4)?;
                if (target & 0x8000_0000 != 0) != require_directory {
                    return Err(OriginalLanguageModuleError::ResourceBounds);
                }
                return usize::try_from(target & 0x7fff_ffff)
                    .map_err(|_| OriginalLanguageModuleError::ResourceBounds);
            }
        }
        Err(OriginalLanguageModuleError::MissingResource(id))
    }

    fn first_data_entry(&self, directory: usize) -> Result<usize, OriginalLanguageModuleError> {
        let header = self.relative_range(directory, 16)?;
        let count = usize::from(read_u16(header, 12)?)
            .checked_add(usize::from(read_u16(header, 14)?))
            .ok_or(OriginalLanguageModuleError::ResourceBounds)?;
        if count == 0 || count > MAX_RESOURCE_DIRECTORY_ENTRIES {
            return Err(OriginalLanguageModuleError::ResourceBounds);
        }
        let entry = self.relative_range(
            directory
                .checked_add(16)
                .ok_or(OriginalLanguageModuleError::ResourceBounds)?,
            8,
        )?;
        let target = read_u32(entry, 4)?;
        if target & 0x8000_0000 != 0 {
            return Err(OriginalLanguageModuleError::ResourceBounds);
        }
        usize::try_from(target).map_err(|_| OriginalLanguageModuleError::ResourceBounds)
    }

    fn relative_range(
        &self,
        offset: usize,
        length: usize,
    ) -> Result<&'a [u8], OriginalLanguageModuleError> {
        let relative_end = offset
            .checked_add(length)
            .ok_or(OriginalLanguageModuleError::ResourceBounds)?;
        if relative_end > self.size {
            return Err(OriginalLanguageModuleError::ResourceBounds);
        }
        let start = self
            .root
            .checked_add(offset)
            .ok_or(OriginalLanguageModuleError::ResourceBounds)?;
        self.bytes
            .get(start..start + length)
            .ok_or(OriginalLanguageModuleError::ResourceBounds)
    }

    fn map_rva(&self, rva: u32, length: usize) -> Result<usize, OriginalLanguageModuleError> {
        let rva = usize::try_from(rva).map_err(|_| OriginalLanguageModuleError::ResourceBounds)?;
        if rva < self.size_of_headers {
            return (rva
                .checked_add(length)
                .is_some_and(|end| end <= self.bytes.len()))
            .then_some(rva)
            .ok_or(OriginalLanguageModuleError::ResourceBounds);
        }
        for index in 0..self.section_count {
            let section = self.sections + index * 40;
            let virtual_size = usize::try_from(read_u32(self.bytes, section + 8)?)
                .map_err(|_| OriginalLanguageModuleError::ResourceBounds)?;
            let virtual_address = usize::try_from(read_u32(self.bytes, section + 12)?)
                .map_err(|_| OriginalLanguageModuleError::ResourceBounds)?;
            let raw_size = usize::try_from(read_u32(self.bytes, section + 16)?)
                .map_err(|_| OriginalLanguageModuleError::ResourceBounds)?;
            let raw = usize::try_from(read_u32(self.bytes, section + 20)?)
                .map_err(|_| OriginalLanguageModuleError::ResourceBounds)?;
            let span = virtual_size.max(raw_size);
            let Some(delta) = rva.checked_sub(virtual_address) else {
                continue;
            };
            if delta >= span || delta.checked_add(length).is_none_or(|end| end > raw_size) {
                continue;
            }
            let offset = raw
                .checked_add(delta)
                .ok_or(OriginalLanguageModuleError::ResourceBounds)?;
            if offset
                .checked_add(length)
                .is_some_and(|end| end <= self.bytes.len())
            {
                return Ok(offset);
            }
        }
        Err(OriginalLanguageModuleError::ResourceBounds)
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, OriginalLanguageModuleError> {
    let end = offset
        .checked_add(2)
        .ok_or(OriginalLanguageModuleError::ResourceBounds)?;
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..end)
            .ok_or(OriginalLanguageModuleError::ResourceBounds)?
            .try_into()
            .expect("bounded two-byte slice"),
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, OriginalLanguageModuleError> {
    let end = offset
        .checked_add(4)
        .ok_or(OriginalLanguageModuleError::ResourceBounds)?;
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..end)
            .ok_or(OriginalLanguageModuleError::ResourceBounds)?
            .try_into()
            .expect("bounded four-byte slice"),
    ))
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum UiTextKey {
    AppTitle,
    FileOpen,
    FileSave,
    FileSaveAs,
    FileClose,
    FileQuit,
    EditUndo,
    EditRedo,
    EditCopy,
    EditCut,
    EditPaste,
    ViewLevel,
    ViewOverworld,
    ViewMap16,
    ViewGraphics,
    ViewPalette,
    ViewExAnimation,
    StatusReady,
    ViewLayer3,
    MenuFile,
    MenuEdit,
    MenuView,
    MenuEditors,
    MenuProfile,
    MenuTools,
    MenuDocuments,
    MenuHelp,
    FileOpenRecent,
    FileExpandRom,
    FileConvertCopierHeader,
    FileAnalyzeLevelUsage,
    FileRestrictLevelAccess,
    FileRestoreRom,
    FileCreateFullRestore,
    FileAppendDeltaRestore,
    FileAppendFullRestore,
    FileAppendAutomaticRestore,
    FileMigrateGraphicsCompression,
    FileInstallBuiltInRuntime,
    FileReclaimOwnedRatsBlocks,
    FileApplyIpsPatch,
    FileCreateIpsPatch,
    ViewSpecialWorldPassed,
    ViewLayer1,
    ViewLayer2,
    ViewLayerSprites,
    ProfileInstallRevision,
    ProfileClear,
    ProfileInstallPatch,
    ToolsKeyboardShortcuts,
    ToolsCustomizeToolbar,
    ToolsUndoHistory,
    ToolsLanguageFormat,
    ToolsInstallLanguage,
    ToolsUseBuiltInEnglish,
    ToolsInstallFrontendConfiguration,
    ToolsInstallToolConfiguration,
    ToolsTestRomInEmulator,
    ToolsChooseEmulator,
    ToolsTestRomInEmulatorAction,
    ToolsBuiltInEnglish,
    HelpTopics,
    HelpCompatibilityDiagnostics,
    HelpStageVerifiedUpdate,
    HelpAbout,
    DocumentsOpenFormat,
    DocumentsCloseFormat,
    DocumentPortablePalette,
    DocumentPortableGraphics,
    DocumentPortableMap16Page,
    DocumentPortableExAnimation,
    DocumentPortableCompleteLevel,
    DocumentPortableCompleteOverworld,
    DocumentPortableOverworldPaths,
    DocumentPortableOverworldMetadata,
    DocumentPortableEntityAppearances,
    DocumentPortableOverworldAppearances,
    DocumentPortableLayer3,
    DocumentMwl,
    DocumentExpandedSettings,
    DocumentCustomObjectLibrary,
    DocumentCustomSpriteLibrary,
    DocumentNativeMap16Sidecar,
    DocumentDscSidecar,
    DocumentSscCustomSpriteMetadata,
    DocumentOscCustomObjectMetadata,
    DocumentCompleteMap16Set,
    DocumentNativeLevelStreams,
    DocumentNativeLevelAssets,
    HelpWindowTitle,
    HelpSearchLabel,
    HelpRustWorkflowGuides,
    HelpOriginalCommandIndex,
    HelpOriginalSection,
    HelpOriginalRoute,
    HelpOriginalNotice,
    HelpOpenOriginalContents,
    HelpOriginalOpened,
    HelpOriginalUnavailable,
    HelpNoMatches,
    AboutWindowTitleFormat,
    AboutVersionFormat,
    AboutBuildFormat,
    AboutCleanRoomIdentity,
    AboutCompatibilityTarget,
    AboutLicenseFormat,
    AboutSourceRepository,
    AboutCopySourceUrl,
    AboutSourceCopied,
    AboutThirdPartyEnhancements,
    AboutThirdPartyTitle,
    AboutThirdPartyBody,
    AboutLegalNotice,
    AboutLegalTitle,
    AboutLegalBody,
    AboutOk,
    DiagnosticsWindowTitle,
    DiagnosticsIntroduction,
    DiagnosticsCopy,
    DiagnosticsCopied,
    HelpGettingStartedTitle,
    HelpGettingStartedBody,
    HelpLevelEditingTitle,
    HelpLevelEditingBody,
    HelpEntrancesExitsTitle,
    HelpEntrancesExitsBody,
    HelpMap16GraphicsTitle,
    HelpMap16GraphicsBody,
    HelpPalettesBackgroundsLayer3Title,
    HelpPalettesBackgroundsLayer3Body,
    HelpOverworldEditingTitle,
    HelpOverworldEditingBody,
    HelpImportExportRecoveryTitle,
    HelpImportExportRecoveryBody,
    HelpCompatibilityDiagnosticsTitle,
    HelpCompatibilityDiagnosticsBody,
    EditorEditFormat,
    EditorCloseFormat,
    EditorInstalledExpandedSettings,
    EditorNativeLevelAssets,
    EditorNativeMap16Set,
    EditorNativePalette,
    EditorNativeGraphics,
    EditorNativeExAnimation,
    EditorNativeOverworld,
    EditorLunarMagicRomMetadata,
    EditorSharedCustomSmwPalettes,
    EditorGlobalSecondaryExits,
    EditorTitleScreenRecording,
    EditorTitleScreenTilemap,
    EditorCreditsTilemap,
    EditorOverworldPlayerStarts,
    EditorOverworldGlobalSettings,
    EditorOverworldEventNumberMap,
    EditorOverworldEventReveals,
    EditorOverworldEventTilemaps,
    EditorOverworldLevelNames,
    EditorBossSequenceMessages,
    EditorOverworldMessages,
    EditorOverworldPathLinks,
    EditorOverworldWarpLinks,
    EditorOverworldSpecialEvents,
    EditorExportAllMwl,
    EditorExportModifiedMwl,
    EditorInsertMultipleMwl,
    RecoveryWindowTitle,
    RecoveryAvailable,
    RecoveryCountFormat,
    RecoveryRevisionFormat,
    RecoveryLevelFormat,
    RecoveryRequiresSaveAs,
    RecoveryCloseCurrent,
    RecoveryAction,
    RecoveryDiscard,
    UnsavedChangesTitle,
    UnsavedChangesQuestion,
    CommonCancel,
    UnsavedDiscard,
    ErrorWindowTitle,
    CommonOk,
    UndoHistoryWindowTitle,
    UndoHistorySnapshotsLabel,
    UndoHistoryHint,
    CommonApply,
    ToolsAutoDetectLanguage,
    ToolsLiveEmulator,
    LiveEmulatorWindowTitle,
    LiveEmulatorPause,
    LiveEmulatorResume,
    LiveEmulatorStep,
    LiveEmulatorStop,
    FileDeleteLevel,
    DeleteLevelWindowTitle,
    DeleteLevelQuestion,
    CommonDelete,
    ToolsAnimationRate,
    FileOpenLevelNumber,
    FileOpenLevelAddress,
    FileScanRom,
    FileOpenLevelFile,
    FileExtractOldBypassList,
    FileInsertOldBypassList,
    FileDeleteMultipleLevels,
    DeleteMultipleLevelsWindowTitle,
    DeleteMultipleLevelsDescription,
    DeleteMultipleLevelsModified,
    DeleteMultipleLevelsUnmodified,
    DeleteMultipleLevelsAll,
    DeleteMultipleLevelsClearOriginal,
    DeleteMultipleLevelsDependencyWarning,
    FileClearOriginalLevelArea,
    ClearOriginalLevelAreaDescription,
    UpdateAvailableTitle,
    UpdateVersionFormat,
    UpdatePlatformFormat,
    UpdateArchiveFormat,
    UpdateSizeFormat,
    UpdateRunningSafeNotice,
    UpdateChooseStageFolder,
    UpdateStagedTitle,
    UpdateStagedReady,
    UpdateImmutableInstallNotice,
    UpdateKeepStaged,
    UpdateChooseInstallRoot,
    UpdateActivatedTitle,
    UpdateRestartNotice,
    UpdateVersionDirectoryFormat,
    UpdateSelectedExecutableFormat,
    UpdateRollbackNotice,
    UpdateVerificationFailedTitle,
    PaletteTransferExportTitle,
    PaletteTransferImportTitle,
    PaletteTransferChooseFormat,
    PaletteTransferRawFormat,
    PaletteTransferTplFormat,
    PaletteTransferRgbFormat,
    PaletteTransferMaskNotice,
    PaletteTransferErrorTitle,
    Map16SetEditorTitle,
    Map16SetPage,
    Map16SetAddBlankPage,
    Map16SetRemoveLastPage,
    Map16SetModified,
    Map16SetSaved,
    Map16SetAddressFormat,
    Map16SetTileLabel,
    Map16SetPriority,
    Map16SetHorizontalFlip,
    Map16SetVerticalFlip,
    Map16SetApplySubtile,
    Map16SetActsLikeLabel,
    Map16SetApplyActsLike,
    Map16SetPreviewUnavailable,
    Map16SetUnsavedTitle,
    Map16SetDiscardQuestion,
    Map16SetErrorTitle,
}

impl UiTextKey {
    #[must_use]
    pub const fn english(self) -> &'static str {
        match self {
            Self::AppTitle => "Lunar Magic Rust",
            Self::FileOpen => "Open",
            Self::FileSave => "Save",
            Self::FileSaveAs => "Save As",
            Self::FileClose => "Close",
            Self::FileQuit => "Quit",
            Self::EditUndo => "Undo",
            Self::EditRedo => "Redo",
            Self::EditCopy => "Copy",
            Self::EditCut => "Cut",
            Self::EditPaste => "Paste",
            Self::ViewLevel => "Level",
            Self::ViewOverworld => "Overworld",
            Self::ViewMap16 => "Map16",
            Self::ViewGraphics => "Graphics",
            Self::ViewPalette => "Palette",
            Self::ViewExAnimation => "ExAnimation",
            Self::StatusReady => "Ready",
            Self::ViewLayer3 => "Layer 3",
            Self::MenuFile => "File",
            Self::MenuEdit => "Edit",
            Self::MenuView => "View",
            Self::MenuEditors => "Editors",
            Self::MenuProfile => "Profile",
            Self::MenuTools => "Tools",
            Self::MenuDocuments => "Documents",
            Self::MenuHelp => "Help",
            Self::FileOpenRecent => "Open Recent",
            Self::FileExpandRom => "Expand ROM…",
            Self::FileConvertCopierHeader => "Convert Copier Header…",
            Self::FileAnalyzeLevelUsage => "Analyze Level Usage…",
            Self::FileRestrictLevelAccess => "Restrict Level Access…",
            Self::FileRestoreRom => "Restore ROM from Restore Point…",
            Self::FileCreateFullRestore => "Create Full Restore Point…",
            Self::FileAppendDeltaRestore => "Append Delta Restore Point…",
            Self::FileAppendFullRestore => "Append Full Restore Point…",
            Self::FileAppendAutomaticRestore => "Append Automatic Restore Point…",
            Self::FileMigrateGraphicsCompression => "Migrate Graphics Compression…",
            Self::FileInstallBuiltInRuntime => "Install Built-in Runtime…",
            Self::FileReclaimOwnedRatsBlocks => "Reclaim Owned RATS Blocks…",
            Self::FileApplyIpsPatch => "Apply IPS Patch…",
            Self::FileCreateIpsPatch => "Create IPS Patch…",
            Self::ViewSpecialWorldPassed => "Special World Passed Graphics",
            Self::ViewLayer1 => "Layer 1",
            Self::ViewLayer2 => "Layer 2",
            Self::ViewLayerSprites => "Sprites",
            Self::ProfileInstallRevision => "Install Revision Profile…",
            Self::ProfileClear => "Clear Profile",
            Self::ProfileInstallPatch => "Install Revision Patch…",
            Self::ToolsKeyboardShortcuts => "Keyboard Shortcuts…",
            Self::ToolsCustomizeToolbar => "Customize Toolbar…",
            Self::ToolsUndoHistory => "Undo History…",
            Self::ToolsAnimationRate => "Animation Rate",
            Self::FileOpenLevelNumber => "Open Level Number…",
            Self::FileOpenLevelAddress => "Open Level From Address…",
            Self::FileScanRom => "Scan ROM…",
            Self::FileOpenLevelFile => "Open Level From File…",
            Self::FileExtractOldBypassList => "Extract Old Bypass List from ROM…",
            Self::FileInsertOldBypassList => "Insert Old Bypass List to ROM…",
            Self::ToolsLanguageFormat => "Language ({locale})",
            Self::ToolsInstallLanguage => "Install Language Catalog…",
            Self::ToolsUseBuiltInEnglish => "Use Built-in English",
            Self::ToolsAutoDetectLanguage => "Auto-detect System Language",
            Self::ToolsLiveEmulator => "Live ROM Test (Libretro)…",
            Self::LiveEmulatorWindowTitle => "Live ROM Test",
            Self::LiveEmulatorPause => "Pause",
            Self::LiveEmulatorResume => "Resume",
            Self::LiveEmulatorStep => "Step",
            Self::LiveEmulatorStop => "Stop",
            Self::ToolsInstallFrontendConfiguration => "Install Frontend Configuration…",
            Self::ToolsInstallToolConfiguration => "Install Tool Configuration…",
            Self::ToolsTestRomInEmulator => "Test ROM in Emulator",
            Self::ToolsChooseEmulator => "Choose Emulator…",
            Self::ToolsTestRomInEmulatorAction => "Test ROM in Emulator…",
            Self::ToolsBuiltInEnglish => "Built-in English",
            Self::HelpTopics => "Help Topics…",
            Self::HelpCompatibilityDiagnostics => "Compatibility diagnostics…",
            Self::HelpStageVerifiedUpdate => "Stage verified update…",
            Self::HelpAbout => "About Lunar Magic Rust…",
            Self::DocumentsOpenFormat => "Open {document}…",
            Self::DocumentsCloseFormat => "Close {document}",
            Self::DocumentPortablePalette => "Portable Palette",
            Self::DocumentPortableGraphics => "Portable Graphics",
            Self::DocumentPortableMap16Page => "Portable Map16 Page",
            Self::DocumentPortableExAnimation => "Portable ExAnimation",
            Self::DocumentPortableCompleteLevel => "Portable Complete Level",
            Self::DocumentPortableCompleteOverworld => "Portable Complete Overworld",
            Self::DocumentPortableOverworldPaths => "Portable Overworld Paths",
            Self::DocumentPortableOverworldMetadata => "Portable Overworld Metadata",
            Self::DocumentPortableEntityAppearances => "Portable Entity Appearances",
            Self::DocumentPortableOverworldAppearances => "Portable Overworld Appearances",
            Self::DocumentPortableLayer3 => "Portable Layer 3",
            Self::DocumentMwl => "MWL",
            Self::DocumentExpandedSettings => "Expanded Settings",
            Self::DocumentCustomObjectLibrary => "Custom Object Library",
            Self::DocumentCustomSpriteLibrary => "Custom Sprite Library",
            Self::DocumentNativeMap16Sidecar => "Native Map16 Sidecar",
            Self::DocumentDscSidecar => "DSC Sidecar",
            Self::DocumentSscCustomSpriteMetadata => "SSC Custom-Sprite Metadata",
            Self::DocumentOscCustomObjectMetadata => "OSC Custom-Object Metadata",
            Self::DocumentCompleteMap16Set => "Complete Map16 Set",
            Self::DocumentNativeLevelStreams => "Native Level Streams",
            Self::DocumentNativeLevelAssets => "Native Level Assets",
            Self::HelpWindowTitle => "Lunar Magic Rust Help",
            Self::HelpSearchLabel => "Search:",
            Self::HelpRustWorkflowGuides => "Rust workflow guides",
            Self::HelpOriginalCommandIndex => "Lunar Magic 3.63 command index",
            Self::HelpOriginalSection => "Original Lunar Magic 3.63 help section",
            Self::HelpOriginalRoute => "Original Lunar Magic 3.63 help route",
            Self::HelpOriginalNotice => {
                "This retained index identifies the original workflow without redistributing the proprietary help text. Search the Rust workflow guides for native usage and Compatibility diagnostics for ROM-specific state."
            }
            Self::HelpOpenOriginalContents => "Open installed Lunar Magic.chm contents",
            Self::HelpOriginalOpened => "Opened the installed Lunar Magic help file.",
            Self::HelpOriginalUnavailable => {
                "Could not open the installed Lunar Magic help file: {error}"
            }
            Self::HelpNoMatches => "No help topics match this search.",
            Self::AboutWindowTitleFormat => "About {product}",
            Self::AboutVersionFormat => "Version {version}",
            Self::AboutBuildFormat => "{build} build for {os}/{arch}",
            Self::AboutCleanRoomIdentity => "Clean-room Rust reimplementation",
            Self::AboutCompatibilityTarget => "Lunar Magic 3.63 workflow compatibility",
            Self::AboutLicenseFormat => "License: {license}",
            Self::AboutSourceRepository => "Source repository",
            Self::AboutCopySourceUrl => "Copy source URL",
            Self::AboutSourceCopied => "Source URL copied.",
            Self::AboutThirdPartyEnhancements => "Third Party Enhancements",
            Self::AboutThirdPartyTitle => "Third Party Enhancements",
            Self::AboutThirdPartyBody => {
                "External tools, ROM patches, emulators, plugins, and proprietary Lunar Magic resources are separate third-party components. They are not bundled with Lunar Magic Rust and retain their own licenses and support boundaries."
            }
            Self::AboutLegalNotice => "Legal Notice",
            Self::AboutLegalTitle => "Legal Notice",
            Self::AboutLegalBody => {
                "Lunar Magic Rust is an independent clean-room reimplementation licensed under MIT OR Apache-2.0. Super Mario World, Lunar Magic, and related names and assets belong to their respective owners. No proprietary game ROM, help text, or original executable is distributed with this project."
            }
            Self::AboutOk => "OK",
            Self::DiagnosticsWindowTitle => "Compatibility diagnostics",
            Self::DiagnosticsIntroduction => {
                "Path-free build and ROM information for compatibility reports:"
            }
            Self::DiagnosticsCopy => "Copy diagnostics",
            Self::DiagnosticsCopied => "Diagnostics copied.",
            Self::HelpGettingStartedTitle => "Getting started",
            Self::HelpGettingStartedBody => {
                "Open a clean Super Mario World ROM with File > Open. Select a level from the level field, then use the Editors menu to open level, Map16, graphics, palette, ExAnimation, Layer 3, and overworld tools. Save writes the checked in-memory ROM transaction; Save As publishes a new file."
            }
            Self::HelpLevelEditingTitle => "Level editing",
            Self::HelpLevelEditingBody => {
                "Use the level canvas to select, place, drag, resize, duplicate, and remove objects and sprites. The canvas fits one 256 by 224 SNES screen into the available pane and recomputes its scale when the window changes size. View toggles control Layer 1, Layer 2, Layer 3, and sprites without deleting their data."
            }
            Self::HelpEntrancesExitsTitle => "Entrances and exits",
            Self::HelpEntrancesExitsBody => {
                "Primary, midway, secondary entrances, and screen exits are edited through their typed forms. Screen and coordinate fields are bounded to their native packed widths. Changes participate in the same undo, redo, checksum, save, and reopen transaction as level objects and sprites."
            }
            Self::HelpMap16GraphicsTitle => "Map16 and graphics",
            Self::HelpMap16GraphicsBody => {
                "The Map16 editor changes visual quadrants, palette, priority, flips, and acts-like behavior. Graphics and ExGFX tools import, export, decode, and edit the active slots. Super GFX Bypass selects per-level foreground, background, and sprite files; animation options update the live preview."
            }
            Self::HelpPalettesBackgroundsLayer3Title => "Palettes, backgrounds, and Layer 3",
            Self::HelpPalettesBackgroundsLayer3Body => {
                "Palette editors provide shared and per-level colors with protected ownership checks. Background and Layer 3 editors expose tilemaps, offsets, graphics selection, priority, and composition. Preview and image export use the staged palette and animation phase currently shown in the editor."
            }
            Self::HelpOverworldEditingTitle => "Overworld editing",
            Self::HelpOverworldEditingBody => {
                "Overworld tools edit Layer 1 paths and events, Layer 2 appearance, level tiles, names, messages, warps, player starts, and special-event state. Each editor stages a checked revision and can be undone before or after saving."
            }
            Self::HelpImportExportRecoveryTitle => "Import, export, and recovery",
            Self::HelpImportExportRecoveryBody => {
                "Level workflows support one-level MWL transfer, directory batch import, all-level export, and PNG or BMP image export. Restore points preserve ROM and associated files. Crash recovery records unsaved ROM revisions and offers them on the next launch without overwriting the last saved file."
            }
            Self::HelpCompatibilityDiagnosticsTitle => "Compatibility diagnostics",
            Self::HelpCompatibilityDiagnosticsBody => {
                "Help > Compatibility diagnostics creates a path-free report describing the build, ROM identity, mapper, checksum, revision profile, runtime generations, and current editor state. Copy that report when a ROM or feature behaves differently from Lunar Magic 3.63."
            }
            Self::EditorEditFormat => "Edit {editor}…",
            Self::EditorCloseFormat => "Close {editor}",
            Self::EditorInstalledExpandedSettings => "Installed Expanded Settings",
            Self::EditorNativeLevelAssets => "Native Level Assets",
            Self::EditorNativeMap16Set => "Native Map16 Set",
            Self::EditorNativePalette => "Native Palette",
            Self::EditorNativeGraphics => "Native Graphics",
            Self::EditorNativeExAnimation => "Native ExAnimation",
            Self::EditorNativeOverworld => "Native Overworld",
            Self::EditorLunarMagicRomMetadata => "Lunar Magic ROM Metadata",
            Self::EditorSharedCustomSmwPalettes => "Shared/Custom SMW Palettes",
            Self::EditorGlobalSecondaryExits => "Global Secondary Exits",
            Self::EditorTitleScreenRecording => "Title-Screen Recording",
            Self::EditorTitleScreenTilemap => "Title-Screen Tilemap",
            Self::EditorCreditsTilemap => "Credits Tilemap",
            Self::EditorOverworldPlayerStarts => "Overworld Player Starts",
            Self::EditorOverworldGlobalSettings => "Overworld Global Settings",
            Self::EditorOverworldEventNumberMap => "Overworld Event-Number Map",
            Self::EditorOverworldEventReveals => "Overworld Event Reveals",
            Self::EditorOverworldEventTilemaps => "Overworld Event Tilemaps",
            Self::EditorOverworldLevelNames => "Overworld Level Names",
            Self::EditorBossSequenceMessages => "Boss-Sequence Messages",
            Self::EditorOverworldMessages => "Overworld Messages",
            Self::EditorOverworldPathLinks => "Overworld Path Links",
            Self::EditorOverworldWarpLinks => "Overworld Warp Links",
            Self::EditorOverworldSpecialEvents => "Overworld Special Events",
            Self::EditorExportAllMwl => "Export All MWL Levels…",
            Self::EditorExportModifiedMwl => "Export Modified MWL Levels…",
            Self::EditorInsertMultipleMwl => "Insert Multiple MWL Levels…",
            Self::RecoveryWindowTitle => "Recover unsaved ROM",
            Self::RecoveryAvailable => {
                "An unsaved ROM snapshot from an interrupted session is available."
            }
            Self::RecoveryCountFormat => "Recovery 1 of {count}; remaining snapshots will follow.",
            Self::RecoveryRevisionFormat => "Revision: {revision}",
            Self::RecoveryLevelFormat => "Last active level: {level}",
            Self::RecoveryRequiresSaveAs => {
                "Recovery opens an unnamed dirty copy and requires Save As."
            }
            Self::RecoveryCloseCurrent => "Close the current ROM before recovering this snapshot.",
            Self::RecoveryAction => "Recover",
            Self::RecoveryDiscard => "Discard Recovery",
            Self::UnsavedChangesTitle => "Unsaved changes",
            Self::UnsavedChangesQuestion => "Discard unsaved changes?",
            Self::CommonCancel => "Cancel",
            Self::UnsavedDiscard => "Discard",
            Self::ErrorWindowTitle => "Error",
            Self::CommonOk => "OK",
            Self::UndoHistoryWindowTitle => "Undo History",
            Self::UndoHistorySnapshotsLabel => {
                "Snapshots retained for the level and overworld editors"
            }
            Self::UndoHistoryHint => {
                "0 or 1 disables Undo. Lunar Magic 3.63 defaults to 33 and allows at most 51."
            }
            Self::CommonApply => "Apply",
            Self::FileDeleteLevel => "Delete Level from ROM…",
            Self::DeleteLevelWindowTitle => "Delete Level from ROM",
            Self::DeleteLevelQuestion => {
                "Delete level {level} from the expanded ROM area and replace it with the original test level?"
            }
            Self::CommonDelete => "Delete",
            Self::FileDeleteMultipleLevels => "Delete Multiple Levels from ROM…",
            Self::DeleteMultipleLevelsWindowTitle => "Delete Multiple Levels from ROM",
            Self::DeleteMultipleLevelsDescription => {
                "Deleted levels are replaced with the test level in the original ROM area."
            }
            Self::DeleteMultipleLevelsModified => "Modified levels",
            Self::DeleteMultipleLevelsUnmodified => "Unmodified levels",
            Self::DeleteMultipleLevelsAll => "All levels",
            Self::DeleteMultipleLevelsClearOriginal => "Clear Original Level Data Area",
            Self::DeleteMultipleLevelsDependencyWarning => {
                "Check dependencies before deleting: 000/100 bonus games, C8/1C8 Yoshi Wing, C5 intro, C7 title screen, and 104 Yoshi's House are used by the game."
            }
            Self::FileClearOriginalLevelArea => "Clear Original Level Data Area…",
            Self::ClearOriginalLevelAreaDescription => {
                "This will resave levels that have not been modified into the expanded ROM area, then clear the original level-data area for reuse."
            }
            Self::UpdateAvailableTitle => "Verified update available",
            Self::UpdateVersionFormat => "Version: {version}",
            Self::UpdatePlatformFormat => "Platform: {platform}",
            Self::UpdateArchiveFormat => "Archive: {archive}",
            Self::UpdateSizeFormat => "Size: {bytes} bytes",
            Self::UpdateRunningSafeNotice => {
                "The current application will not be replaced automatically."
            }
            Self::UpdateChooseStageFolder => "Choose folder and stage verified archive",
            Self::UpdateStagedTitle => "Update staged",
            Self::UpdateStagedReady => "The verified archive is ready for immutable installation.",
            Self::UpdateImmutableInstallNotice => {
                "Installation creates a new version directory and changes only the rollback-safe launcher selector."
            }
            Self::UpdateKeepStaged => "Keep staged only",
            Self::UpdateChooseInstallRoot => "Choose install root and activate",
            Self::UpdateActivatedTitle => "Update activated",
            Self::UpdateRestartNotice => "Exit this application, then restart through lm-launcher.",
            Self::UpdateVersionDirectoryFormat => "Version directory: {path}",
            Self::UpdateSelectedExecutableFormat => "Selected executable: {path}",
            Self::UpdateRollbackNotice => {
                "The previous selected version remains available for rollback."
            }
            Self::UpdateVerificationFailedTitle => "Update verification failed",
            Self::PaletteTransferExportTitle => "Export Current-Level Palette",
            Self::PaletteTransferImportTitle => "Import Current-Level Palette",
            Self::PaletteTransferChooseFormat => "Choose Lunar Magic's native transfer format:",
            Self::PaletteTransferRawFormat => "Raw 257-color",
            Self::PaletteTransferTplFormat => "TPL v2",
            Self::PaletteTransferRgbFormat => "RGB24",
            Self::PaletteTransferMaskNotice => {
                "Imports automatically apply a same-name .palmask sidecar when present."
            }
            Self::PaletteTransferErrorTitle => "Current-level palette transfer error",
            Self::Map16SetEditorTitle => "Complete Map16 Set Editor",
            Self::Map16SetPage => "Page",
            Self::Map16SetAddBlankPage => "Add blank page",
            Self::Map16SetRemoveLastPage => "Remove last page",
            Self::Map16SetModified => "Modified",
            Self::Map16SetSaved => "Saved",
            Self::Map16SetAddressFormat => "Address {address}",
            Self::Map16SetTileLabel => "8×8 tile (hex)",
            Self::Map16SetPriority => "Priority",
            Self::Map16SetHorizontalFlip => "Horizontal flip",
            Self::Map16SetVerticalFlip => "Vertical flip",
            Self::Map16SetApplySubtile => "Apply subtile",
            Self::Map16SetActsLikeLabel => "Acts Like (hex)",
            Self::Map16SetApplyActsLike => "Apply Acts Like",
            Self::Map16SetPreviewUnavailable => "Preview unavailable",
            Self::Map16SetUnsavedTitle => "Unsaved complete Map16 set",
            Self::Map16SetDiscardQuestion => "Discard unsaved complete Map16 changes?",
            Self::Map16SetErrorTitle => "Complete Map16 editor error",
        }
    }

    pub const ALL: [Self; 256] = [
        Self::AppTitle,
        Self::FileOpen,
        Self::FileSave,
        Self::FileSaveAs,
        Self::FileClose,
        Self::FileQuit,
        Self::EditUndo,
        Self::EditRedo,
        Self::EditCopy,
        Self::EditCut,
        Self::EditPaste,
        Self::ViewLevel,
        Self::ViewOverworld,
        Self::ViewMap16,
        Self::ViewGraphics,
        Self::ViewPalette,
        Self::ViewExAnimation,
        Self::StatusReady,
        Self::ViewLayer3,
        Self::MenuFile,
        Self::MenuEdit,
        Self::MenuView,
        Self::MenuEditors,
        Self::MenuProfile,
        Self::MenuTools,
        Self::MenuDocuments,
        Self::MenuHelp,
        Self::FileOpenRecent,
        Self::FileExpandRom,
        Self::FileConvertCopierHeader,
        Self::FileAnalyzeLevelUsage,
        Self::FileRestrictLevelAccess,
        Self::FileRestoreRom,
        Self::FileCreateFullRestore,
        Self::FileAppendDeltaRestore,
        Self::FileAppendFullRestore,
        Self::FileAppendAutomaticRestore,
        Self::FileMigrateGraphicsCompression,
        Self::FileInstallBuiltInRuntime,
        Self::FileReclaimOwnedRatsBlocks,
        Self::FileApplyIpsPatch,
        Self::FileCreateIpsPatch,
        Self::ViewSpecialWorldPassed,
        Self::ViewLayer1,
        Self::ViewLayer2,
        Self::ViewLayerSprites,
        Self::ProfileInstallRevision,
        Self::ProfileClear,
        Self::ProfileInstallPatch,
        Self::ToolsKeyboardShortcuts,
        Self::ToolsCustomizeToolbar,
        Self::ToolsUndoHistory,
        Self::ToolsLanguageFormat,
        Self::ToolsInstallLanguage,
        Self::ToolsUseBuiltInEnglish,
        Self::ToolsInstallFrontendConfiguration,
        Self::ToolsInstallToolConfiguration,
        Self::ToolsTestRomInEmulator,
        Self::ToolsChooseEmulator,
        Self::ToolsTestRomInEmulatorAction,
        Self::ToolsBuiltInEnglish,
        Self::HelpTopics,
        Self::HelpCompatibilityDiagnostics,
        Self::HelpStageVerifiedUpdate,
        Self::HelpAbout,
        Self::DocumentsOpenFormat,
        Self::DocumentsCloseFormat,
        Self::DocumentPortablePalette,
        Self::DocumentPortableGraphics,
        Self::DocumentPortableMap16Page,
        Self::DocumentPortableExAnimation,
        Self::DocumentPortableCompleteLevel,
        Self::DocumentPortableCompleteOverworld,
        Self::DocumentPortableOverworldPaths,
        Self::DocumentPortableOverworldMetadata,
        Self::DocumentPortableEntityAppearances,
        Self::DocumentPortableOverworldAppearances,
        Self::DocumentPortableLayer3,
        Self::DocumentMwl,
        Self::DocumentExpandedSettings,
        Self::DocumentCustomObjectLibrary,
        Self::DocumentCustomSpriteLibrary,
        Self::DocumentNativeMap16Sidecar,
        Self::DocumentDscSidecar,
        Self::DocumentSscCustomSpriteMetadata,
        Self::DocumentOscCustomObjectMetadata,
        Self::DocumentCompleteMap16Set,
        Self::DocumentNativeLevelStreams,
        Self::DocumentNativeLevelAssets,
        Self::HelpWindowTitle,
        Self::HelpSearchLabel,
        Self::HelpRustWorkflowGuides,
        Self::HelpOriginalCommandIndex,
        Self::HelpOriginalSection,
        Self::HelpOriginalRoute,
        Self::HelpOriginalNotice,
        Self::HelpOpenOriginalContents,
        Self::HelpOriginalOpened,
        Self::HelpOriginalUnavailable,
        Self::HelpNoMatches,
        Self::AboutWindowTitleFormat,
        Self::AboutVersionFormat,
        Self::AboutBuildFormat,
        Self::AboutCleanRoomIdentity,
        Self::AboutCompatibilityTarget,
        Self::AboutLicenseFormat,
        Self::AboutSourceRepository,
        Self::AboutCopySourceUrl,
        Self::AboutSourceCopied,
        Self::AboutThirdPartyEnhancements,
        Self::AboutThirdPartyTitle,
        Self::AboutThirdPartyBody,
        Self::AboutLegalNotice,
        Self::AboutLegalTitle,
        Self::AboutLegalBody,
        Self::AboutOk,
        Self::DiagnosticsWindowTitle,
        Self::DiagnosticsIntroduction,
        Self::DiagnosticsCopy,
        Self::DiagnosticsCopied,
        Self::HelpGettingStartedTitle,
        Self::HelpGettingStartedBody,
        Self::HelpLevelEditingTitle,
        Self::HelpLevelEditingBody,
        Self::HelpEntrancesExitsTitle,
        Self::HelpEntrancesExitsBody,
        Self::HelpMap16GraphicsTitle,
        Self::HelpMap16GraphicsBody,
        Self::HelpPalettesBackgroundsLayer3Title,
        Self::HelpPalettesBackgroundsLayer3Body,
        Self::HelpOverworldEditingTitle,
        Self::HelpOverworldEditingBody,
        Self::HelpImportExportRecoveryTitle,
        Self::HelpImportExportRecoveryBody,
        Self::HelpCompatibilityDiagnosticsTitle,
        Self::HelpCompatibilityDiagnosticsBody,
        Self::EditorEditFormat,
        Self::EditorCloseFormat,
        Self::EditorInstalledExpandedSettings,
        Self::EditorNativeLevelAssets,
        Self::EditorNativeMap16Set,
        Self::EditorNativePalette,
        Self::EditorNativeGraphics,
        Self::EditorNativeExAnimation,
        Self::EditorNativeOverworld,
        Self::EditorLunarMagicRomMetadata,
        Self::EditorSharedCustomSmwPalettes,
        Self::EditorGlobalSecondaryExits,
        Self::EditorTitleScreenRecording,
        Self::EditorTitleScreenTilemap,
        Self::EditorCreditsTilemap,
        Self::EditorOverworldPlayerStarts,
        Self::EditorOverworldGlobalSettings,
        Self::EditorOverworldEventNumberMap,
        Self::EditorOverworldEventReveals,
        Self::EditorOverworldEventTilemaps,
        Self::EditorOverworldLevelNames,
        Self::EditorBossSequenceMessages,
        Self::EditorOverworldMessages,
        Self::EditorOverworldPathLinks,
        Self::EditorOverworldWarpLinks,
        Self::EditorOverworldSpecialEvents,
        Self::EditorExportAllMwl,
        Self::EditorExportModifiedMwl,
        Self::EditorInsertMultipleMwl,
        Self::RecoveryWindowTitle,
        Self::RecoveryAvailable,
        Self::RecoveryCountFormat,
        Self::RecoveryRevisionFormat,
        Self::RecoveryLevelFormat,
        Self::RecoveryRequiresSaveAs,
        Self::RecoveryCloseCurrent,
        Self::RecoveryAction,
        Self::RecoveryDiscard,
        Self::UnsavedChangesTitle,
        Self::UnsavedChangesQuestion,
        Self::CommonCancel,
        Self::UnsavedDiscard,
        Self::ErrorWindowTitle,
        Self::CommonOk,
        Self::UndoHistoryWindowTitle,
        Self::UndoHistorySnapshotsLabel,
        Self::UndoHistoryHint,
        Self::CommonApply,
        Self::ToolsAutoDetectLanguage,
        Self::ToolsLiveEmulator,
        Self::LiveEmulatorWindowTitle,
        Self::LiveEmulatorPause,
        Self::LiveEmulatorResume,
        Self::LiveEmulatorStep,
        Self::LiveEmulatorStop,
        Self::FileDeleteLevel,
        Self::DeleteLevelWindowTitle,
        Self::DeleteLevelQuestion,
        Self::CommonDelete,
        Self::ToolsAnimationRate,
        Self::FileOpenLevelNumber,
        Self::FileOpenLevelAddress,
        Self::FileScanRom,
        Self::FileOpenLevelFile,
        Self::FileExtractOldBypassList,
        Self::FileInsertOldBypassList,
        Self::FileDeleteMultipleLevels,
        Self::DeleteMultipleLevelsWindowTitle,
        Self::DeleteMultipleLevelsDescription,
        Self::DeleteMultipleLevelsModified,
        Self::DeleteMultipleLevelsUnmodified,
        Self::DeleteMultipleLevelsAll,
        Self::DeleteMultipleLevelsClearOriginal,
        Self::DeleteMultipleLevelsDependencyWarning,
        Self::FileClearOriginalLevelArea,
        Self::ClearOriginalLevelAreaDescription,
        Self::UpdateAvailableTitle,
        Self::UpdateVersionFormat,
        Self::UpdatePlatformFormat,
        Self::UpdateArchiveFormat,
        Self::UpdateSizeFormat,
        Self::UpdateRunningSafeNotice,
        Self::UpdateChooseStageFolder,
        Self::UpdateStagedTitle,
        Self::UpdateStagedReady,
        Self::UpdateImmutableInstallNotice,
        Self::UpdateKeepStaged,
        Self::UpdateChooseInstallRoot,
        Self::UpdateActivatedTitle,
        Self::UpdateRestartNotice,
        Self::UpdateVersionDirectoryFormat,
        Self::UpdateSelectedExecutableFormat,
        Self::UpdateRollbackNotice,
        Self::UpdateVerificationFailedTitle,
        Self::PaletteTransferExportTitle,
        Self::PaletteTransferImportTitle,
        Self::PaletteTransferChooseFormat,
        Self::PaletteTransferRawFormat,
        Self::PaletteTransferTplFormat,
        Self::PaletteTransferRgbFormat,
        Self::PaletteTransferMaskNotice,
        Self::PaletteTransferErrorTitle,
        Self::Map16SetEditorTitle,
        Self::Map16SetPage,
        Self::Map16SetAddBlankPage,
        Self::Map16SetRemoveLastPage,
        Self::Map16SetModified,
        Self::Map16SetSaved,
        Self::Map16SetAddressFormat,
        Self::Map16SetTileLabel,
        Self::Map16SetPriority,
        Self::Map16SetHorizontalFlip,
        Self::Map16SetVerticalFlip,
        Self::Map16SetApplySubtile,
        Self::Map16SetActsLikeLabel,
        Self::Map16SetApplyActsLike,
        Self::Map16SetPreviewUnavailable,
        Self::Map16SetUnsavedTitle,
        Self::Map16SetDiscardQuestion,
        Self::Map16SetErrorTitle,
    ];

    fn from_byte(value: u8) -> Option<Self> {
        Self::ALL.get(usize::from(value)).copied()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalizationCatalog {
    pub locale: String,
    entries: BTreeMap<UiTextKey, String>,
    dialog_entries: BTreeMap<OriginalDialogTextKey, String>,
}

const DIALOG_TITLE_ITEM_INDEX: u16 = u16::MAX;
const DIALOG_TITLE_CONTROL_ID: u32 = u32::MAX;
const RUST_UI_DIALOG_ID: u16 = u16::MAX;
const RUST_UI_ITEM_INDEX: u16 = u16::MAX - 1;

/// Stable typed identities for Rust-native text beyond the fixed 256-key `LMLOC001` prefix.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u32)]
pub enum ExtendedUiTextKey {
    MwlDocumentEditorTitle,
    MwlDocumentVersionFormat,
    MwlDocumentFlagsHex,
    MwlDocumentAttributionNotice,
    MwlDocumentLevelNumberNotice,
    MwlDocumentApplyHeader,
    MwlDocumentLayer3Heading,
    MwlDocumentLayer3Unavailable,
    MwlDocumentLayer3Enable,
    MwlDocumentLayer3File,
    MwlDocumentLengthSelector,
    MwlDocumentDestinationSelector,
    MwlDocumentExpandedMode,
    MwlDocumentApplyLayer3,
    MwlDocumentEntranceHeading,
    MwlDocumentEntranceNotice,
    MwlDocumentMainPosition,
    MwlDocumentMainVertical,
    MwlDocumentMainScreenMethod,
    MwlDocumentMainModeScreen,
    MwlDocumentMainFlags,
    MwlDocumentMainHighPosition,
    MwlDocumentMainAdditionalFlags,
    MwlDocumentMidwayPosition,
    MwlDocumentMidwayFlags,
    MwlDocumentMidwayHighPosition,
    MwlDocumentMidwayAdditionalFlags,
    MwlDocumentSeparateLayer2Scroll,
    MwlDocumentOriginalScrollPreset,
    MwlDocumentHorizontalSelector,
    MwlDocumentVerticalSelector,
    MwlDocumentSpriteSpawning,
    MwlDocumentVerticalSpawnRange,
    MwlDocumentSmartSpawn,
    MwlDocumentSectionLevelHeader,
    MwlDocumentSectionLayer1,
    MwlDocumentSectionLayer2,
    MwlDocumentSectionSprites,
    MwlDocumentSectionPalette,
    MwlDocumentSectionSecondaryExits,
    MwlDocumentSectionExAnimation,
    MwlDocumentSectionExpandedHeader,
    MwlDocumentSectionLengthFormat,
    MwlDocumentSectionBytes,
    MwlDocumentReplaceSection,
    MwlDocumentUndo,
    MwlDocumentRedo,
    MwlDocumentSave,
    MwlDocumentModified,
    MwlDocumentSaved,
    MwlDocumentDiscardTitle,
    MwlDocumentUnsavedNotice,
    MwlDocumentCancel,
    MwlDocumentDiscard,
    MwlDocumentErrorTitle,
    MwlDocumentOk,
    MwlObjectHeading,
    MwlObjectCountFormat,
    MwlObjectHeader,
    MwlObjectStageHeader,
    MwlObjectRecord,
    MwlObjectCommit,
    MwlObjectRecoveredFields,
    MwlObjectCommandId,
    MwlObjectParameter,
    MwlObjectFirstCoordinate,
    MwlObjectSecondCoordinate,
    MwlObjectAdvancesScreen,
    MwlObjectStageFields,
    MwlObjectJumpEncodingFormat,
    MwlObjectResolvedScreenFormat,
    MwlObjectOutsideScreenSuffix,
    MwlObjectJumpTarget,
    MwlObjectStageJumpTarget,
    MwlInsertBefore,
    MwlReplace,
    MwlDelete,
    MwlMoveUp,
    MwlMoveDown,
    MwlSpriteHeading,
    MwlSpriteExpanded,
    MwlSpriteTokenCountFormat,
    MwlSpriteStageHeader,
    MwlSpriteRecordBytes,
    MwlSpriteUpperYToken,
    MwlSpriteControlToken,
    MwlSpriteCommit,
    MwlSpriteLengthNotice,
    MwlSpriteSetLength,
    MwlSpriteResetLengths,
    MwlSpriteRecoveredFields,
    MwlSpriteYLow,
    MwlSpriteExtraBits,
    MwlSpriteScreen,
    MwlSpriteX,
    MwlSpriteNumber,
    MwlSpriteStageFields,
    MwlOptionalImportHeading,
    MwlOptionalMaximumRecords,
    MwlOptionalImport,
    MwlOptionalInterpret,
    MwlOptionalImportNotice,
    MwlOptionalHeading,
    MwlOptionalPalette,
    MwlOptionalExAnimation,
    MwlOptionalPaletteMetadata,
    MwlOptionalExAnimationMetadata,
    MwlOptionalColorFormat,
    MwlOptionalFeaturesHeading,
    MwlOptionalPaletteAnimation,
    MwlOptionalVanillaAnimation,
    MwlOptionalGlobalAnimation,
    MwlOptionalLevelAnimation,
    MwlOptionalApplyFeatures,
    MwlOptionalPreservedNibbleFormat,
    MwlOptionalCreateAnimation,
    MwlOptionalSetting,
    MwlOptionalHeader,
    MwlOptionalApplyGlobals,
    MwlOptionalTrigger,
    MwlOptionalTriggerEnabled,
    MwlOptionalApplyTrigger,
    MwlOptionalKind,
    MwlOptionalDestination,
    MwlOptionalDestinationFlag,
    MwlOptionalSourceWords,
    MwlOptionalAppendRecord,
    MwlOptionalReplaceRecord,
    MwlOptionalRemoveRecord,
    MwlOptionalFrameHeading,
    MwlOptionalSourceWordList,
    MwlOptionalMoveBefore,
    MwlOptionalInsertFrame,
    MwlOptionalReplaceFrame,
    MwlOptionalRemoveFrame,
    MwlOptionalMoveFrame,
    MwlOptionalWord0,
    MwlOptionalWord1,
    MwlOptionalApplyMetadata,
    Map16SidecarEditorTitle,
    Map16SidecarInterpretTitle,
    Map16SidecarM16Kind,
    Map16SidecarS16Kind,
    Map16SidecarCancel,
    Map16SidecarOpen,
    Map16SidecarM16Exact,
    Map16SidecarS16Canonical,
    Map16SidecarSummaryFormat,
    Map16SidecarRawEntry,
    Map16SidecarRawDword,
    Map16SidecarApplyRaw,
    Map16SidecarDefinitionFormat,
    Map16SidecarQuadrant,
    Map16SidecarTile,
    Map16SidecarPalette,
    Map16SidecarPriority,
    Map16SidecarHorizontalFlip,
    Map16SidecarVerticalFlip,
    Map16SidecarApplySubtile,
    Map16SidecarUndo,
    Map16SidecarRedo,
    Map16SidecarSave,
    Map16SidecarModified,
    Map16SidecarSaved,
    Map16SidecarDiscardTitle,
    Map16SidecarDiscardNotice,
    Map16SidecarDiscard,
    Map16SidecarErrorTitle,
    Map16SidecarOk,
    ToolbarEditorTitle,
    ToolbarEditorNotice,
    ToolbarEditorDefaultNotice,
    ToolbarEditorMoveUp,
    ToolbarEditorMoveDown,
    ToolbarEditorRemove,
    ToolbarEditorAddButton,
    ToolbarEditorAddSeparator,
    ToolbarEditorApply,
    ToolbarEditorUseDefault,
    ToolbarEditorCancel,
    ToolbarEditorSeparator,
    RestoreAutomaticTitle,
    RestoreInterval,
    RestoreDaily,
    RestoreDestructive,
    RestoreContinuityNotice,
    RestoreAppend,
    RestoreCancel,
    RestoreAutomaticComplete,
    RestoreArchiveFormat,
    RestoreOriginalFormat,
    RestoreTargetFormat,
    RestoreId,
    RestoreDateTime,
    RestoreType,
    RestoreDescription,
    RestoreAm,
    RestorePm,
    RestoreReversion,
    RestoreFull,
    RestoreDelta,
    RestoreReplaceWarning,
    RestoreRunningTitle,
    RestorePointFormat,
    RestoreRunningTargetFormat,
    RestoreRunningNotice,
    RestoreCompleteTitle,
    RestoreErrorTitle,
    RestoreOk,
    RestoreAssociatedOne,
    RestoreAssociatedManyFormat,
    RestoreCompleteFormat,
    LevelUsageOutputFormat,
    LevelUsageProgressTitle,
    LevelUsageLevelsFormat,
    LevelUsageScanningFormat,
    LevelUsageCancel,
    LevelUsageCompleteFormat,
    LevelUsageCompleteTitle,
    LevelUsageErrorTitle,
    LevelUsageOk,
    GraphicsMigrationAllocationNotice,
    GraphicsMigrationStart,
    GraphicsMigrationEnd,
    GraphicsMigrationErrorTitle,
    GraphicsMigrationOk,
    ShortcutEditorTitle,
    ShortcutEditorGestureNotice,
    ShortcutEditorPrimaryNotice,
    ShortcutEditorRemove,
    ShortcutEditorAdd,
    ShortcutEditorApply,
    ShortcutEditorClearAll,
    ShortcutEditorCancel,
    PathEditorTitle,
    PathEditorPolicyTitle,
    PathEditorReciprocalPolicy,
    PathEditorCancel,
    PathEditorOpen,
    PathEditorNodes,
    PathEditorEdges,
    PathEditorUndo,
    PathEditorRedo,
    PathEditorSave,
    PathEditorModified,
    PathEditorSaved,
    PathEditorNode,
    PathEditorEdge,
    PathEditorUpsertNode,
    PathEditorUpsertEdge,
    PathEditorRemoveSelected,
    PathEditorStableId,
    PathEditorX,
    PathEditorY,
    PathEditorLevel,
    PathEditorRawFlags,
    PathEditorFromNode,
    PathEditorToNode,
    PathEditorExit,
    PathEditorOneWay,
    PathEditorReciprocalPair,
    PathEditorReverseExit,
    PathEditorReverseRawFlags,
    PathEditorDiscardTitle,
    PathEditorDiscardNotice,
    PathEditorDiscard,
    PathEditorErrorTitle,
    PathEditorOk,
    PathEditorDirectionUp,
    PathEditorDirectionRight,
    PathEditorDirectionDown,
    PathEditorDirectionLeft,
    ExternalToolRunningTitleFormat,
    ExternalToolWaitingFormat,
    ExternalToolStop,
    ExternalToolAllowTitle,
    ExternalToolIdFormat,
    ExternalToolExecutableFormat,
    ExternalToolWorkingDirectoryFormat,
    ExternalToolInherited,
    ExternalToolArgumentsNotice,
    ExternalToolArgumentFormat,
    ExternalToolDeny,
    ExternalToolRun,
    ExternalToolCompletedFormat,
    ExternalToolStoppedFormat,
    NativeLevelDocumentTitle,
    NativeLevelDocumentSourceFormat,
    NativeLevelDocumentExpandedFraming,
    NativeLevelDocumentLegacyFraming,
    NativeLevelDocumentLegacyHeaderFormat,
    NativeLevelDocumentUndo,
    NativeLevelDocumentRedo,
    NativeLevelDocumentSave,
    NativeLevelDocumentApplySpriteHeader,
    NativeLevelDocumentModified,
    NativeLevelDocumentSaved,
    NativeLevelDocumentDiscardTitle,
    NativeLevelDocumentDiscardNotice,
    NativeLevelDocumentCancel,
    NativeLevelDocumentDiscard,
    NativeLevelDocumentErrorTitle,
    NativeLevelDocumentOk,
    NativeLevelDocumentIndex,
    NativeLevelDocumentObjectsFormat,
    NativeLevelDocumentLoadSelected,
    NativeLevelDocumentInsert,
    NativeLevelDocumentReplace,
    NativeLevelDocumentRemove,
    NativeLevelDocumentApplyObjectFields,
    NativeLevelDocumentCopy,
    NativeLevelDocumentPaste,
    NativeLevelDocumentSpriteTokensFormat,
    NativeLevelDocumentLoadRecord,
    NativeLevelDocumentInsertRecord,
    NativeLevelDocumentReplaceRecord,
    NativeLevelDocumentRemoveToken,
    NativeLevelDocumentApplySpriteFields,
    NativeLevelDocumentCopyRecord,
    NativeLevelDocumentPasteRecord,
    NativeLevelDocumentObjectCommand,
    NativeLevelDocumentObjectParameter,
    NativeLevelDocumentObjectFirstCoordinate,
    NativeLevelDocumentObjectSecondCoordinate,
    NativeLevelDocumentScreen,
    NativeLevelDocumentObjectPerpendicularHigh,
    NativeLevelDocumentSpriteNumber,
    NativeLevelDocumentSpriteX,
    NativeLevelDocumentSpriteYLow,
    NativeLevelDocumentSpriteExtraBits,
    NativeLevelDocumentSpriteMemory,
    NativeLevelDocumentSpriteBuoyancy1,
    NativeLevelDocumentSpriteInteraction,
    NativeLevelDocumentSpriteBuoyancy2,
    NativeLevelDocumentSpriteDisableLayerInteraction,
    NativeAssetsTitle,
    NativeAssetsOpenTitle,
    NativeAssetsMaximumRecordsNotice,
    NativeAssetsCancel,
    NativeAssetsOpen,
    NativeAssetsUndo,
    NativeAssetsRedo,
    NativeAssetsSaveAggregate,
    NativeAssetsModified,
    NativeAssetsSaved,
    NativeAssetsDiscardTitle,
    NativeAssetsDiscardNotice,
    NativeAssetsDiscard,
    NativeAssetsErrorTitle,
    NativeAssetsOk,
    NativeAssetsTabLevel,
    NativeAssetsTabLayer2,
    NativeAssetsTabPalette,
    NativeAssetsTabExAnimation,
    NativeAssetsTabSettings,
    NativeAssetsLevelSourceFormat,
    NativeAssetsLevelHeader,
    NativeAssetsLevelMode,
    NativeAssetsBackgroundPalette,
    NativeAssetsLastScreen,
    NativeAssetsBackgroundColor,
    NativeAssetsSpriteTileset,
    NativeAssetsDefaultMusic,
    NativeAssetsTimeLimit,
    NativeAssetsCustomTimeBypass,
    NativeAssetsEnabled,
    NativeAssetsCustomTimeHex,
    NativeAssetsForceTimeReset,
    NativeAssetsForegroundPalette,
    NativeAssetsSpritePalette,
    NativeAssetsObjectTileset,
    NativeAssetsLayer1VerticalScroll,
    NativeAssetsStageHeader,
    NativeAssetsResetHeader,
    NativeAssetsMoveUp,
    NativeAssetsMoveDown,
    NativeAssetsApplyHeader,
    NativeAssetsVerticalSpawnRange,
    NativeAssetsSmartSpawn,
    NativeAssetsApplySpawn,
    NativeAssetsSpawnUnavailable,
    NativeAssetsPaletteColorFormat,
    NativeAssetsPaletteOwnershipEditable,
    NativeAssetsPaletteOwnershipFixed,
    NativeAssetsPaletteOwnershipExAnimationFormat,
    NativeAssetsPaletteOwnershipInvalid,
    NativeAssetsPaletteCopyColor,
    NativeAssetsPalettePasteColor,
    NativeAssetsPaletteCopyRow,
    NativeAssetsPalettePasteRow,
    NativeAssetsPaletteShortcutNotice,
    NativeAssetsLayer2ObjectsFormat,
    NativeAssetsLayer2TilemapFormat,
    NativeAssetsLayer2InstalledDescriptorFormat,
    NativeAssetsLayer2LegacyDescriptor,
    NativeAssetsLayer2SelectionNotice,
    NativeAssetsLayer2SelectionFormat,
    NativeAssetsLayer2SelectionOne,
    NativeAssetsLayer2SelectionMany,
    NativeAssetsLayer2StorageIndex,
    NativeAssetsLayer2ClearSelection,
    NativeAssetsLayer2RemapTitle,
    NativeAssetsLayer2RemapNotice,
    NativeAssetsLayer2GlobalOffset,
    NativeAssetsLayer2SelectionOnly,
    NativeAssetsLayer2ApplyRemap,
    NativeAssetsLayer2RemapHelp,
    NativeAssetsLayer2TileWord,
    NativeAssetsLayer2Load,
    NativeAssetsLayer2FillSelectionFormat,
    NativeAssetsLayer2ApplyTile,
    NativeAssetsLayer2FloodCursor,
    NativeAssetsLayer2FloodHelp,
    NativeAssetsLayer2MoveSelection,
    NativeAssetsLayer2MoveHelp,
    NativeAssetsLayer2ResizeSelection,
    NativeAssetsLayer2ResizeHelp,
    NativeAssetsLayer2CapturePattern,
    NativeAssetsLayer2CapturePatternHelp,
    NativeAssetsLayer2FloodCaptured,
    NativeAssetsLayer2FloodPatternFormat,
    NativeAssetsLayer2PatternHelp,
    NativeAssetsLayer2CopySelection,
    NativeAssetsLayer2CutSelection,
    NativeAssetsLayer2PasteAnchor,
    NativeAssetsLayer2CellHelpFormat,
    NativeAssetsAnimationRecordsFormat,
    NativeAssetsAnimationKind,
    NativeAssetsAnimationTrigger,
    NativeAssetsAnimationDestination,
    NativeAssetsAnimationDestinationFlag,
    NativeAssetsAnimationSourceWords,
    NativeAssetsAnimationAppend,
    NativeAssetsAnimationReplace,
    NativeAssetsAnimationRemove,
    NativeAssetsAnimationSetting,
    NativeAssetsAnimationHeader,
    NativeAssetsAnimationApplySlots,
    NativeAssetsAnimationEnabled,
    NativeAssetsAnimationApplyTrigger,
    NativeAssetsAnimationCopyRecord,
    NativeAssetsAnimationPasteRecord,
    NativeAssetsAnimationFramePrefix,
    NativeAssetsAnimationCopyFrame,
    NativeAssetsAnimationPasteFrame,
    NativeAssetsSettingsUnavailable,
    NativeAssetsSettingsLayer3Title,
    NativeAssetsSettingsLayer3Enable,
    NativeAssetsSettingsGfxFile,
    NativeAssetsSettingsLengthSelector,
    NativeAssetsSettingsDestinationSelector,
    NativeAssetsSettingsApplyLayer3,
    NativeAssetsSettingsExpandedMode,
    NativeAssetsSettingsExpandedModeNotice,
    NativeAssetsSettingsApplyExpandedMode,
    NativeAssetsSettingsBypassTitle,
    NativeAssetsSettingsBypassEnable,
    NativeAssetsSettingsApplyBypass,
    NativeAssetsSettingsBoundaryTitle,
    NativeAssetsSettingsBoundaryAir,
    NativeAssetsSettingsBoundaryNotice,
    NativeAssetsSettingsApplyBoundary,
    NativeAssetsSettingsRawWordsNotice,
    NativeAssetsSettingsWordFormat,
    NativeAssetsSettingsApplyWords,
    NativeAssetsSettingsAnimationOptions,
    NativeAssetsSettingsAnimationUnavailable,
    NativeAssetsSettingsPaletteAnimation,
    NativeAssetsSettingsVanillaAnimation,
    NativeAssetsSettingsGlobalAnimation,
    NativeAssetsSettingsLevelAnimation,
    NativeAssetsSettingsPreservedNibbleFormat,
    NativeAssetsSettingsApplyAnimation,
    RomNativeAssetsTitle,
    RomNativeAssetsStaleNotice,
    RomNativeAssetsBusyNotice,
    RomNativeAssetsReservedModeFormat,
    RomNativeAssetsUndo,
    RomNativeAssetsRedo,
    RomNativeAssetsModified,
    RomNativeAssetsUnmodified,
    RomNativeAssetsAllocation,
    RomNativeAssetsRangeSeparator,
    RomNativeAssetsPaletteImportFull,
    RomNativeAssetsPaletteExportFull,
    RomNativeAssetsPaletteFullNotice,
    RomNativeAssetsPaletteImportRaw,
    RomNativeAssetsPaletteExportRaw,
    RomNativeAssetsPaletteImportTpl,
    RomNativeAssetsPaletteExportTpl,
    RomNativeAssetsPaletteImportRgb,
    RomNativeAssetsPaletteExportRgb,
    RomNativeAssetsPaletteNativeNotice,
    RomNativeAssetsDiscardTitle,
    RomNativeAssetsDiscardNotice,
    RomNativeAssetsCancel,
    RomNativeAssetsDiscard,
    RomNativeAssetsErrorTitle,
    RomNativeAssetsOk,
    RomNativeAssetsMwlExportComplete,
    RomNativeAssetsMwlImportComplete,
    RomNativeAssetsMwlExportLegacy,
    RomNativeAssetsMwlImportLegacy,
    RomNativeAssetsMwlExportAll,
    RomNativeAssetsMwlExportModified,
    RomNativeAssetsMwlBatchTitle,
    RomNativeAssetsMwlBatchPathFormat,
    RomNativeAssetsMwlBatchNotice,
    RomNativeAssetsMwlBatchCancelling,
    RomNativeAssetsImageExportFull,
    RomNativeAssetsImageExportPngBatch,
    RomNativeAssetsImageExportBmpBatch,
    RomNativeAssetsImageModifiedOnly,
    RomNativeAssetsImageAutoScreens,
    RomNativeAssetsImageExportedPathFormat,
    RomNativeAssetsImageBatchResultFormat,
    RomNativeAssetsImageBatchCancelled,
    RomNativeAssetsImageBatchTitle,
    RomNativeAssetsImageBatchPathFormat,
    RomNativeAssetsImageBatchModifiedSelection,
    RomNativeAssetsImageBatchAllSelection,
    RomNativeAssetsImageBatchProgressFormat,
    RomNativeAssetsImageBatchNotice,
    RomNativeAssetsValidateGfx,
    RomNativeAssetsPreviewStart,
    RomNativeAssetsPreviewStop,
    RomNativeAssetsPreviewCamera,
    RomNativeAssetsPreviewXPrefix,
    RomNativeAssetsPreviewYPrefix,
    RomNativeAssetsPreviewReset,
    RomNativeAssetsPreviewMap16Grid,
    RomNativeAssetsPreviewSelectionFormat,
    RomNativeAssetsPreviewClearSelection,
    RomNativeAssetsPreviewHoverNotice,
    RomNativeAssetsCommit,
    RomNativeAssetsCommitReclaim,
    RomNativeAssetsStaged,
    RomNativeAssetsNoStaged,
    RomNativeAssetsLayer2ResetTitle,
    RomNativeAssetsLayer2ResetChangeFormat,
    RomNativeAssetsLayer2ResetNotice,
    RomNativeAssetsLayer2ResetAction,
    RomNativeAssetsMwlBatchResultFormat,
    RomNativeAssetsMwlBatchCancelled,
    RomNativeAssetsLegacyCompatibilityFormat,
    RomNativeAssetsPreviewRendered,
    RomNativeAssetsPreviewUnresolvedFormat,
    RomNativeAssetsInspectionHeadingFormat,
    RomNativeAssetsInspectionNoMap16,
    RomNativeAssetsInspectionSpriteHeading,
    RomNativeAssetsInspectionNoSprite,
    RomOverworldOpenTitle,
    RomOverworldOpenSlot,
    RomOverworldCancel,
    RomOverworldOpen,
    RomOverworldDiscardTitle,
    RomOverworldDiscardPlayableNotice,
    RomOverworldDiscardCompleteNotice,
    RomOverworldDiscard,
    RomOverworldErrorTitle,
    RomOverworldOk,
    RomOverworldCompleteTitle,
    RomOverworldPlayableTitle,
    RomOverworldImportComplete,
    RomOverworldExportComplete,
    RomOverworldCompleteTransferNotice,
    RomOverworldImportAnimation,
    RomOverworldExportAnimation,
    RomOverworldAnimationTransferNotice,
    RomOverworldStaleNotice,
    RomOverworldPlayableMapNotice,
    RomOverworldAllocation,
    RomOverworldRangeSeparator,
    RomOverworldCommitPlayable,
    RomOverworldPlayableStaged,
    RomOverworldPlayableUnmodified,
    RomOverworldRouteBlocksTerrain,
    RomOverworldRouteTitle,
    RomOverworldRouteNotice,
    RomOverworldRouteCanvasNotice,
    RomOverworldRouteUnavailable,
    RomOverworldRouteLink,
    RomOverworldRouteSourceX,
    RomOverworldRouteSourceY,
    RomOverworldRouteSourceSubmap,
    RomOverworldRouteDestinationX,
    RomOverworldRouteDestinationY,
    RomOverworldRouteDestinationSubmap,
    RomOverworldRouteTargetX,
    RomOverworldRouteTargetY,
    RomOverworldRouteDirection,
    RomOverworldRouteOneWay,
    RomOverworldRouteOrderNotice,
    RomOverworldRouteReload,
    RomOverworldRouteApply,
    RomOverworldRouteCommit,
    RomOverworldTerrainBlocksRoute,
    RomOverworldRouteStaged,
    RomOverworldLayer2Tilemap,
    RomOverworldTileWord,
    RomOverworldApplyLayerTile,
    RomOverworldTabRecords,
    RomOverworldTabPalette,
    RomOverworldTabAnimation,
    RomOverworldTabNativeSprites,
    RomOverworldSpriteTitle,
    RomOverworldSpriteNotice,
    RomOverworldSpriteCanvasNotice,
    RomOverworldSpriteMap,
    RomOverworldSpriteIndex,
    RomOverworldSpriteId,
    RomOverworldSpriteX,
    RomOverworldSpriteY,
    RomOverworldSpriteScreen,
    RomOverworldSpriteExtension,
    RomOverworldSpriteLoad,
    RomOverworldSpriteUseCanvas,
    RomOverworldSpritePlace,
    RomOverworldSpriteRequiredFormat,
    RomOverworldSpriteFillExtension,
    RomOverworldSpriteInsert,
    RomOverworldSpriteReplace,
    RomOverworldSpriteDelete,
    RomOverworldSpriteMoveUp,
    RomOverworldSpriteMoveDown,
    RomOverworldSpriteCountFormat,
    RomOverworldSpritePropertiesTitle,
    RomOverworldSpriteRecordFormat,
    RomOverworldSpriteApply,
    RomOverworldSaveTransitionTitle,
    RomOverworldSaveTransitionNotice,
    RomOverworldSave,
    RomOverworldCommitAll,
    RomOverworldCommitReclaim,
    RomOverworldStaged,
    RomOverworldUnmodified,
    RomOverworldDirectTilePicker,
    RomOverworldPaletteRow,
    RomOverworldGraphicsPreviewUnavailable,
    RomOverworldLayer1,
    RomOverworldLayer2,
    RomOverworldMap16Tile,
    RomOverworldAnimationDestinations,
    RomOverworldAnimationDestinationNotice,
    RomOverworldAnimationCacheUnavailable,
    RomOverworldAnimationOwnerFormat,
    RomOverworldAnimationNoOwnerFormat,
    RomOverworldMap16Picker,
    RomOverworldMap16Page,
    RomOverworldMap16PreviewUnavailable,
    RomOverworldCompletedReveals,
    RomOverworldPreviewUnavailable,
    RomOverworldToolSelect,
    RomOverworldToolBrush,
    RomOverworldToolRectangle,
    RomOverworldToolFill,
    RomOverworldToolNativeSprite,
    RomOverworldToolRouteSource,
    RomOverworldToolRouteDestination,
    RomOverworldAnimationRate7_5,
    RomOverworldAnimationRate15,
    RomOverworldAnimationRate30,
    RomOverworldAnimationRate60,
    RomOverworldAnimationSubstep,
    RomOverworldAnimationSubsteps,
    RomOverworldAnimationTriggerPrefix,
    RomOverworldAnimationManualFramePrefix,
    NativePreviewPreparing,
    NativePreviewUnavailableFormat,
    ExternalToolConfigAddSnes,
    ExternalToolConfigAddGba,
    ExternalToolConfigAddTileEditor,
    ExternalToolConfigRemove,
    ExternalToolConfigEmptyNotice,
    ExternalToolConfigStableId,
    ExternalToolConfigDisplayName,
    ExternalToolConfigArgumentsNotice,
    ExternalToolConfigWorkingDirectory,
    ExternalToolConfigRunAfter,
    ExternalToolConfigRomOpened,
    ExternalToolConfigRomSaved,
    ExternalToolConfigLevelChanged,
    OverworldPaletteColorFormat,
    OverworldPaletteAnimationOwnerFormat,
    OverworldPaletteEditable,
    OverworldPaletteFixed,
    OverworldPaletteExAnimationFormat,
    OverworldPaletteInvalid,
    OverworldPaletteCopyColor,
    OverworldPalettePasteColor,
    OverworldPaletteCopyRow,
    OverworldPalettePasteRow,
    OverworldPaletteGestureNotice,
    OverworldRecordsReveals,
    OverworldRecordsEndpoints,
    OverworldRecordsMessages,
    OverworldRecordsSprites,
    OverworldRecordsNoReveals,
    OverworldRecordsReveal,
    OverworldRecordsSourceTile,
    OverworldRecordsDestinationTile,
    OverworldRecordsApplyReveal,
    OverworldRecordsMoveSelection,
    OverworldRecordsFirstPrefix,
    OverworldRecordsLastPrefix,
    OverworldRecordsXTilesPrefix,
    OverworldRecordsYTilesPrefix,
    OverworldRecordsMoveNotice,
    OverworldRecordsMoveSelected,
    OverworldRecordsNoEndpoints,
    OverworldRecordsEndpoint,
    OverworldRecordsXHex,
    OverworldRecordsYHex,
    OverworldRecordsSubmapHex,
    OverworldRecordsApplyEndpoint,
    OverworldRecordsNoMessages,
    OverworldRecordsMessage,
    OverworldRecordsColumn,
    OverworldRecordsRow,
    OverworldRecordsTileHex,
    OverworldRecordsCopyMessage,
    OverworldRecordsPasteMessage,
    OverworldRecordsApplyMessageTile,
    OverworldRecordsNoSprites,
    OverworldRecordsSprite,
    OverworldRecordsIdHex,
    OverworldRecordsUnownedExtension,
    OverworldRecordsCopySprite,
    OverworldRecordsPasteSprite,
    OverworldRecordsApplySprite,
    OverworldDocumentTitle,
    OverworldDocumentOpenTitle,
    OverworldDocumentMaximumRecords,
    OverworldDocumentOpen,
    OverworldDocumentUndo,
    OverworldDocumentRedo,
    OverworldDocumentSave,
    OverworldDocumentModified,
    OverworldDocumentSaved,
    OverworldDocumentTilemap,
    OverworldDocumentCoordinateFormat,
    OverworldDocumentMap16Tile,
    OverworldDocumentApplyTile,
    OverworldDocumentCompletedReveals,
    OverworldDocumentPreviewUnavailable,
    OverworldDocumentDiscardTitle,
    OverworldDocumentDiscardNotice,
    OverworldDocumentCancel,
    OverworldDocumentDiscard,
    OverworldDocumentErrorTitle,
    OverworldDocumentOk,
    LevelAuxScreenExits,
    LevelAuxSecondaryExits,
    LevelAuxMap16Overrides,
    LevelAuxScreenExit,
    LevelAuxEncodedValue,
    LevelAuxSecondaryExit,
    LevelAuxOverride,
    LevelAuxUpsert,
    LevelAuxRemoveSelected,
    LevelAuxAppend,
    LevelAuxReplace,
    LevelAuxRemove,
    LevelAuxDestination,
    LevelAuxPositionMethod,
    LevelAuxScreen,
    LevelAuxX,
    LevelAuxY,
    LevelAuxDestinationFlags,
    LevelAuxXOverworldFlags,
    LevelAuxAdditionalFlags,
    LevelAuxIndex,
    LevelAuxTopLeft,
    LevelAuxTopRight,
    LevelAuxBottomLeft,
    LevelAuxBottomRight,
    LevelAuxActsLike,
    LevelAdvancedExpandedHeader,
    LevelAdvancedLayer3,
    LevelAdvancedEnableLayer3,
    LevelAdvancedStartPosition,
    LevelAdvancedTilemapSize,
    LevelAdvancedLiquidType,
    LevelAdvancedFlags,
    LevelAdvancedGraphicsFormat,
    LevelAdvancedReservedBytes,
    LevelAdvancedRawTilemap,
    LevelAdvancedRemapBytes,
    LevelAdvancedApplyLayer3,
    LevelAdvancedDisableLayer3,
    LevelAdvancedCopyTilemap,
    LevelAdvancedPasteTilemap,
    LevelAdvancedCopyRemap,
    LevelAdvancedPasteRemap,
    LevelAdvancedExpandedEnabled,
    LevelAdvancedExpandedNotice,
    LevelAdvancedSuperGfx,
    LevelAdvancedUsePerLevelGfx,
    LevelAdvancedRawExpandedWords,
    LevelAdvancedFieldFormat,
    LevelCoreHeader,
    LevelCoreObjects,
    LevelCoreSprites,
    LevelCoreEntrances,
    LevelCoreExitsMap16,
    LevelCoreAdvanced,
    LevelCoreLayer1,
    LevelCoreLayer2,
    LevelCoreRecord,
    LevelCoreObjectBytes,
    LevelCoreSpriteBytes,
    LevelCoreAppend,
    LevelCoreReplace,
    LevelCoreRemove,
    LevelCoreCopy,
    LevelCorePaste,
    LevelCoreStreamHeaderFormat,
    LevelCoreEntrance,
    LevelCoreMain,
    LevelCoreMidway,
    LevelCoreSecondary,
    LevelCoreX,
    LevelCoreY,
    LevelCoreScreen,
    LevelCoreAction,
    LevelCoreRawFlags,
    LevelCoreLevelNumber,
    LevelCoreBackgroundPalette,
    LevelCoreLastScreen,
    LevelCoreLevelMode,
    LevelCoreBackgroundColor,
    LevelCoreSpriteTileset,
    LevelCoreDefaultMusicSelector,
    LevelCoreTimeLimitSelector,
    LevelCoreSpritePalette,
    LevelCoreForegroundPalette,
    LevelCoreObjectTileset,
    LevelCoreLayer1VerticalScroll,
    LevelDocumentTitle,
    LevelDocumentDimensionsTitle,
    LevelDocumentDimensionsNotice,
    LevelDocumentLayer1Width,
    LevelDocumentLayer1Height,
    LevelDocumentLayer2Width,
    LevelDocumentLayer2Height,
    LevelDocumentCancel,
    LevelDocumentOpen,
    LevelDocumentUndo,
    LevelDocumentRedo,
    LevelDocumentSave,
    LevelDocumentModified,
    LevelDocumentSaved,
    LevelDocumentLayer1,
    LevelDocumentLayer2,
    LevelDocumentPreviewUnavailable,
    LevelDocumentTilemap,
    LevelDocumentCoordinateFormat,
    LevelDocumentMap16Tile,
    LevelDocumentApplyTile,
    LevelDocumentDiscardTitle,
    LevelDocumentDiscardNotice,
    LevelDocumentDiscard,
    LevelDocumentErrorTitle,
    LevelDocumentOk,
    Map16DocumentTitle,
    Map16DocumentSave,
    Map16DocumentModified,
    Map16DocumentSaved,
    Map16DocumentPreviewUnavailable,
    Map16DocumentTileFormat,
    Map16DocumentSubtileHex,
    Map16DocumentHorizontalFlip,
    Map16DocumentVerticalFlip,
    Map16DocumentTopLeft,
    Map16DocumentTopRight,
    Map16DocumentBottomLeft,
    Map16DocumentBottomRight,
    Map16DocumentDiscardTitle,
    Map16DocumentDiscardNotice,
    Map16DocumentCancel,
    Map16DocumentDiscard,
    Map16DocumentErrorTitle,
    Map16DocumentOk,
    VanillaGraphicsHeadingFormat,
    VanillaGraphicsSplitPointers,
    VanillaGraphicsPaintColor,
    VanillaGraphicsRelocationNotice,
    VanillaGraphicsExpandRom,
    VanillaGraphicsCommit,
    VanillaGraphicsNoTiles,
    NavigationPathTitle,
    NavigationWarpTitle,
    NavigationPathNotice,
    NavigationWarpNotice,
    NavigationPathCountFormat,
    NavigationWarpCountFormat,
    NavigationStaleNotice,
    NavigationPathTableCount,
    NavigationWarpTableCount,
    NavigationResizeTable,
    NavigationLoadLink,
    NavigationApplyLink,
    NavigationCommitLinks,
    NavigationStaged,
    NavigationUnchanged,
    NavigationIndex,
    NavigationSourceX,
    NavigationSourceY,
    NavigationSourceSubmap,
    NavigationDestinationX,
    NavigationDestinationY,
    NavigationDestinationSubmap,
    NavigationTargetXTile,
    NavigationTargetYTile,
    NavigationSourcePackedVertical,
    NavigationSourceHorizontalTile,
    NavigationDestinationPackedVertical,
    NavigationDestinationHorizontalTile,
    NavigationPathDiscardTitle,
    NavigationWarpDiscardTitle,
    NavigationPathDiscardNotice,
    NavigationWarpDiscardNotice,
    NavigationCancel,
    NavigationDiscard,
    NavigationPathErrorTitle,
    NavigationWarpErrorTitle,
    NavigationOk,
    OverworldAppearancePortableTitle,
    OverworldAppearanceNativeTitle,
    OverworldAppearanceImportNative,
    OverworldAppearanceExportNative,
    OverworldAppearanceDefinitionsFormat,
    OverworldAppearanceDefinition,
    OverworldAppearanceEmptyNotice,
    OverworldAppearanceSpriteId,
    OverworldAppearanceInsertDefinition,
    OverworldAppearanceRemoveDefinition,
    OverworldAppearanceMoveToEnd,
    OverworldAppearanceMoveDefinition,
    OverworldAppearancePartsTitleFormat,
    OverworldAppearancePartsCountFormat,
    OverworldAppearancePart,
    OverworldAppearanceReplacePart,
    OverworldAppearanceRemovePart,
    OverworldAppearanceCopyPart,
    OverworldAppearancePasteOverPart,
    OverworldAppearancePasteAfterPart,
    OverworldAppearanceDuplicatePart,
    OverworldAppearanceCopyComposition,
    OverworldAppearanceReplaceComposition,
    OverworldAppearanceAppendComposition,
    OverworldAppearancePasteNewDefinition,
    OverworldAppearanceMovePart,
    OverworldAppearanceInsertPart,
    OverworldAppearancePreviewTitle,
    OverworldAppearancePreviewNotice,
    OverworldAppearanceSaveNative,
    OverworldAppearanceNativeSummaryFormat,
    OverworldAppearanceNativeSpriteId,
    OverworldAppearanceTooltip,
    OverworldAppearanceDefinitionEnabled,
    OverworldAppearanceDisablePositionText,
    OverworldAppearanceApplyTooltip,
    OverworldAppearanceExternalRanges,
    OverworldAppearanceRangesNotice,
    OverworldAppearanceGraphics,
    OverworldAppearancePalette,
    OverworldAppearanceApplyRangesFormat,
    OverworldAppearanceDisplay,
    OverworldAppearanceEditorShadow,
    OverworldAppearanceMap16Tiles,
    OverworldAppearanceTextLabel,
    OverworldAppearanceX,
    OverworldAppearanceY,
    OverworldAppearanceApplyDisplay,
    OverworldAppearanceCustomMap16,
    OverworldAppearanceNativeTile,
    OverworldAppearanceTopLeft,
    OverworldAppearanceTopRight,
    OverworldAppearanceBottomLeft,
    OverworldAppearanceBottomRight,
    OverworldAppearanceApplyMap16,
    OverworldAppearanceNativePartsFormat,
    OverworldAppearanceAddPart,
    OverworldAppearanceRemovePartNative,
    OverworldAppearanceSendBackward,
    OverworldAppearanceBringForward,
    OverworldAppearanceMap16,
    OverworldAppearanceTranslucent,
    OverworldAppearanceAddRange,
    OverworldAppearanceKind,
    OverworldAppearanceFirst,
    OverworldAppearanceLast,
    OverworldAppearanceBase,
    OverworldAppearanceRemoveRange,
    ApplicationGfxOverrideTitle,
    ApplicationGfxOverrideLayer12,
    ApplicationGfxOverrideLayer3,
    ApplicationGfxOverrideNotice,
    ApplicationGfxOverrideOk,
    ApplicationGfxOverrideCancel,
    ApplicationToolbarBack,
    ApplicationToolbarForward,
    ApplicationToolbarLevel,
    ApplicationRecentEmpty,
    ApplicationRecentClear,
    ApplicationRecentClearTitle,
    ApplicationRecentClearNotice,
    ApplicationRecentYes,
    ApplicationRecentNo,
    ApplicationIpsWarningTitle,
    ApplicationIpsWarningFormat,
    ApplicationIpsRenameNotice,
    ApplicationIpsSaveQuestion,
    ApplicationIpsSaveAnyway,
    ApplicationIpsCancel,
    ApplicationTwoBppTitle,
    ApplicationTwoBppQuestion,
    ApplicationYes,
    ApplicationNo,
    ApplicationTruncateTitle,
    ApplicationTruncateNotice,
    ExAnimationDocumentTitle,
    ExAnimationDocumentOpenTitle,
    ExAnimationDocumentMaximumRecords,
    ExAnimationDocumentOpen,
    ExAnimationDocumentUndo,
    ExAnimationDocumentRedo,
    ExAnimationDocumentSave,
    ExAnimationDocumentModified,
    ExAnimationDocumentSaved,
    ExAnimationDocumentDiscardTitle,
    ExAnimationDocumentDiscardNotice,
    ExAnimationDocumentCancel,
    ExAnimationDocumentDiscard,
    ExAnimationDocumentErrorTitle,
    ExAnimationDocumentOk,
    ExAnimationDocumentRecords,
    ExAnimationDocumentRecordListFormat,
    ExAnimationDocumentAppendRecord,
    ExAnimationDocumentRemoveSelected,
    ExAnimationDocumentSlotSettings,
    ExAnimationDocumentSettingHex,
    ExAnimationDocumentHeaderHex,
    ExAnimationDocumentTriggerValueHex,
    ExAnimationDocumentRecordFormat,
    ExAnimationDocumentKindHex,
    ExAnimationDocumentTriggerHex,
    ExAnimationDocumentDestinationHex,
    ExAnimationDocumentSourceWordsNotice,
    ExAnimationDocumentSpecialTransferNotice,
    ExAnimationDocumentApplyRecord,
    PaletteDocumentTitle,
    PaletteDocumentUndo,
    PaletteDocumentRedo,
    PaletteDocumentSave,
    PaletteDocumentModified,
    PaletteDocumentSaved,
    PaletteDocumentDiscardTitle,
    PaletteDocumentDiscardNotice,
    PaletteDocumentCancel,
    PaletteDocumentDiscard,
    PaletteDocumentErrorTitle,
    PaletteDocumentOk,
    PaletteDocumentColorFormat,
    RomPaletteTitle,
    RomPaletteStaleNotice,
    RomPaletteAllocation,
    RomPaletteRangeSeparator,
    RomPaletteCommit,
    RomPaletteCommitReclaim,
    RomPaletteStaged,
    RomPaletteUnmodified,
    RomPaletteColorFormat,
    RomPaletteShortcutNotice,
    RomPaletteMaskMode,
    RomPaletteEnableAll,
    RomPaletteDisableAll,
    RomPaletteMaskNotice,
    RomPaletteDiscardTitle,
    RomPaletteDiscardNotice,
    RomPaletteCancel,
    RomPaletteDiscard,
    RomPaletteErrorTitle,
    RomPaletteOk,
    RomPaletteImportRow,
    RomPaletteExportRow,
    RomPaletteRowTransferNotice,
    RomPaletteImportRaw,
    RomPaletteExportRaw,
    RomPaletteRawTransferNotice,
    RomExAnimationTitle,
    RomExAnimationSwitchDomain,
    RomExAnimationGlobalUnavailableFormat,
    RomExAnimationSwitchBlocked,
    RomExAnimationGlobalTarget,
    RomExAnimationLevelTargetFormat,
    RomExAnimationCommit,
    RomExAnimationStaged,
    RomExAnimationUnmodified,
    RomExAnimationAppendRecord,
    RomExAnimationSpecialTransferNotice,
    RomExAnimationReplaceRecord,
    RomExAnimationDiscardTitle,
    RomExAnimationDiscardNotice,
    RomExAnimationCancel,
    RomExAnimationDiscard,
    RomExAnimationErrorTitle,
    RomExAnimationOk,
    RomPaletteImportTpl,
    RomPaletteExportTpl,
    RomPaletteImportRgb,
    RomPaletteExportRgb,
    RomPaletteSupportedTransferNotice,
    CustomSpriteEditorTitle,
    CustomSpritePlacementsFormat,
    CustomSpritePlacement,
    CustomSpriteRecordsNotice,
    CustomSpriteDescriptionNotice,
    CustomSpriteCopyPlacement,
    CustomSpritePastePlacement,
    CustomSpriteHeaderHex,
    CustomSpriteApplyHeader,
    CustomSpriteSearch,
    CustomSpriteReplaceSelected,
    CustomSpriteRemoveSelected,
    CustomSpriteInsertAt,
    CustomSpriteMoveTo,
    CustomSpriteUtf8Bom,
    CustomSpriteCrlf,
    CustomSpriteTrailingLineEnding,
    CustomSpriteApplyFraming,
    CustomSpriteUndo,
    CustomSpriteRedo,
    CustomSpriteSavePair,
    CustomSpriteModified,
    CustomSpriteSaved,
    CustomSpriteDiscardTitle,
    CustomSpriteUnsavedNotice,
    CustomSpriteCancel,
    CustomSpriteDiscard,
    CustomSpriteErrorTitle,
    CustomSpriteOk,
    CustomObjectEditorTitle,
    CustomObjectEntriesFormat,
    CustomObjectSearch,
    CustomObjectEntry,
    CustomObjectBytesNotice,
    CustomObjectDescriptionNotice,
    CustomObjectCopy,
    CustomObjectPaste,
    CustomObjectReplaceSelected,
    CustomObjectRemoveSelected,
    CustomObjectInsertAt,
    CustomObjectMoveTo,
    CustomObjectUtf8Bom,
    CustomObjectCrlf,
    CustomObjectTrailingLineEnding,
    CustomObjectApplyFraming,
    CustomObjectUndo,
    CustomObjectRedo,
    CustomObjectSavePair,
    CustomObjectModified,
    CustomObjectSaved,
    CustomObjectDiscardTitle,
    CustomObjectUnsavedNotice,
    CustomObjectCancel,
    CustomObjectDiscard,
    CustomObjectErrorTitle,
    CustomObjectOk,
    AppearanceEditorTitle,
    AppearancePainterRecordsFormat,
    AppearanceSelected,
    AppearanceSourceLayer1,
    AppearanceSourceLayer2,
    AppearanceSourceSprite,
    AppearanceSourceIdHex,
    AppearanceTileIndexHex,
    AppearanceXOffsetDecimal,
    AppearanceYOffsetDecimal,
    AppearancePaletteRow,
    AppearanceHorizontalFlip,
    AppearanceVerticalFlip,
    AppearanceReplaceSelected,
    AppearanceRemoveSelected,
    AppearanceInsertBefore,
    AppearanceMoveBefore,
    AppearanceUndo,
    AppearanceRedo,
    AppearanceSave,
    AppearanceModified,
    AppearanceSaved,
    AppearanceDiscardTitle,
    AppearanceUnsavedNotice,
    AppearanceCancel,
    AppearanceDiscard,
    AppearanceErrorTitle,
    AppearanceOk,
    Layer3DocumentEditorTitle,
    Layer3DocumentStartPosition,
    Layer3DocumentTilemapSize,
    Layer3DocumentLiquidType,
    Layer3DocumentRawFlags,
    Layer3DocumentGraphicsFormat,
    Layer3DocumentReservedNotice,
    Layer3DocumentTilemapNotice,
    Layer3DocumentRemapNotice,
    Layer3DocumentApplyAll,
    Layer3DocumentCopyTilemap,
    Layer3DocumentPasteTilemap,
    Layer3DocumentCopyRemap,
    Layer3DocumentPasteRemap,
    Layer3DocumentUndo,
    Layer3DocumentRedo,
    Layer3DocumentSave,
    Layer3DocumentModified,
    Layer3DocumentSaved,
    Layer3DocumentDiscardTitle,
    Layer3DocumentUnsavedNotice,
    Layer3DocumentCancel,
    Layer3DocumentDiscard,
    Layer3DocumentErrorTitle,
    Layer3DocumentOk,
    MetadataEditorTitle,
    MetadataLevelNames,
    MetadataPlayerStarts,
    MetadataSubmapSettings,
    MetadataUndo,
    MetadataRedo,
    MetadataSave,
    MetadataModified,
    MetadataSaved,
    MetadataLevelNameRecord,
    MetadataLevelKeyHex,
    MetadataTileBytesHex,
    MetadataRawFlagsHex,
    MetadataPlayerStartRecord,
    MetadataPlayerKeyHex,
    MetadataXHex,
    MetadataYHex,
    MetadataSettingsRecord,
    MetadataMusicHex,
    MetadataPaletteHex,
    MetadataLayer1ScrollHex,
    MetadataLayer2ScrollHex,
    MetadataUnknownBytesHex,
    MetadataUpsertName,
    MetadataUpsertStart,
    MetadataUpsertSettings,
    MetadataRemoveSelected,
    MetadataSubmapMain,
    MetadataSubmapYoshiIsland,
    MetadataSubmapVanillaDome,
    MetadataSubmapForestIllusion,
    MetadataSubmapValleyBowser,
    MetadataSubmapSpecialWorld,
    MetadataSubmapStarWorld,
    MetadataDiscardTitle,
    MetadataUnsavedNotice,
    MetadataCancel,
    MetadataDiscard,
    MetadataErrorTitle,
    MetadataOk,
    OscEditorTitle,
    OscSourceSummaryFormat,
    OscReplaceSource,
    OscDiagnosticsHeading,
    OscParsedRecord,
    OscNoMetadataRecords,
    OscUndo,
    OscRedo,
    OscSave,
    OscModified,
    OscSaved,
    OscDiscardTitle,
    OscUnsavedNotice,
    OscCancel,
    OscDiscard,
    OscErrorTitle,
    OscOk,
    SscEditorTitle,
    SscSourceSummaryFormat,
    SscAssetsSummaryFormat,
    SscPaletteLoaded,
    SscPaletteMissing,
    SscReplaceSource,
    SscDiagnosticsHeading,
    SscParsedRecord,
    SscNoMetadataRecords,
    SscUndo,
    SscRedo,
    SscSave,
    SscModified,
    SscSaved,
    SscDiscardTitle,
    SscUnsavedNotice,
    SscCancel,
    SscDiscard,
    SscErrorTitle,
    SscOk,
    DscEditorTitle,
    DscSourceSummaryFormat,
    DscSourceNotice,
    DscReplaceSource,
    DscDiagnosticsHeading,
    DscParsedRecord,
    DscNoRecoveredRecords,
    DscUndo,
    DscRedo,
    DscSave,
    DscModified,
    DscSaved,
    DscDiscardTitle,
    DscUnsavedNotice,
    DscCancel,
    DscDiscard,
    DscErrorTitle,
    DscOk,
    TilemapTitleScreenName,
    TilemapCreditsName,
    TilemapEditorTitleFormat,
    TilemapDimensionsFormat,
    TilemapStaleNotice,
    TilemapRow,
    TilemapColumn,
    TilemapPlane,
    TilemapPrimary,
    TilemapSecondary,
    TilemapWord,
    TilemapLoadTile,
    TilemapApplyTile,
    TilemapCommit,
    TilemapStaged,
    TilemapUnchanged,
    TilemapDiscardTitleFormat,
    TilemapUnsavedNotice,
    TilemapErrorTitleFormat,
    EventNumberEditorTitle,
    EventNumberDescription,
    EventNumberStoredLengthFormat,
    EventNumberStaleNotice,
    EventNumberEvent,
    EventNumberMappedEvent,
    EventNumberLoadEntry,
    EventNumberApplyEntry,
    EventNumberCommit,
    EventNumberStaged,
    EventNumberUnchanged,
    EventNumberDiscardTitle,
    EventNumberUnsavedNotice,
    EventNumberErrorTitle,
    LevelNameEditorTitle,
    LevelNameDescription,
    LevelNameCountFormat,
    LevelNameStaleNotice,
    LevelNameLevel,
    LevelNameTile,
    LevelNameTileValue,
    LevelNameLoadTile,
    LevelNameApplyTile,
    LevelNameCommit,
    LevelNameStaged,
    LevelNameUnchanged,
    LevelNameDiscardTitle,
    LevelNameUnsavedNotice,
    LevelNameErrorTitle,
    PlayerStartEditorTitle,
    PlayerStartDescription,
    PlayerStartReservedFormat,
    PlayerStartStaleNotice,
    PlayerStartPlayer,
    PlayerStartMario,
    PlayerStartLuigi,
    PlayerStartLoad,
    PlayerStartX,
    PlayerStartY,
    PlayerStartSubmap,
    PlayerStartInvalid,
    PlayerStartMainMap,
    PlayerStartYoshisIsland,
    PlayerStartVanillaDome,
    PlayerStartForestIllusion,
    PlayerStartValleyBowser,
    PlayerStartSpecialWorld,
    PlayerStartStarWorld,
    PlayerStartApply,
    PlayerStartCommit,
    PlayerStartStaged,
    PlayerStartUnchanged,
    PlayerStartDiscardTitle,
    PlayerStartUnsavedNotice,
    PlayerStartErrorTitle,
    SpecialEventEditorTitle,
    SpecialEventDescription,
    SpecialEventStaleNotice,
    SpecialEventIndex,
    SpecialEventSourceTile,
    SpecialEventDestinationTile,
    SpecialEventDirection,
    SpecialEventLoadEntry,
    SpecialEventApplyEntry,
    SpecialEventCommit,
    SpecialEventStaged,
    SpecialEventUnchanged,
    SpecialEventDiscardTitle,
    SpecialEventUnsavedNotice,
    SpecialEventErrorTitle,
    EventRevealEditorTitle,
    EventRevealDescription,
    EventRevealCountFormat,
    EventRevealStaleNotice,
    EventRevealIndex,
    EventRevealSourceTile,
    EventRevealDestinationTile,
    EventRevealTableCount,
    EventRevealResizeTable,
    EventRevealLoad,
    EventRevealApply,
    EventRevealCommit,
    EventRevealStaged,
    EventRevealUnchanged,
    EventRevealDiscardTitle,
    EventRevealUnsavedNotice,
    EventRevealErrorTitle,
    EventTilemapEditorTitle,
    EventTilemapDescription,
    EventTilemapLoadedStorageFormat,
    EventTilemapPristineStorage,
    EventTilemapInstalledStorage,
    EventTilemapStaleNotice,
    EventTilemapTileIndex,
    EventTilemapPlane,
    EventTilemapPrimaryLow,
    EventTilemapPrimaryHigh,
    EventTilemapSecondaryHigh,
    EventTilemapByteValue,
    EventTilemapLoadByte,
    EventTilemapApplyByte,
    EventTilemapCommit,
    EventTilemapStaged,
    EventTilemapUnchanged,
    EventTilemapDiscardTitle,
    EventTilemapUnsavedNotice,
    EventTilemapErrorTitle,
    OverworldSettingsEditorTitle,
    OverworldSettingsDescription,
    OverworldSettingsInstalled,
    OverworldSettingsPristine,
    OverworldSettingsStaleNotice,
    OverworldSettingsSubmapRecord,
    OverworldSettingsLoad,
    OverworldSettingsWordFormat,
    OverworldSettingsLayer3Header,
    OverworldSettingsUseCustomTilemap,
    OverworldSettingsUseCustomGraphics,
    OverworldSettingsTilemapFile,
    OverworldSettingsTilemapSize,
    OverworldSettingsTilemapPosition,
    OverworldSettingsAddressLayoutWords,
    OverworldSettingsGraphicsFiles,
    OverworldSettingsGfxFormat,
    OverworldSettingsApplyLayer3,
    OverworldSettingsPreservationNotice,
    OverworldSettingsApplyRecord,
    OverworldSettingsCommit,
    OverworldSettingsStaged,
    OverworldSettingsUnchanged,
    OverworldSettingsDiscardTitle,
    OverworldSettingsUnsavedNotice,
    OverworldSettingsErrorTitle,
    SecondaryExitDescription,
    SecondaryExitStaleNotice,
    SecondaryExitEntry,
    SecondaryExitLoad,
    SecondaryExitPositionMethod,
    SecondaryExitDestinationFlags,
    SecondaryExitXOverworldFlags,
    SecondaryExitAdditionalFlags,
    SecondaryExitApplyEntry,
    SecondaryExitCommit,
    SecondaryExitStaged,
    SecondaryExitUnchanged,
    SecondaryExitClearAllTitle,
    SecondaryExitClearAllNotice,
    SecondaryExitClearAll,
    SecondaryExitDiscardTitle,
    SecondaryExitUnsavedNotice,
    SecondaryExitErrorTitle,
    SharedPaletteEditorTitle,
    SharedPaletteSummaryFormat,
    SharedPaletteStaleNotice,
    SharedPaletteImport,
    SharedPaletteExport,
    SharedPaletteTransferNotice,
    SharedPalettePage,
    SharedPalettePageOfFormat,
    SharedPaletteSelectedFormat,
    SharedPaletteBgr555,
    SharedPaletteDecodeRaw,
    SharedPaletteRed,
    SharedPaletteGreen,
    SharedPaletteBlue,
    SharedPalettePreview,
    SharedPaletteApplyRgb,
    SharedPaletteApplyRaw,
    SharedPaletteCopyRow,
    SharedPalettePasteRow,
    SharedPaletteCopyColor,
    SharedPalettePasteColor,
    SharedPaletteClipboardNotice,
    SharedPaletteAuxiliaryBytes,
    SharedPaletteStageAuxiliary,
    SharedPaletteCommit,
    SharedPaletteStaged,
    SharedPaletteUnchanged,
    SharedPaletteDiscardTitle,
    SharedPaletteUnsavedNotice,
    SharedPaletteErrorTitle,
    GraphicsExternalRunningTitle,
    GraphicsExternalWaitingFormat,
    GraphicsExternalReloadNotice,
    GraphicsExternalConsentTitle,
    GraphicsExternalExecutableFormat,
    GraphicsExternalStagedFileFormat,
    GraphicsExternalArgumentsNotice,
    GraphicsExternalArgumentFormat,
    GraphicsExternalRun,
    GraphicsOwnershipEditable,
    GraphicsOwnershipFixed,
    GraphicsOwnershipExAnimationFormat,
    GraphicsOwnershipOriginalAnimationFormat,
    GraphicsOwnershipLevelExAnimationFormat,
    GraphicsOwnershipGlobalExAnimationFormat,
    GraphicsOwnershipInvalid,
    GraphicsDiscardTitle,
    GraphicsUnsavedNotice,
    GraphicsErrorTitle,
    GraphicsEditorTitle,
    PortableGraphicsEditorTitle,
    PortableGraphicsDiscardTitle,
    PortableGraphicsUnsavedNotice,
    PortableGraphicsErrorTitle,
    PortableGraphicsUndo,
    PortableGraphicsRedo,
    PortableGraphicsSave,
    PortableGraphicsCopyTile,
    PortableGraphicsPasteTile,
    PortableGraphicsModified,
    PortableGraphicsSaved,
    PortableGraphicsNoTiles,
    PortableGraphicsTileFormat,
    PortableGraphicsCancel,
    PortableGraphicsDiscard,
    PortableGraphicsOk,
    GraphicsRotateClockwise,
    GraphicsFlipHorizontal,
    GraphicsFlipVertical,
    GraphicsPreviousPage,
    GraphicsNextPage,
    GraphicsPreviousPalette,
    GraphicsNextPalette,
    GraphicsColorMapFilters,
    GraphicsApplyColorMapFilter,
    GraphicsFilterFormat,
    GraphicsStaleNotice,
    GraphicsPaletteRow,
    GraphicsDefaultPalette,
    GraphicsUseJoined,
    GraphicsJoinedNotice,
    GraphicsConfiguredEditor,
    GraphicsNone,
    GraphicsEditConfigured,
    GraphicsEditExecutable,
    GraphicsInsertRaw,
    GraphicsExtractRaw,
    GraphicsExtractLevel,
    GraphicsExtractLevelNotice,
    GraphicsExtractStandard,
    GraphicsExtractSpecial,
    GraphicsSpecialNotice,
    GraphicsExtractExGfx,
    GraphicsExtractExGfxNotice,
    GraphicsExtractAllGfx,
    GraphicsInsertStandard,
    GraphicsStagedEditNotice,
    GraphicsInsertSpecial,
    GraphicsInsertExGfx,
    GraphicsInsertExGfxNotice,
    GraphicsInsertAllGfx,
    GraphicsAllocationPc,
    GraphicsAllocationRangeSeparator,
    GraphicsCommit,
    GraphicsCommitReclaim,
    GraphicsStagedChanges,
    GraphicsNoStagedChanges,
    GraphicsInternalCacheNotice,
    GraphicsSaveLevelTitle,
    GraphicsSaveLevelQuestion,
    GraphicsSaveLevelPurpose,
    GraphicsSaveLevelWarning,
    GraphicsNoTiles,
    GraphicsTileFormat,
    GraphicsInternalTileNotice,
    GraphicsCopyTile,
    GraphicsPasteTile,
    GraphicsFormatWarningTitle,
    GraphicsFormatWarningBody,
    GraphicsYes,
    GraphicsNo,
    GraphicsExtractingFormat,
    GraphicsStagingFormat,
    GraphicsBatchAtomicNotice,
    GraphicsCancellingNotice,
    GraphicsInsertingFormat,
    GraphicsReadingFormat,
    GraphicsImportAtomicNotice,
    GraphicsToolbarGfxCompleteTitle,
    GraphicsToolbarExGfxCompleteTitle,
    GraphicsToolbarGfxCompleteFormat,
    GraphicsToolbarExGfxCompleteFormat,
    GraphicsToolbarErrorTitle,
    RomMap16EditorTitle,
    RomMap16StaleNotice,
    RomMap16PreviewLevel,
    RomMap16ObjectSet,
    RomMap16FgPalette,
    RomMap16Grid,
    RomMap16GridNotice,
    RomMap16GridColor,
    RomMap16ZoomOut,
    RomMap16ZoomReset,
    RomMap16ZoomIn,
    RomMap16PageNumber,
    RomMap16PageNumberNotice,
    RomMap16LockPages,
    RomMap16UnlockPages,
    RomMap16PreviewHexError,
    RomMap16PreviewRangeError,
    RomMap16SelectionNotice,
    RomMap16Page,
    RomMap16Tile,
    RomMap16Quadrant,
    RomMap16AddressFormat,
    RomMap16CopyTile,
    RomMap16PasteTile,
    RomMap16Undo,
    RomMap16Redo,
    RomMap16Subtile,
    RomMap16Palette,
    RomMap16Priority,
    RomMap16XFlip,
    RomMap16YFlip,
    RomMap16ApplySubtile,
    RomMap16ActsLike,
    RomMap16ApplyActsLike,
    RomMap16NoActsLikeNotice,
    RomMap16ProtectedNotice,
    RomMap16UnlockTitle,
    RomMap16LockTitle,
    RomMap16UnlockWarning,
    RomMap16LockQuestion,
    RomMap16Unlock,
    RomMap16Lock,
    RomMap16AllocationPc,
    RomMap16AllocationSeparator,
    RomMap16Commit,
    RomMap16CommitReclaim,
    RomMap16Staged,
    RomMap16Unchanged,
    RomMap16DiscardTitle,
    RomMap16UnsavedNotice,
    RomMap16ErrorTitle,
    RomMap16TransferImportComplete,
    RomMap16TransferExportComplete,
    RomMap16TransferTemplateNotice,
    RomMap16TransferNativeOnlyNotice,
    RomMap16TransferSelectedWidth,
    RomMap16TransferSelectedHeight,
    RomMap16TransferFileOrigin,
    RomMap16TransferImportSelected,
    RomMap16TransferExportSelected,
    RomMap16TransferCopyRectangle,
    RomMap16TransferPasteRectangle,
    RomMap16TransferSelectedNotice,
    RomMap16TransferImportPage,
    RomMap16TransferExportPage,
    RomMap16TransferPageNotice,
    RomMap16TransferPageUnsupportedNotice,
    RomMap16TransferImportForeground,
    RomMap16TransferExportForeground,
    RomMap16TransferImportBackground,
    RomMap16TransferExportBackground,
    RomMap16TransferLegacyCompleteNotice,
    RomMap16SidecarHeading,
    RomMap16SidecarExportM16,
    RomMap16SidecarExportS16,
    RomMap16SidecarConfirmTitle,
    RomMap16SidecarConfirmQuestion,
    RomMap16SidecarNo,
    RomMap16SidecarYes,
    RomMap16SnesHeading,
    RomMap16SnesImportPalette,
    RomMap16SnesPaletteRowPrefix,
    RomMap16SnesOptimize,
    RomMap16SnesLoad,
    RomMap16SnesGraphicsOffset,
    RomMap16SnesMapOffset,
    RomMap16SnesColorFilter,
    RomMap16SnesColorMap,
    RomMap16SnesNotice,
    RomMap16SnesPreviewTitle,
    RomMap16SnesTargetPage,
    RomMap16SnesPlacement,
    RomMap16SnesGraphicsTiles,
    RomMap16SnesCandidateDefinitions,
    RomMap16SnesDefinitionsWritten,
    RomMap16SnesIndexGridSpan,
    RomMap16SnesPaletteLoaded,
    RomMap16SnesPaletteNotLoaded,
    RomMap16SnesStaleNotice,
    RomMap16SnesPreviewNotice,
    RomMap16SnesApply,
    RomMap16SnesDiscard,
    RomMap16BitmapOpeningTitle,
    RomMap16BitmapReadingClipboard,
    RomMap16BitmapTitle,
    RomMap16BitmapStaleNotice,
    RomMap16BitmapOptimize8x8,
    RomMap16BitmapReuse8x8,
    RomMap16BitmapReservedBlank,
    RomMap16BitmapOptimize16x16,
    RomMap16BitmapLayerPriority,
    RomMap16BitmapConfiguredBlank,
    RomMap16BitmapFirst8x8,
    RomMap16BitmapBlank8x8,
    RomMap16BitmapFirstMap16,
    RomMap16BitmapReservedMap16,
    RomMap16BitmapPlan,
    RomMap16BitmapAllocation,
    RomMap16BitmapExhausted,
    RomMap16BitmapImport,
    RomMap16BitmapCancel,
    RomMap16BitmapPreviewZoom,
    RomMap16BitmapResetPan,
    RomMap16BitmapOriginal,
    RomMap16BitmapConverted,
    RomMap16BitmapHeading,
    RomMap16BitmapLevelNotice,
    RomMap16BitmapGfxSlot4,
    RomMap16BitmapGfxSlot5,
    RomMap16BitmapGfxNotice,
    RomMap16BitmapChoose,
    RomMap16BitmapPaste,
    RomMap16BitmapMaximumColors,
    RomMap16BitmapPriority,
    RomMap16BitmapMedianCut,
    RomMap16BitmapPopularity,
    RomMap16BitmapAllowUnmarked,
    RomMap16BitmapPrioritizeExact,
    RomMap16BitmapPrioritizeExactNotice,
    RomMap16BitmapHueTolerance,
    RomMap16BitmapPaletteLegend,
    RomMap16BitmapUniqueColors,
    RomMap16BitmapMaintainDetail,
    RomMap16BitmapReduceMethod1,
    RomMap16BitmapReduceMethod2,
    RomExpansionTitle,
    RomExpansionTargetNotice,
    RomExpansionAlignmentNotice,
    RomExpansionLmTarget,
    RomExpansion2MiB,
    RomExpansion3MiB,
    RomExpansion4MiB,
    RomExpansionExLoRomHeading,
    RomExpansionExLoRomNotice,
    RomExpansionExLoRomConvert,
    RomExpansionExLoRomRequires,
    RomExpansionSa1Heading,
    RomExpansion6MiB,
    RomExpansion8MiB,
    RomExpansionSa1Requires,
    RomExpansionTarget,
    RomExpansionFillByte,
    RomExpansionSa1FixedNotice,
    RomExpansionCancel,
    RomExpansionApply,
    RomExpansionExLoRomWarningTitle,
    RomExpansionMapperWarning,
    RomExpansionCompatibilityWarning,
    RomExpansionUndoableNotice,
    RomExpansionConvertRom,
    RomExpansionSa1ConfirmTitle,
    RomExpansionSa1ConfirmNotice,
    RomExpansionSnes9xNotice,
    RomExpansionZsnesNotice,
    RomExpansionExpandRom,
    RomExpansionErrorTitle,
    RomExpansionOk,
    RomExpandedSettingsTitle,
    RomExpandedSettingsRecordNotice,
    RomExpandedSettingsStaleNotice,
    RomExpandedSettingsLayer3Heading,
    RomExpandedSettingsLayer3Enable,
    RomExpandedSettingsGfxFile,
    RomExpandedSettingsLengthSelector,
    RomExpandedSettingsDestinationSelector,
    RomExpandedSettingsStageLayer3,
    RomExpandedSettingsExpandedMode,
    RomExpandedSettingsExpandedModeNotice,
    RomExpandedSettingsStageExpandedMode,
    RomExpandedSettingsBypassHeading,
    RomExpandedSettingsBypassEnable,
    RomExpandedSettingsStageBypass,
    RomExpandedSettingsBoundaryHeading,
    RomExpandedSettingsBoundaryAir,
    RomExpandedSettingsStageBoundary,
    RomExpandedSettingsWordsHeading,
    RomExpandedSettingsWord,
    RomExpandedSettingsStageWords,
    RomExpandedSettingsCommit,
    RomExpandedSettingsStaged,
    RomExpandedSettingsUnchanged,
    RomExpandedSettingsDiscardTitle,
    RomExpandedSettingsUnsavedNotice,
    RomExpandedSettingsCancel,
    RomExpandedSettingsDiscard,
    RomExpandedSettingsErrorTitle,
    RomExpandedSettingsOk,
    RomExpandedSettingsGfxSlotFormat,
    ExpandedSettingsDocumentTitle,
    ExpandedSettingsRecoveredNotice,
    ExpandedSettingsApplyLayer3,
    ExpandedSettingsApplyExpandedMode,
    ExpandedSettingsApplyBypass,
    ExpandedSettingsApplyBoundary,
    ExpandedSettingsWordsNotice,
    ExpandedSettingsApplyWords,
    ExpandedSettingsUndo,
    ExpandedSettingsRedo,
    ExpandedSettingsSave,
    ExpandedSettingsModified,
    ExpandedSettingsSaved,
    ExpandedSettingsUnsavedTitle,
    ExpandedSettingsDiscardQuestion,
    ExpandedSettingsErrorTitle,
    LevelRestrictionEditingWarning,
    LevelRestrictionAcknowledge,
    LevelRestrictionRestoreTitle,
    LevelRestrictionRestoreNotice,
    LevelRestrictionRetryRestore,
    LevelRestrictionIpsTitle,
    LevelRestrictionIpsQuestion,
    LevelRestrictionYes,
    LevelRestrictionNo,
    LevelRestrictionSavingTitle,
    LevelRestrictionSavingForIps,
    LevelRestrictionSaveRequired,
    LevelRestrictionRetrySave,
    LevelRestrictionCompleteTitle,
    LevelRestrictionCompleteNotice,
    LevelRestrictionOk,
    LevelRestrictionSavingForClose,
    LevelRestrictionStillOpen,
    LevelRestrictionRetrySaveClose,
    LevelRestrictionErrorTitle,
    OverworldAnimationThisMap,
    OverworldAnimationGlobal,
    OverworldAnimationGlobalReadOnly,
    OverworldAnimationSetting,
    OverworldAnimationHeader,
    OverworldAnimationApplyGlobals,
    OverworldAnimationTrigger,
    OverworldAnimationEnabled,
    OverworldAnimationValue,
    OverworldAnimationApplyTrigger,
    OverworldAnimationRecord,
    OverworldAnimationKind,
    OverworldAnimationRecordTrigger,
    OverworldAnimationDestination,
    OverworldAnimationDestinationFlag,
    OverworldAnimationSourceWords,
    OverworldAnimationSpecialNotice,
    OverworldAnimationAppend,
    OverworldAnimationReplace,
    OverworldAnimationRemove,
    OverworldAnimationCopyRecord,
    OverworldAnimationPasteRecord,
    OverworldAnimationFramePrefix,
    OverworldAnimationCopyFrame,
    OverworldAnimationPasteFrame,
    OverworldAnimationOptionsHeading,
    OverworldAnimationMapSelector,
    OverworldAnimationOriginalPalette,
    OverworldAnimationOriginalTiles,
    OverworldAnimationGlobalFeature,
    OverworldAnimationMapFeature,
    OverworldAnimationOriginalLightning,
    OverworldAnimationOptionsUnsupported,
    OverworldAnimationRuntimeRequired,
    OverworldAnimationInstallRuntime,
    OverworldAnimationInstallRuntimeNotice,
    OverworldAnimationInstallBlocked,
    OverworldAnimationPreviewHeading,
    OverworldAnimationPlay,
    OverworldAnimationPause,
    OverworldAnimationReset,
    OverworldAnimationStepTimer,
    OverworldAnimationPhaseTick,
    OverworldAnimationTimerNotice,
    OverworldAnimationCustom,
    OverworldAnimationOneShot,
    OverworldAnimationManualFrame,
    OverworldAnimationActive,
    OverworldAnimationEventPrefix,
    OverworldAnimationPassed,
    OverworldAnimationEventManualNotice,
    OverworldAnimationNoRecordsNotice,
    TitleRecordingTitle,
    TitleRecordingDescription,
    TitleRecordingNoPlayback,
    TitleRecordingStaleNotice,
    TitleRecordingBytesPresent,
    TitleRecordingEnterPayload,
    TitleRecordingMinimalPayload,
    TitleRecordingNormalizeHex,
    TitleRecordingCommit,
    TitleRecordingModified,
    TitleRecordingUnchanged,
    TitleRecordingRecorderHeading,
    TitleRecordingRecorderAbsentNotice,
    TitleRecordingInstallRecorder,
    TitleRecordingRecorderInstalledNotice,
    TitleRecordingUninstallRecorder,
    TitleRecordingFilesHeading,
    TitleRecordingImportNative,
    TitleRecordingImportZsnes,
    TitleRecordingImportSnes9x,
    TitleRecordingExportNative,
    TitleRecordingExportZsnes,
    TitleRecordingTransferNotice,
    TitleRecordingDiscardTitle,
    TitleRecordingUnsavedNotice,
    TitleRecordingCancel,
    TitleRecordingDiscard,
    TitleRecordingErrorTitle,
    TitleRecordingOk,
    OverworldMessageTitle,
    OverworldMessageDescription,
    OverworldMessageStorageStatus,
    OverworldMessageStaleNotice,
    OverworldMessageTableCount,
    OverworldMessageResize,
    OverworldMessageIndex,
    OverworldMessageColumn,
    OverworldMessageTileValue,
    OverworldMessageDiscardTitle,
    OverworldMessageUnsavedNotice,
    OverworldMessageErrorTitle,
    BossMessageTitle,
    BossMessageDescription,
    BossMessageStaleNotice,
    BossMessageIndex,
    BossMessageColumn,
    BossMessageTileValue,
    BossMessageDiscardTitle,
    BossMessageUnsavedNotice,
    BossMessageErrorTitle,
    MessageEditorRow,
    MessageEditorLoadTile,
    MessageEditorApplyTile,
    MessageEditorCommit,
    MessageEditorStaged,
    MessageEditorUnchanged,
    MessageEditorCancel,
    MessageEditorDiscard,
    MessageEditorOk,
    RomMetadataTitle,
    RomMetadataDescription,
    RomMetadataSummary,
    RomMetadataStaleNotice,
    RomMetadataRegion,
    RomMetadataAttribution,
    RomMetadataAttributionRange,
    RomMetadataVramVersion,
    RomMetadataVramVersionRange,
    RomMetadataFeatureRecord,
    RomMetadataFeatureRecordRange,
    RomMetadataByteIndex,
    RomMetadataByteValue,
    RomMetadataLoadByte,
    RomMetadataApplyByte,
    RomMetadataCommit,
    RomMetadataStaged,
    RomMetadataUnchanged,
    RomMetadataDiscardTitle,
    RomMetadataUnsavedNotice,
    RomMetadataCancel,
    RomMetadataDiscard,
    RomMetadataErrorTitle,
    RomMetadataOk,
    LegacyBypassFgBgTitle,
    LegacyBypassSpriteTitle,
    LegacyBypassDescription,
    LegacyBypassEnable,
    LegacyBypassListRow,
    LegacyBypassRegularRow,
    LegacyBypassRegularNotice,
    LegacyBypassZeroFallback,
    LegacyBypassStaleNotice,
    LegacyBypassStage,
    LegacyBypassCommit,
    LegacyBypassStaged,
    LegacyBypassUnchanged,
    LegacyBypassDiscardTitle,
    LegacyBypassUnsavedNotice,
    LegacyBypassCancel,
    LegacyBypassDiscard,
    LegacyBypassErrorTitle,
    LegacyBypassOk,
    CopierHeaderTitle,
    CopierHeaderLogicalRomFormat,
    CopierHeaderCurrentStateFormat,
    CopierHeaderTarget,
    CopierHeaderAbsent,
    CopierHeaderPresent,
    CopierHeaderFillByte,
    CopierHeaderPreservationNotice,
    CopierHeaderUseCanonical,
    CopierHeaderCancel,
    CopierHeaderConvert,
    CopierHeaderErrorTitle,
    CopierHeaderOk,
    IpsApplyTitle,
    IpsApplyHeaderNotice,
    IpsApplySummaryFormat,
    IpsApplyIdentityNotice,
    IpsApplyStaleNotice,
    IpsApplyCancel,
    IpsApplyAction,
    IpsApplyErrorTitle,
    IpsApplyOk,
    IpsCreateOriginalPrompt,
    IpsCreateModifiedPrompt,
    IpsCreateTitle,
    IpsCreateOriginalFormat,
    IpsCreateModifiedFormat,
    IpsCreateOutputFormat,
    IpsCreateProgress,
    IpsCreateCompletedTitle,
    IpsCreateCompletedFormat,
    IpsCreateErrorTitle,
    IpsCreateOk,
    RatsReclaimTitle,
    RatsReclaimOwnershipNotice,
    RatsReclaimSummaryFormat,
    RatsReclaimFillByte,
    RatsReclaimTransactionNotice,
    RatsReclaimStaleNotice,
    RatsReclaimCancel,
    RatsReclaimAction,
    RatsReclaimErrorTitle,
    RatsReclaimOk,
    RevisionPatchTitle,
    RevisionPatchIdentityFormat,
    RevisionPatchPayloadSummaryFormat,
    RevisionPatchRangeNotice,
    RevisionPatchSearchStart,
    RevisionPatchSearchEnd,
    RevisionPatchExpansionFill,
    RevisionPatchAtomicNotice,
    RevisionPatchStaleNotice,
    RevisionPatchCancel,
    RevisionPatchInstall,
    RevisionPatchErrorTitle,
    RevisionPatchOk,
    BuiltInRuntimeTitle,
    BuiltInRuntimeTarget,
    BuiltInRuntimeFamily,
    BuiltInRuntimeExpandedSettings,
    BuiltInRuntimeCompleteLayer3,
    BuiltInRuntimeLfix3,
    BuiltInRuntimeMap16,
    BuiltInRuntimeExAnimation,
    BuiltInRuntimeLayer2,
    BuiltInRuntimeSprite19,
    BuiltInRuntimeSupportPatchB,
    BuiltInRuntimeLz2Speed,
    BuiltInRuntimeSharedPalettes,
    BuiltInRuntimeExpandedSettingsDescription,
    BuiltInRuntimeCompleteLayer3Description,
    BuiltInRuntimeLfix3Description,
    BuiltInRuntimeMap16Description,
    BuiltInRuntimeExAnimationDescription,
    BuiltInRuntimeLayer2Description,
    BuiltInRuntimeSprite19Description,
    BuiltInRuntimeSupportPatchBDescription,
    BuiltInRuntimeLz2SpeedDescription,
    BuiltInRuntimeSharedPalettesDescription,
    BuiltInRuntimeAlreadyInstalled,
    BuiltInRuntimeAtomicNotice,
    BuiltInRuntimeStaleNotice,
    BuiltInRuntimeCancel,
    BuiltInRuntimeMigrate,
    BuiltInRuntimeInstall,
    BuiltInRuntimeErrorTitle,
    BuiltInRuntimeOk,
    BuiltInRuntimeMigrateLfix3Gen1,
    BuiltInRuntimeMigrateLfix3Gen2,
    BuiltInRuntimeMigrateMap16Stage1,
    BuiltInRuntimeMigrateMap16Stage2,
    BuiltInRuntimeMigrateMap16Stage3,
    BuiltInRuntimeMigrateExAnimationPointers,
    BuiltInRuntimeMigrateExAnimationTable,
    BuiltInRuntimeMigrateLayer2Format100,
    BuiltInRuntimeMigrateLayer2Format101,
    BuiltInRuntimeMigrateLayer2Format102,
    RomLoaderMissingHeaderTitle,
    RomLoaderMissingHeaderQuestion,
    RomLoaderAddHeader,
    RomLoaderCancel,
    RomLoaderOpeningTitle,
    RomLoaderOpeningProgress,
    MwlImportTitle,
    MwlImportReadingFormat,
    MwlImportReadingSidecarsFormat,
    MwlImportMissingPalette,
    MwlImportCommittingFormat,
    MwlImportCommittingNotesFormat,
    MwlImportClose,
    MwlImportInsertedFormat,
    MwlImportFailedFormat,
    MwlBatchImportTitle,
    MwlBatchImportDirectoryFormat,
    MwlBatchImportSummaryFormat,
    MwlBatchImportAllocationSearch,
    MwlBatchImportRangeSeparator,
    MwlBatchImportStart,
    MwlBatchImportCancelNotice,
    MwlBatchImportCancel,
    MwlBatchImportClose,
    MwlBatchImportCancelled,
    MwlBatchImportCompleteFormat,
    MwlBatchImportReadingFormat,
    MwlBatchImportCommittingFormat,
    MwlBatchImportPreparedFormat,
    MwlBatchImportInsertedFormat,
    MwlBatchImportReadFailedFormat,
    MwlBatchImportInsertFailedFormat,
    MwlBatchImportCommitFailedFormat,
    MwlBatchImportDiscardedRead,
    MwlBatchExportProgressTitle,
    MwlBatchExportTemplateFormat,
    MwlBatchExportAtomicNotice,
    MwlBatchExportCancellationRequested,
    MwlBatchExportCancel,
    MwlBatchExportResultTitle,
    MwlBatchExportCompletedFormat,
    MwlBatchExportCancelled,
    MwlBatchExportClose,
    VramPatchTitle,
    VramPatchDescription,
    VramPatchDeferredNotice,
    VramPatchType,
    VramPatchNone,
    VramPatchNoneHelp,
    VramPatchNormal,
    VramPatchNormalHelp,
    VramPatchHd16x9,
    VramPatchHd21x9,
    VramPatchUnknownNotice,
    VramPatchCancel,
    VramPatchOk,
    VramPatchErrorTitle,
    VramPatchStatusNone,
    VramPatchStatusNormal,
    VramPatchStatusHd,
    LegacyBypassTransferCompleteTitle,
    LegacyBypassTransferCompleteFormat,
    LegacyBypassTransferDestinationFallback,
    LegacyBypassTransferErrorTitle,
    LegacyBypassTransferOk,
    VanillaLevelZoomTitle,
    VanillaLevelZoomIn,
    VanillaLevelZoomOut,
    VanillaLevelZoomFilter,
    VanillaLevelConditionalMap16Title,
    VanillaLevelConditionalMap16RuntimeFlag,
    VanillaLevelConditionalMap16AlwaysShow,
    VanillaLevelConditionalMap16RemoveFlag,
    VanillaLevelApply,
    VanillaLevelCancel,
    VanillaLevelDirectMap16RemapTitle,
    VanillaLevelHexSourceDestinationPairs,
    VanillaLevelDirectMap16RemapHelp,
    VanillaLevelBackgroundMap16BankTitle,
    VanillaLevelBackgroundMap16BankHelp,
    VanillaLevelBank,
    VanillaLevelOk,
    VanillaLevelBackgroundTileRemapTitle,
    VanillaLevelBackgroundTileOffset,
    VanillaLevelBackgroundTileRemapHelp,
    VanillaLevelPropertiesTitle,
    VanillaLevelManualEditTitle,
    VanillaLevelLayer1ObjectFormat,
    VanillaLevelLayer2ObjectFormat,
    VanillaLevelSpriteRecordFormat,
    VanillaLevelApplyProperties,
    VanillaLevelSelectEntityForProperties,
    VanillaLevelManualSingleSelection,
    VanillaLevelSpriteTokenFormat,
    VanillaLevelApplyCompleteRecord,
    VanillaLevelSelectEntityForManualEdit,
    VanillaLevelAddStructures,
    VanillaLevelHexFilter,
    VanillaLevelHexNameFilter,
    VanillaLevelClear,
    VanillaLevelChooseStandardObject,
    VanillaLevelHandlerMapUnavailable,
    VanillaLevelStandardDefinitionsUnavailable,
    VanillaLevelSwitchPreviewsUnavailable,
    VanillaLevelStandardObject,
    VanillaLevelAddCustomOscObject,
    VanillaLevelCustomObject,
    VanillaLevelAddExtendedObjects,
    VanillaLevelChooseExtendedObject,
    VanillaLevelExtendedDefinitionsUnavailable,
    VanillaLevelExtendedObject,
    VanillaLevelInsertAfterSelection,
    VanillaLevelApplyScreenJump,
    VanillaLevelApplyScreenExit,
    VanillaLevelApplyObjectFields,
    VanillaLevelApplyRawRecord,
    VanillaLevelRemoveObject,
    VanillaLevelMoveUp,
    VanillaLevelMoveDown,
    VanillaLevelCopy,
    VanillaLevelPasteAfterSelection,
    VanillaLevelPasteMap16Rectangle,
    VanillaLevelExistingSpritesFormat,
    VanillaLevelChooseExistingSprite,
    VanillaLevelChooseExistingSpritePlaceholder,
    VanillaLevelPlacementActive,
    VanillaLevelRawSpriteStream,
    VanillaLevelSpritesStored,
    VanillaLevelAddStandardSprites,
    VanillaLevelChooseStandardSprite,
    VanillaLevelStandardSprite,
    VanillaLevelAddCustomSprites,
    VanillaLevelStageSpriteHeader,
    VanillaLevelReplaceRecord,
    VanillaLevelApplySpriteFields,
    VanillaLevelRemoveSprite,
    VanillaLevelCopyRecord,
    VanillaLevelPasteRecordAfterSelection,
    VanillaLevelPlaceOnCanvas,
    VanillaLevelApplyFields,
    VanillaLevelHeaderCountsFormat,
    VanillaLevelMode,
    VanillaLevelBackgroundPalette,
    VanillaLevelLastScreen,
    VanillaLevelBackgroundColor,
    VanillaLevelSpriteTileset,
    VanillaLevelDefaultMusic,
    VanillaLevelCustomMusicBypass,
    VanillaLevelEnabled,
    VanillaLevelCustomMusicTrack,
    VanillaLevelTimeLimit,
    VanillaLevelCustomTimeBypass,
    VanillaLevelCustomTime,
    VanillaLevelForceTimeReset,
    VanillaLevelForegroundPalette,
    VanillaLevelSpritePalette,
    VanillaLevelObjectTileset,
    VanillaLevelLayer1VerticalScroll,
    VanillaLevelStageHeader,
    VanillaLevelResetStagedValues,
    VanillaLevelResetLayer2Title,
    VanillaLevelResetLayer2Format,
    VanillaLevelResetLayer2Help,
    VanillaLevelResetLayer2Apply,
    VanillaLevelMainEntrance,
    VanillaLevelEntranceExactRecord,
    VanillaLevelPosition,
    VanillaLevelLayer2ScrollPreset,
    VanillaLevelVerticalSettings,
    VanillaLevelScreenMethod,
    VanillaLevelModeScreen,
    VanillaLevelMidwayInstalled,
    VanillaLevelFlags,
    VanillaLevelAdditionalFlags,
    VanillaLevelHighPosition,
    VanillaLevelMidwayNotInstalled,
    VanillaLevelInstallMidway,
    VanillaLevelStageEntrance,
    VanillaLevelResetEntrance,
    VanillaLevelCommitEntrances,
    VanillaLevelCurrentUnavailable,
    VanillaLevelExitTableHelp,
    VanillaLevelScreen,
    VanillaLevelPresent,
    VanillaLevelDestinationFlags,
    VanillaLevelApplyAllExits,
    VanillaLevelResetExits,
    VanillaLevelInvalidExitScreens,
    VanillaLevelInvalidExitSaveHelp,
    VanillaLevelDisableWarningFormat,
    VanillaLevelSaveAnywayQuestion,
    VanillaLevelSaveAnyway,
    VanillaLevelScanExitsTitle,
    VanillaLevelNoInvalidExits,
    VanillaLevelInvalidExitFixHelp,
    VanillaLevelLayer2,
    VanillaLevelLayer2TilemapStatusFormat,
    VanillaLevelMap16Word,
    VanillaLevelStageSelectedTile,
    VanillaLevelLayer2PaintHelp,
    VanillaLevelSharedBackgroundReadOnly,
    VanillaLevelLayer2ObjectCountFormat,
    VanillaLevelBackgroundCanvas,
    VanillaLevelCanvasPlaceHelp,
    VanillaLevelCanvasSelectHelp,
    VanillaLevelDuplicateSelected,
    VanillaLevelDeleteSelected,
    VanillaLevelGamePixels,
    VanillaLevelViewport,
    VanillaLevelSelectionOverGame,
    VanillaLevelSelectionOverGameHelp,
    VanillaLevelCanvasTool,
    VanillaLevelSelectMove,
    VanillaLevelPlaceObject,
    VanillaLevelPlaceSprite,
    VanillaLevelPaintLayer2Tile,
    VanillaLevelPlaceLayer2Object,
    VanillaLevelZoom,
    VanillaLevelReset,
    VanillaLevelCamera,
    VanillaLevelScreenMinus,
    VanillaLevelScreenPlus,
    VanillaLevelEntrance,
    VanillaLevelObjectPlacementWarningFormat,
    VanillaLevelSpriteCountFormat,
    VanillaLevelSpriteCountWarning,
    VanillaLevelVerticalFireballWarning,
    VanillaLevelSaveTitle,
    VanillaLevelSaveBeforeContinuing,
    VanillaLevelSave,
    VanillaLevelDiscard,
    VanillaLevelSaveBeforeExitFormat,
    VanillaLevelObjectFormat,
    VanillaLevelNoSelectedObject,
    VanillaLevelNativeScreenExit,
    VanillaLevelSourceScreen,
    VanillaLevelScreenExitEncodingHelp,
    VanillaLevelScreenJumpFormat,
    VanillaLevelLowByteFirst,
    VanillaLevelHighByteFirst,
    VanillaLevelFirstEncodedComponent,
    VanillaLevelSecondEncodedComponent,
    VanillaLevelAdvanceScreen,
    VanillaLevelPreviewZoomOut,
    VanillaLevelPreviewZoomIn,
    VanillaLevelPreviewZoomDefault,
    VanillaLevelSpriteMemory,
    VanillaLevelSpriteBuoyancy1,
    VanillaLevelWaterLavaInteraction,
    VanillaLevelSpriteBuoyancy2,
    VanillaLevelWaterLavaDisableLayerInteraction,
    VanillaLevelRecordBytes,
    VanillaLevelSpriteNumber,
    VanillaLevelX,
    VanillaLevelYLowBits,
    VanillaLevelExtraBits,
}

impl ExtendedUiTextKey {
    pub const ALL: [Self; 2372] = [
        Self::MwlDocumentEditorTitle,
        Self::MwlDocumentVersionFormat,
        Self::MwlDocumentFlagsHex,
        Self::MwlDocumentAttributionNotice,
        Self::MwlDocumentLevelNumberNotice,
        Self::MwlDocumentApplyHeader,
        Self::MwlDocumentLayer3Heading,
        Self::MwlDocumentLayer3Unavailable,
        Self::MwlDocumentLayer3Enable,
        Self::MwlDocumentLayer3File,
        Self::MwlDocumentLengthSelector,
        Self::MwlDocumentDestinationSelector,
        Self::MwlDocumentExpandedMode,
        Self::MwlDocumentApplyLayer3,
        Self::MwlDocumentEntranceHeading,
        Self::MwlDocumentEntranceNotice,
        Self::MwlDocumentMainPosition,
        Self::MwlDocumentMainVertical,
        Self::MwlDocumentMainScreenMethod,
        Self::MwlDocumentMainModeScreen,
        Self::MwlDocumentMainFlags,
        Self::MwlDocumentMainHighPosition,
        Self::MwlDocumentMainAdditionalFlags,
        Self::MwlDocumentMidwayPosition,
        Self::MwlDocumentMidwayFlags,
        Self::MwlDocumentMidwayHighPosition,
        Self::MwlDocumentMidwayAdditionalFlags,
        Self::MwlDocumentSeparateLayer2Scroll,
        Self::MwlDocumentOriginalScrollPreset,
        Self::MwlDocumentHorizontalSelector,
        Self::MwlDocumentVerticalSelector,
        Self::MwlDocumentSpriteSpawning,
        Self::MwlDocumentVerticalSpawnRange,
        Self::MwlDocumentSmartSpawn,
        Self::MwlDocumentSectionLevelHeader,
        Self::MwlDocumentSectionLayer1,
        Self::MwlDocumentSectionLayer2,
        Self::MwlDocumentSectionSprites,
        Self::MwlDocumentSectionPalette,
        Self::MwlDocumentSectionSecondaryExits,
        Self::MwlDocumentSectionExAnimation,
        Self::MwlDocumentSectionExpandedHeader,
        Self::MwlDocumentSectionLengthFormat,
        Self::MwlDocumentSectionBytes,
        Self::MwlDocumentReplaceSection,
        Self::MwlDocumentUndo,
        Self::MwlDocumentRedo,
        Self::MwlDocumentSave,
        Self::MwlDocumentModified,
        Self::MwlDocumentSaved,
        Self::MwlDocumentDiscardTitle,
        Self::MwlDocumentUnsavedNotice,
        Self::MwlDocumentCancel,
        Self::MwlDocumentDiscard,
        Self::MwlDocumentErrorTitle,
        Self::MwlDocumentOk,
        Self::MwlObjectHeading,
        Self::MwlObjectCountFormat,
        Self::MwlObjectHeader,
        Self::MwlObjectStageHeader,
        Self::MwlObjectRecord,
        Self::MwlObjectCommit,
        Self::MwlObjectRecoveredFields,
        Self::MwlObjectCommandId,
        Self::MwlObjectParameter,
        Self::MwlObjectFirstCoordinate,
        Self::MwlObjectSecondCoordinate,
        Self::MwlObjectAdvancesScreen,
        Self::MwlObjectStageFields,
        Self::MwlObjectJumpEncodingFormat,
        Self::MwlObjectResolvedScreenFormat,
        Self::MwlObjectOutsideScreenSuffix,
        Self::MwlObjectJumpTarget,
        Self::MwlObjectStageJumpTarget,
        Self::MwlInsertBefore,
        Self::MwlReplace,
        Self::MwlDelete,
        Self::MwlMoveUp,
        Self::MwlMoveDown,
        Self::MwlSpriteHeading,
        Self::MwlSpriteExpanded,
        Self::MwlSpriteTokenCountFormat,
        Self::MwlSpriteStageHeader,
        Self::MwlSpriteRecordBytes,
        Self::MwlSpriteUpperYToken,
        Self::MwlSpriteControlToken,
        Self::MwlSpriteCommit,
        Self::MwlSpriteLengthNotice,
        Self::MwlSpriteSetLength,
        Self::MwlSpriteResetLengths,
        Self::MwlSpriteRecoveredFields,
        Self::MwlSpriteYLow,
        Self::MwlSpriteExtraBits,
        Self::MwlSpriteScreen,
        Self::MwlSpriteX,
        Self::MwlSpriteNumber,
        Self::MwlSpriteStageFields,
        Self::MwlOptionalImportHeading,
        Self::MwlOptionalMaximumRecords,
        Self::MwlOptionalImport,
        Self::MwlOptionalInterpret,
        Self::MwlOptionalImportNotice,
        Self::MwlOptionalHeading,
        Self::MwlOptionalPalette,
        Self::MwlOptionalExAnimation,
        Self::MwlOptionalPaletteMetadata,
        Self::MwlOptionalExAnimationMetadata,
        Self::MwlOptionalColorFormat,
        Self::MwlOptionalFeaturesHeading,
        Self::MwlOptionalPaletteAnimation,
        Self::MwlOptionalVanillaAnimation,
        Self::MwlOptionalGlobalAnimation,
        Self::MwlOptionalLevelAnimation,
        Self::MwlOptionalApplyFeatures,
        Self::MwlOptionalPreservedNibbleFormat,
        Self::MwlOptionalCreateAnimation,
        Self::MwlOptionalSetting,
        Self::MwlOptionalHeader,
        Self::MwlOptionalApplyGlobals,
        Self::MwlOptionalTrigger,
        Self::MwlOptionalTriggerEnabled,
        Self::MwlOptionalApplyTrigger,
        Self::MwlOptionalKind,
        Self::MwlOptionalDestination,
        Self::MwlOptionalDestinationFlag,
        Self::MwlOptionalSourceWords,
        Self::MwlOptionalAppendRecord,
        Self::MwlOptionalReplaceRecord,
        Self::MwlOptionalRemoveRecord,
        Self::MwlOptionalFrameHeading,
        Self::MwlOptionalSourceWordList,
        Self::MwlOptionalMoveBefore,
        Self::MwlOptionalInsertFrame,
        Self::MwlOptionalReplaceFrame,
        Self::MwlOptionalRemoveFrame,
        Self::MwlOptionalMoveFrame,
        Self::MwlOptionalWord0,
        Self::MwlOptionalWord1,
        Self::MwlOptionalApplyMetadata,
        Self::Map16SidecarEditorTitle,
        Self::Map16SidecarInterpretTitle,
        Self::Map16SidecarM16Kind,
        Self::Map16SidecarS16Kind,
        Self::Map16SidecarCancel,
        Self::Map16SidecarOpen,
        Self::Map16SidecarM16Exact,
        Self::Map16SidecarS16Canonical,
        Self::Map16SidecarSummaryFormat,
        Self::Map16SidecarRawEntry,
        Self::Map16SidecarRawDword,
        Self::Map16SidecarApplyRaw,
        Self::Map16SidecarDefinitionFormat,
        Self::Map16SidecarQuadrant,
        Self::Map16SidecarTile,
        Self::Map16SidecarPalette,
        Self::Map16SidecarPriority,
        Self::Map16SidecarHorizontalFlip,
        Self::Map16SidecarVerticalFlip,
        Self::Map16SidecarApplySubtile,
        Self::Map16SidecarUndo,
        Self::Map16SidecarRedo,
        Self::Map16SidecarSave,
        Self::Map16SidecarModified,
        Self::Map16SidecarSaved,
        Self::Map16SidecarDiscardTitle,
        Self::Map16SidecarDiscardNotice,
        Self::Map16SidecarDiscard,
        Self::Map16SidecarErrorTitle,
        Self::Map16SidecarOk,
        Self::ToolbarEditorTitle,
        Self::ToolbarEditorNotice,
        Self::ToolbarEditorDefaultNotice,
        Self::ToolbarEditorMoveUp,
        Self::ToolbarEditorMoveDown,
        Self::ToolbarEditorRemove,
        Self::ToolbarEditorAddButton,
        Self::ToolbarEditorAddSeparator,
        Self::ToolbarEditorApply,
        Self::ToolbarEditorUseDefault,
        Self::ToolbarEditorCancel,
        Self::ToolbarEditorSeparator,
        Self::RestoreAutomaticTitle,
        Self::RestoreInterval,
        Self::RestoreDaily,
        Self::RestoreDestructive,
        Self::RestoreContinuityNotice,
        Self::RestoreAppend,
        Self::RestoreCancel,
        Self::RestoreAutomaticComplete,
        Self::RestoreArchiveFormat,
        Self::RestoreOriginalFormat,
        Self::RestoreTargetFormat,
        Self::RestoreId,
        Self::RestoreDateTime,
        Self::RestoreType,
        Self::RestoreDescription,
        Self::RestoreAm,
        Self::RestorePm,
        Self::RestoreReversion,
        Self::RestoreFull,
        Self::RestoreDelta,
        Self::RestoreReplaceWarning,
        Self::RestoreRunningTitle,
        Self::RestorePointFormat,
        Self::RestoreRunningTargetFormat,
        Self::RestoreRunningNotice,
        Self::RestoreCompleteTitle,
        Self::RestoreErrorTitle,
        Self::RestoreOk,
        Self::RestoreAssociatedOne,
        Self::RestoreAssociatedManyFormat,
        Self::RestoreCompleteFormat,
        Self::LevelUsageOutputFormat,
        Self::LevelUsageProgressTitle,
        Self::LevelUsageLevelsFormat,
        Self::LevelUsageScanningFormat,
        Self::LevelUsageCancel,
        Self::LevelUsageCompleteFormat,
        Self::LevelUsageCompleteTitle,
        Self::LevelUsageErrorTitle,
        Self::LevelUsageOk,
        Self::GraphicsMigrationAllocationNotice,
        Self::GraphicsMigrationStart,
        Self::GraphicsMigrationEnd,
        Self::GraphicsMigrationErrorTitle,
        Self::GraphicsMigrationOk,
        Self::ShortcutEditorTitle,
        Self::ShortcutEditorGestureNotice,
        Self::ShortcutEditorPrimaryNotice,
        Self::ShortcutEditorRemove,
        Self::ShortcutEditorAdd,
        Self::ShortcutEditorApply,
        Self::ShortcutEditorClearAll,
        Self::ShortcutEditorCancel,
        Self::PathEditorTitle,
        Self::PathEditorPolicyTitle,
        Self::PathEditorReciprocalPolicy,
        Self::PathEditorCancel,
        Self::PathEditorOpen,
        Self::PathEditorNodes,
        Self::PathEditorEdges,
        Self::PathEditorUndo,
        Self::PathEditorRedo,
        Self::PathEditorSave,
        Self::PathEditorModified,
        Self::PathEditorSaved,
        Self::PathEditorNode,
        Self::PathEditorEdge,
        Self::PathEditorUpsertNode,
        Self::PathEditorUpsertEdge,
        Self::PathEditorRemoveSelected,
        Self::PathEditorStableId,
        Self::PathEditorX,
        Self::PathEditorY,
        Self::PathEditorLevel,
        Self::PathEditorRawFlags,
        Self::PathEditorFromNode,
        Self::PathEditorToNode,
        Self::PathEditorExit,
        Self::PathEditorOneWay,
        Self::PathEditorReciprocalPair,
        Self::PathEditorReverseExit,
        Self::PathEditorReverseRawFlags,
        Self::PathEditorDiscardTitle,
        Self::PathEditorDiscardNotice,
        Self::PathEditorDiscard,
        Self::PathEditorErrorTitle,
        Self::PathEditorOk,
        Self::PathEditorDirectionUp,
        Self::PathEditorDirectionRight,
        Self::PathEditorDirectionDown,
        Self::PathEditorDirectionLeft,
        Self::ExternalToolRunningTitleFormat,
        Self::ExternalToolWaitingFormat,
        Self::ExternalToolStop,
        Self::ExternalToolAllowTitle,
        Self::ExternalToolIdFormat,
        Self::ExternalToolExecutableFormat,
        Self::ExternalToolWorkingDirectoryFormat,
        Self::ExternalToolInherited,
        Self::ExternalToolArgumentsNotice,
        Self::ExternalToolArgumentFormat,
        Self::ExternalToolDeny,
        Self::ExternalToolRun,
        Self::ExternalToolCompletedFormat,
        Self::ExternalToolStoppedFormat,
        Self::NativeLevelDocumentTitle,
        Self::NativeLevelDocumentSourceFormat,
        Self::NativeLevelDocumentExpandedFraming,
        Self::NativeLevelDocumentLegacyFraming,
        Self::NativeLevelDocumentLegacyHeaderFormat,
        Self::NativeLevelDocumentUndo,
        Self::NativeLevelDocumentRedo,
        Self::NativeLevelDocumentSave,
        Self::NativeLevelDocumentApplySpriteHeader,
        Self::NativeLevelDocumentModified,
        Self::NativeLevelDocumentSaved,
        Self::NativeLevelDocumentDiscardTitle,
        Self::NativeLevelDocumentDiscardNotice,
        Self::NativeLevelDocumentCancel,
        Self::NativeLevelDocumentDiscard,
        Self::NativeLevelDocumentErrorTitle,
        Self::NativeLevelDocumentOk,
        Self::NativeLevelDocumentIndex,
        Self::NativeLevelDocumentObjectsFormat,
        Self::NativeLevelDocumentLoadSelected,
        Self::NativeLevelDocumentInsert,
        Self::NativeLevelDocumentReplace,
        Self::NativeLevelDocumentRemove,
        Self::NativeLevelDocumentApplyObjectFields,
        Self::NativeLevelDocumentCopy,
        Self::NativeLevelDocumentPaste,
        Self::NativeLevelDocumentSpriteTokensFormat,
        Self::NativeLevelDocumentLoadRecord,
        Self::NativeLevelDocumentInsertRecord,
        Self::NativeLevelDocumentReplaceRecord,
        Self::NativeLevelDocumentRemoveToken,
        Self::NativeLevelDocumentApplySpriteFields,
        Self::NativeLevelDocumentCopyRecord,
        Self::NativeLevelDocumentPasteRecord,
        Self::NativeLevelDocumentObjectCommand,
        Self::NativeLevelDocumentObjectParameter,
        Self::NativeLevelDocumentObjectFirstCoordinate,
        Self::NativeLevelDocumentObjectSecondCoordinate,
        Self::NativeLevelDocumentScreen,
        Self::NativeLevelDocumentObjectPerpendicularHigh,
        Self::NativeLevelDocumentSpriteNumber,
        Self::NativeLevelDocumentSpriteX,
        Self::NativeLevelDocumentSpriteYLow,
        Self::NativeLevelDocumentSpriteExtraBits,
        Self::NativeLevelDocumentSpriteMemory,
        Self::NativeLevelDocumentSpriteBuoyancy1,
        Self::NativeLevelDocumentSpriteInteraction,
        Self::NativeLevelDocumentSpriteBuoyancy2,
        Self::NativeLevelDocumentSpriteDisableLayerInteraction,
        Self::NativeAssetsTitle,
        Self::NativeAssetsOpenTitle,
        Self::NativeAssetsMaximumRecordsNotice,
        Self::NativeAssetsCancel,
        Self::NativeAssetsOpen,
        Self::NativeAssetsUndo,
        Self::NativeAssetsRedo,
        Self::NativeAssetsSaveAggregate,
        Self::NativeAssetsModified,
        Self::NativeAssetsSaved,
        Self::NativeAssetsDiscardTitle,
        Self::NativeAssetsDiscardNotice,
        Self::NativeAssetsDiscard,
        Self::NativeAssetsErrorTitle,
        Self::NativeAssetsOk,
        Self::NativeAssetsTabLevel,
        Self::NativeAssetsTabLayer2,
        Self::NativeAssetsTabPalette,
        Self::NativeAssetsTabExAnimation,
        Self::NativeAssetsTabSettings,
        Self::NativeAssetsLevelSourceFormat,
        Self::NativeAssetsLevelHeader,
        Self::NativeAssetsLevelMode,
        Self::NativeAssetsBackgroundPalette,
        Self::NativeAssetsLastScreen,
        Self::NativeAssetsBackgroundColor,
        Self::NativeAssetsSpriteTileset,
        Self::NativeAssetsDefaultMusic,
        Self::NativeAssetsTimeLimit,
        Self::NativeAssetsCustomTimeBypass,
        Self::NativeAssetsEnabled,
        Self::NativeAssetsCustomTimeHex,
        Self::NativeAssetsForceTimeReset,
        Self::NativeAssetsForegroundPalette,
        Self::NativeAssetsSpritePalette,
        Self::NativeAssetsObjectTileset,
        Self::NativeAssetsLayer1VerticalScroll,
        Self::NativeAssetsStageHeader,
        Self::NativeAssetsResetHeader,
        Self::NativeAssetsMoveUp,
        Self::NativeAssetsMoveDown,
        Self::NativeAssetsApplyHeader,
        Self::NativeAssetsVerticalSpawnRange,
        Self::NativeAssetsSmartSpawn,
        Self::NativeAssetsApplySpawn,
        Self::NativeAssetsSpawnUnavailable,
        Self::NativeAssetsPaletteColorFormat,
        Self::NativeAssetsPaletteOwnershipEditable,
        Self::NativeAssetsPaletteOwnershipFixed,
        Self::NativeAssetsPaletteOwnershipExAnimationFormat,
        Self::NativeAssetsPaletteOwnershipInvalid,
        Self::NativeAssetsPaletteCopyColor,
        Self::NativeAssetsPalettePasteColor,
        Self::NativeAssetsPaletteCopyRow,
        Self::NativeAssetsPalettePasteRow,
        Self::NativeAssetsPaletteShortcutNotice,
        Self::NativeAssetsLayer2ObjectsFormat,
        Self::NativeAssetsLayer2TilemapFormat,
        Self::NativeAssetsLayer2InstalledDescriptorFormat,
        Self::NativeAssetsLayer2LegacyDescriptor,
        Self::NativeAssetsLayer2SelectionNotice,
        Self::NativeAssetsLayer2SelectionFormat,
        Self::NativeAssetsLayer2SelectionOne,
        Self::NativeAssetsLayer2SelectionMany,
        Self::NativeAssetsLayer2StorageIndex,
        Self::NativeAssetsLayer2ClearSelection,
        Self::NativeAssetsLayer2RemapTitle,
        Self::NativeAssetsLayer2RemapNotice,
        Self::NativeAssetsLayer2GlobalOffset,
        Self::NativeAssetsLayer2SelectionOnly,
        Self::NativeAssetsLayer2ApplyRemap,
        Self::NativeAssetsLayer2RemapHelp,
        Self::NativeAssetsLayer2TileWord,
        Self::NativeAssetsLayer2Load,
        Self::NativeAssetsLayer2FillSelectionFormat,
        Self::NativeAssetsLayer2ApplyTile,
        Self::NativeAssetsLayer2FloodCursor,
        Self::NativeAssetsLayer2FloodHelp,
        Self::NativeAssetsLayer2MoveSelection,
        Self::NativeAssetsLayer2MoveHelp,
        Self::NativeAssetsLayer2ResizeSelection,
        Self::NativeAssetsLayer2ResizeHelp,
        Self::NativeAssetsLayer2CapturePattern,
        Self::NativeAssetsLayer2CapturePatternHelp,
        Self::NativeAssetsLayer2FloodCaptured,
        Self::NativeAssetsLayer2FloodPatternFormat,
        Self::NativeAssetsLayer2PatternHelp,
        Self::NativeAssetsLayer2CopySelection,
        Self::NativeAssetsLayer2CutSelection,
        Self::NativeAssetsLayer2PasteAnchor,
        Self::NativeAssetsLayer2CellHelpFormat,
        Self::NativeAssetsAnimationRecordsFormat,
        Self::NativeAssetsAnimationKind,
        Self::NativeAssetsAnimationTrigger,
        Self::NativeAssetsAnimationDestination,
        Self::NativeAssetsAnimationDestinationFlag,
        Self::NativeAssetsAnimationSourceWords,
        Self::NativeAssetsAnimationAppend,
        Self::NativeAssetsAnimationReplace,
        Self::NativeAssetsAnimationRemove,
        Self::NativeAssetsAnimationSetting,
        Self::NativeAssetsAnimationHeader,
        Self::NativeAssetsAnimationApplySlots,
        Self::NativeAssetsAnimationEnabled,
        Self::NativeAssetsAnimationApplyTrigger,
        Self::NativeAssetsAnimationCopyRecord,
        Self::NativeAssetsAnimationPasteRecord,
        Self::NativeAssetsAnimationFramePrefix,
        Self::NativeAssetsAnimationCopyFrame,
        Self::NativeAssetsAnimationPasteFrame,
        Self::NativeAssetsSettingsUnavailable,
        Self::NativeAssetsSettingsLayer3Title,
        Self::NativeAssetsSettingsLayer3Enable,
        Self::NativeAssetsSettingsGfxFile,
        Self::NativeAssetsSettingsLengthSelector,
        Self::NativeAssetsSettingsDestinationSelector,
        Self::NativeAssetsSettingsApplyLayer3,
        Self::NativeAssetsSettingsExpandedMode,
        Self::NativeAssetsSettingsExpandedModeNotice,
        Self::NativeAssetsSettingsApplyExpandedMode,
        Self::NativeAssetsSettingsBypassTitle,
        Self::NativeAssetsSettingsBypassEnable,
        Self::NativeAssetsSettingsApplyBypass,
        Self::NativeAssetsSettingsBoundaryTitle,
        Self::NativeAssetsSettingsBoundaryAir,
        Self::NativeAssetsSettingsBoundaryNotice,
        Self::NativeAssetsSettingsApplyBoundary,
        Self::NativeAssetsSettingsRawWordsNotice,
        Self::NativeAssetsSettingsWordFormat,
        Self::NativeAssetsSettingsApplyWords,
        Self::NativeAssetsSettingsAnimationOptions,
        Self::NativeAssetsSettingsAnimationUnavailable,
        Self::NativeAssetsSettingsPaletteAnimation,
        Self::NativeAssetsSettingsVanillaAnimation,
        Self::NativeAssetsSettingsGlobalAnimation,
        Self::NativeAssetsSettingsLevelAnimation,
        Self::NativeAssetsSettingsPreservedNibbleFormat,
        Self::NativeAssetsSettingsApplyAnimation,
        Self::RomNativeAssetsTitle,
        Self::RomNativeAssetsStaleNotice,
        Self::RomNativeAssetsBusyNotice,
        Self::RomNativeAssetsReservedModeFormat,
        Self::RomNativeAssetsUndo,
        Self::RomNativeAssetsRedo,
        Self::RomNativeAssetsModified,
        Self::RomNativeAssetsUnmodified,
        Self::RomNativeAssetsAllocation,
        Self::RomNativeAssetsRangeSeparator,
        Self::RomNativeAssetsPaletteImportFull,
        Self::RomNativeAssetsPaletteExportFull,
        Self::RomNativeAssetsPaletteFullNotice,
        Self::RomNativeAssetsPaletteImportRaw,
        Self::RomNativeAssetsPaletteExportRaw,
        Self::RomNativeAssetsPaletteImportTpl,
        Self::RomNativeAssetsPaletteExportTpl,
        Self::RomNativeAssetsPaletteImportRgb,
        Self::RomNativeAssetsPaletteExportRgb,
        Self::RomNativeAssetsPaletteNativeNotice,
        Self::RomNativeAssetsDiscardTitle,
        Self::RomNativeAssetsDiscardNotice,
        Self::RomNativeAssetsCancel,
        Self::RomNativeAssetsDiscard,
        Self::RomNativeAssetsErrorTitle,
        Self::RomNativeAssetsOk,
        Self::RomNativeAssetsMwlExportComplete,
        Self::RomNativeAssetsMwlImportComplete,
        Self::RomNativeAssetsMwlExportLegacy,
        Self::RomNativeAssetsMwlImportLegacy,
        Self::RomNativeAssetsMwlExportAll,
        Self::RomNativeAssetsMwlExportModified,
        Self::RomNativeAssetsMwlBatchTitle,
        Self::RomNativeAssetsMwlBatchPathFormat,
        Self::RomNativeAssetsMwlBatchNotice,
        Self::RomNativeAssetsMwlBatchCancelling,
        Self::RomNativeAssetsImageExportFull,
        Self::RomNativeAssetsImageExportPngBatch,
        Self::RomNativeAssetsImageExportBmpBatch,
        Self::RomNativeAssetsImageModifiedOnly,
        Self::RomNativeAssetsImageAutoScreens,
        Self::RomNativeAssetsImageExportedPathFormat,
        Self::RomNativeAssetsImageBatchResultFormat,
        Self::RomNativeAssetsImageBatchCancelled,
        Self::RomNativeAssetsImageBatchTitle,
        Self::RomNativeAssetsImageBatchPathFormat,
        Self::RomNativeAssetsImageBatchModifiedSelection,
        Self::RomNativeAssetsImageBatchAllSelection,
        Self::RomNativeAssetsImageBatchProgressFormat,
        Self::RomNativeAssetsImageBatchNotice,
        Self::RomNativeAssetsValidateGfx,
        Self::RomNativeAssetsPreviewStart,
        Self::RomNativeAssetsPreviewStop,
        Self::RomNativeAssetsPreviewCamera,
        Self::RomNativeAssetsPreviewXPrefix,
        Self::RomNativeAssetsPreviewYPrefix,
        Self::RomNativeAssetsPreviewReset,
        Self::RomNativeAssetsPreviewMap16Grid,
        Self::RomNativeAssetsPreviewSelectionFormat,
        Self::RomNativeAssetsPreviewClearSelection,
        Self::RomNativeAssetsPreviewHoverNotice,
        Self::RomNativeAssetsCommit,
        Self::RomNativeAssetsCommitReclaim,
        Self::RomNativeAssetsStaged,
        Self::RomNativeAssetsNoStaged,
        Self::RomNativeAssetsLayer2ResetTitle,
        Self::RomNativeAssetsLayer2ResetChangeFormat,
        Self::RomNativeAssetsLayer2ResetNotice,
        Self::RomNativeAssetsLayer2ResetAction,
        Self::RomNativeAssetsMwlBatchResultFormat,
        Self::RomNativeAssetsMwlBatchCancelled,
        Self::RomNativeAssetsLegacyCompatibilityFormat,
        Self::RomNativeAssetsPreviewRendered,
        Self::RomNativeAssetsPreviewUnresolvedFormat,
        Self::RomNativeAssetsInspectionHeadingFormat,
        Self::RomNativeAssetsInspectionNoMap16,
        Self::RomNativeAssetsInspectionSpriteHeading,
        Self::RomNativeAssetsInspectionNoSprite,
        Self::RomOverworldOpenTitle,
        Self::RomOverworldOpenSlot,
        Self::RomOverworldCancel,
        Self::RomOverworldOpen,
        Self::RomOverworldDiscardTitle,
        Self::RomOverworldDiscardPlayableNotice,
        Self::RomOverworldDiscardCompleteNotice,
        Self::RomOverworldDiscard,
        Self::RomOverworldErrorTitle,
        Self::RomOverworldOk,
        Self::RomOverworldCompleteTitle,
        Self::RomOverworldPlayableTitle,
        Self::RomOverworldImportComplete,
        Self::RomOverworldExportComplete,
        Self::RomOverworldCompleteTransferNotice,
        Self::RomOverworldImportAnimation,
        Self::RomOverworldExportAnimation,
        Self::RomOverworldAnimationTransferNotice,
        Self::RomOverworldStaleNotice,
        Self::RomOverworldPlayableMapNotice,
        Self::RomOverworldAllocation,
        Self::RomOverworldRangeSeparator,
        Self::RomOverworldCommitPlayable,
        Self::RomOverworldPlayableStaged,
        Self::RomOverworldPlayableUnmodified,
        Self::RomOverworldRouteBlocksTerrain,
        Self::RomOverworldRouteTitle,
        Self::RomOverworldRouteNotice,
        Self::RomOverworldRouteCanvasNotice,
        Self::RomOverworldRouteUnavailable,
        Self::RomOverworldRouteLink,
        Self::RomOverworldRouteSourceX,
        Self::RomOverworldRouteSourceY,
        Self::RomOverworldRouteSourceSubmap,
        Self::RomOverworldRouteDestinationX,
        Self::RomOverworldRouteDestinationY,
        Self::RomOverworldRouteDestinationSubmap,
        Self::RomOverworldRouteTargetX,
        Self::RomOverworldRouteTargetY,
        Self::RomOverworldRouteDirection,
        Self::RomOverworldRouteOneWay,
        Self::RomOverworldRouteOrderNotice,
        Self::RomOverworldRouteReload,
        Self::RomOverworldRouteApply,
        Self::RomOverworldRouteCommit,
        Self::RomOverworldTerrainBlocksRoute,
        Self::RomOverworldRouteStaged,
        Self::RomOverworldLayer2Tilemap,
        Self::RomOverworldTileWord,
        Self::RomOverworldApplyLayerTile,
        Self::RomOverworldTabRecords,
        Self::RomOverworldTabPalette,
        Self::RomOverworldTabAnimation,
        Self::RomOverworldTabNativeSprites,
        Self::RomOverworldSpriteTitle,
        Self::RomOverworldSpriteNotice,
        Self::RomOverworldSpriteCanvasNotice,
        Self::RomOverworldSpriteMap,
        Self::RomOverworldSpriteIndex,
        Self::RomOverworldSpriteId,
        Self::RomOverworldSpriteX,
        Self::RomOverworldSpriteY,
        Self::RomOverworldSpriteScreen,
        Self::RomOverworldSpriteExtension,
        Self::RomOverworldSpriteLoad,
        Self::RomOverworldSpriteUseCanvas,
        Self::RomOverworldSpritePlace,
        Self::RomOverworldSpriteRequiredFormat,
        Self::RomOverworldSpriteFillExtension,
        Self::RomOverworldSpriteInsert,
        Self::RomOverworldSpriteReplace,
        Self::RomOverworldSpriteDelete,
        Self::RomOverworldSpriteMoveUp,
        Self::RomOverworldSpriteMoveDown,
        Self::RomOverworldSpriteCountFormat,
        Self::RomOverworldSpritePropertiesTitle,
        Self::RomOverworldSpriteRecordFormat,
        Self::RomOverworldSpriteApply,
        Self::RomOverworldSaveTransitionTitle,
        Self::RomOverworldSaveTransitionNotice,
        Self::RomOverworldSave,
        Self::RomOverworldCommitAll,
        Self::RomOverworldCommitReclaim,
        Self::RomOverworldStaged,
        Self::RomOverworldUnmodified,
        Self::RomOverworldDirectTilePicker,
        Self::RomOverworldPaletteRow,
        Self::RomOverworldGraphicsPreviewUnavailable,
        Self::RomOverworldLayer1,
        Self::RomOverworldLayer2,
        Self::RomOverworldMap16Tile,
        Self::RomOverworldAnimationDestinations,
        Self::RomOverworldAnimationDestinationNotice,
        Self::RomOverworldAnimationCacheUnavailable,
        Self::RomOverworldAnimationOwnerFormat,
        Self::RomOverworldAnimationNoOwnerFormat,
        Self::RomOverworldMap16Picker,
        Self::RomOverworldMap16Page,
        Self::RomOverworldMap16PreviewUnavailable,
        Self::RomOverworldCompletedReveals,
        Self::RomOverworldPreviewUnavailable,
        Self::RomOverworldToolSelect,
        Self::RomOverworldToolBrush,
        Self::RomOverworldToolRectangle,
        Self::RomOverworldToolFill,
        Self::RomOverworldToolNativeSprite,
        Self::RomOverworldToolRouteSource,
        Self::RomOverworldToolRouteDestination,
        Self::RomOverworldAnimationRate7_5,
        Self::RomOverworldAnimationRate15,
        Self::RomOverworldAnimationRate30,
        Self::RomOverworldAnimationRate60,
        Self::RomOverworldAnimationSubstep,
        Self::RomOverworldAnimationSubsteps,
        Self::RomOverworldAnimationTriggerPrefix,
        Self::RomOverworldAnimationManualFramePrefix,
        Self::NativePreviewPreparing,
        Self::NativePreviewUnavailableFormat,
        Self::ExternalToolConfigAddSnes,
        Self::ExternalToolConfigAddGba,
        Self::ExternalToolConfigAddTileEditor,
        Self::ExternalToolConfigRemove,
        Self::ExternalToolConfigEmptyNotice,
        Self::ExternalToolConfigStableId,
        Self::ExternalToolConfigDisplayName,
        Self::ExternalToolConfigArgumentsNotice,
        Self::ExternalToolConfigWorkingDirectory,
        Self::ExternalToolConfigRunAfter,
        Self::ExternalToolConfigRomOpened,
        Self::ExternalToolConfigRomSaved,
        Self::ExternalToolConfigLevelChanged,
        Self::OverworldPaletteColorFormat,
        Self::OverworldPaletteAnimationOwnerFormat,
        Self::OverworldPaletteEditable,
        Self::OverworldPaletteFixed,
        Self::OverworldPaletteExAnimationFormat,
        Self::OverworldPaletteInvalid,
        Self::OverworldPaletteCopyColor,
        Self::OverworldPalettePasteColor,
        Self::OverworldPaletteCopyRow,
        Self::OverworldPalettePasteRow,
        Self::OverworldPaletteGestureNotice,
        Self::OverworldRecordsReveals,
        Self::OverworldRecordsEndpoints,
        Self::OverworldRecordsMessages,
        Self::OverworldRecordsSprites,
        Self::OverworldRecordsNoReveals,
        Self::OverworldRecordsReveal,
        Self::OverworldRecordsSourceTile,
        Self::OverworldRecordsDestinationTile,
        Self::OverworldRecordsApplyReveal,
        Self::OverworldRecordsMoveSelection,
        Self::OverworldRecordsFirstPrefix,
        Self::OverworldRecordsLastPrefix,
        Self::OverworldRecordsXTilesPrefix,
        Self::OverworldRecordsYTilesPrefix,
        Self::OverworldRecordsMoveNotice,
        Self::OverworldRecordsMoveSelected,
        Self::OverworldRecordsNoEndpoints,
        Self::OverworldRecordsEndpoint,
        Self::OverworldRecordsXHex,
        Self::OverworldRecordsYHex,
        Self::OverworldRecordsSubmapHex,
        Self::OverworldRecordsApplyEndpoint,
        Self::OverworldRecordsNoMessages,
        Self::OverworldRecordsMessage,
        Self::OverworldRecordsColumn,
        Self::OverworldRecordsRow,
        Self::OverworldRecordsTileHex,
        Self::OverworldRecordsCopyMessage,
        Self::OverworldRecordsPasteMessage,
        Self::OverworldRecordsApplyMessageTile,
        Self::OverworldRecordsNoSprites,
        Self::OverworldRecordsSprite,
        Self::OverworldRecordsIdHex,
        Self::OverworldRecordsUnownedExtension,
        Self::OverworldRecordsCopySprite,
        Self::OverworldRecordsPasteSprite,
        Self::OverworldRecordsApplySprite,
        Self::OverworldDocumentTitle,
        Self::OverworldDocumentOpenTitle,
        Self::OverworldDocumentMaximumRecords,
        Self::OverworldDocumentOpen,
        Self::OverworldDocumentUndo,
        Self::OverworldDocumentRedo,
        Self::OverworldDocumentSave,
        Self::OverworldDocumentModified,
        Self::OverworldDocumentSaved,
        Self::OverworldDocumentTilemap,
        Self::OverworldDocumentCoordinateFormat,
        Self::OverworldDocumentMap16Tile,
        Self::OverworldDocumentApplyTile,
        Self::OverworldDocumentCompletedReveals,
        Self::OverworldDocumentPreviewUnavailable,
        Self::OverworldDocumentDiscardTitle,
        Self::OverworldDocumentDiscardNotice,
        Self::OverworldDocumentCancel,
        Self::OverworldDocumentDiscard,
        Self::OverworldDocumentErrorTitle,
        Self::OverworldDocumentOk,
        Self::LevelAuxScreenExits,
        Self::LevelAuxSecondaryExits,
        Self::LevelAuxMap16Overrides,
        Self::LevelAuxScreenExit,
        Self::LevelAuxEncodedValue,
        Self::LevelAuxSecondaryExit,
        Self::LevelAuxOverride,
        Self::LevelAuxUpsert,
        Self::LevelAuxRemoveSelected,
        Self::LevelAuxAppend,
        Self::LevelAuxReplace,
        Self::LevelAuxRemove,
        Self::LevelAuxDestination,
        Self::LevelAuxPositionMethod,
        Self::LevelAuxScreen,
        Self::LevelAuxX,
        Self::LevelAuxY,
        Self::LevelAuxDestinationFlags,
        Self::LevelAuxXOverworldFlags,
        Self::LevelAuxAdditionalFlags,
        Self::LevelAuxIndex,
        Self::LevelAuxTopLeft,
        Self::LevelAuxTopRight,
        Self::LevelAuxBottomLeft,
        Self::LevelAuxBottomRight,
        Self::LevelAuxActsLike,
        Self::LevelAdvancedExpandedHeader,
        Self::LevelAdvancedLayer3,
        Self::LevelAdvancedEnableLayer3,
        Self::LevelAdvancedStartPosition,
        Self::LevelAdvancedTilemapSize,
        Self::LevelAdvancedLiquidType,
        Self::LevelAdvancedFlags,
        Self::LevelAdvancedGraphicsFormat,
        Self::LevelAdvancedReservedBytes,
        Self::LevelAdvancedRawTilemap,
        Self::LevelAdvancedRemapBytes,
        Self::LevelAdvancedApplyLayer3,
        Self::LevelAdvancedDisableLayer3,
        Self::LevelAdvancedCopyTilemap,
        Self::LevelAdvancedPasteTilemap,
        Self::LevelAdvancedCopyRemap,
        Self::LevelAdvancedPasteRemap,
        Self::LevelAdvancedExpandedEnabled,
        Self::LevelAdvancedExpandedNotice,
        Self::LevelAdvancedSuperGfx,
        Self::LevelAdvancedUsePerLevelGfx,
        Self::LevelAdvancedRawExpandedWords,
        Self::LevelAdvancedFieldFormat,
        Self::LevelCoreHeader,
        Self::LevelCoreObjects,
        Self::LevelCoreSprites,
        Self::LevelCoreEntrances,
        Self::LevelCoreExitsMap16,
        Self::LevelCoreAdvanced,
        Self::LevelCoreLayer1,
        Self::LevelCoreLayer2,
        Self::LevelCoreRecord,
        Self::LevelCoreObjectBytes,
        Self::LevelCoreSpriteBytes,
        Self::LevelCoreAppend,
        Self::LevelCoreReplace,
        Self::LevelCoreRemove,
        Self::LevelCoreCopy,
        Self::LevelCorePaste,
        Self::LevelCoreStreamHeaderFormat,
        Self::LevelCoreEntrance,
        Self::LevelCoreMain,
        Self::LevelCoreMidway,
        Self::LevelCoreSecondary,
        Self::LevelCoreX,
        Self::LevelCoreY,
        Self::LevelCoreScreen,
        Self::LevelCoreAction,
        Self::LevelCoreRawFlags,
        Self::LevelCoreLevelNumber,
        Self::LevelCoreBackgroundPalette,
        Self::LevelCoreLastScreen,
        Self::LevelCoreLevelMode,
        Self::LevelCoreBackgroundColor,
        Self::LevelCoreSpriteTileset,
        Self::LevelCoreDefaultMusicSelector,
        Self::LevelCoreTimeLimitSelector,
        Self::LevelCoreSpritePalette,
        Self::LevelCoreForegroundPalette,
        Self::LevelCoreObjectTileset,
        Self::LevelCoreLayer1VerticalScroll,
        Self::LevelDocumentTitle,
        Self::LevelDocumentDimensionsTitle,
        Self::LevelDocumentDimensionsNotice,
        Self::LevelDocumentLayer1Width,
        Self::LevelDocumentLayer1Height,
        Self::LevelDocumentLayer2Width,
        Self::LevelDocumentLayer2Height,
        Self::LevelDocumentCancel,
        Self::LevelDocumentOpen,
        Self::LevelDocumentUndo,
        Self::LevelDocumentRedo,
        Self::LevelDocumentSave,
        Self::LevelDocumentModified,
        Self::LevelDocumentSaved,
        Self::LevelDocumentLayer1,
        Self::LevelDocumentLayer2,
        Self::LevelDocumentPreviewUnavailable,
        Self::LevelDocumentTilemap,
        Self::LevelDocumentCoordinateFormat,
        Self::LevelDocumentMap16Tile,
        Self::LevelDocumentApplyTile,
        Self::LevelDocumentDiscardTitle,
        Self::LevelDocumentDiscardNotice,
        Self::LevelDocumentDiscard,
        Self::LevelDocumentErrorTitle,
        Self::LevelDocumentOk,
        Self::Map16DocumentTitle,
        Self::Map16DocumentSave,
        Self::Map16DocumentModified,
        Self::Map16DocumentSaved,
        Self::Map16DocumentPreviewUnavailable,
        Self::Map16DocumentTileFormat,
        Self::Map16DocumentSubtileHex,
        Self::Map16DocumentHorizontalFlip,
        Self::Map16DocumentVerticalFlip,
        Self::Map16DocumentTopLeft,
        Self::Map16DocumentTopRight,
        Self::Map16DocumentBottomLeft,
        Self::Map16DocumentBottomRight,
        Self::Map16DocumentDiscardTitle,
        Self::Map16DocumentDiscardNotice,
        Self::Map16DocumentCancel,
        Self::Map16DocumentDiscard,
        Self::Map16DocumentErrorTitle,
        Self::Map16DocumentOk,
        Self::VanillaGraphicsHeadingFormat,
        Self::VanillaGraphicsSplitPointers,
        Self::VanillaGraphicsPaintColor,
        Self::VanillaGraphicsRelocationNotice,
        Self::VanillaGraphicsExpandRom,
        Self::VanillaGraphicsCommit,
        Self::VanillaGraphicsNoTiles,
        Self::NavigationPathTitle,
        Self::NavigationWarpTitle,
        Self::NavigationPathNotice,
        Self::NavigationWarpNotice,
        Self::NavigationPathCountFormat,
        Self::NavigationWarpCountFormat,
        Self::NavigationStaleNotice,
        Self::NavigationPathTableCount,
        Self::NavigationWarpTableCount,
        Self::NavigationResizeTable,
        Self::NavigationLoadLink,
        Self::NavigationApplyLink,
        Self::NavigationCommitLinks,
        Self::NavigationStaged,
        Self::NavigationUnchanged,
        Self::NavigationIndex,
        Self::NavigationSourceX,
        Self::NavigationSourceY,
        Self::NavigationSourceSubmap,
        Self::NavigationDestinationX,
        Self::NavigationDestinationY,
        Self::NavigationDestinationSubmap,
        Self::NavigationTargetXTile,
        Self::NavigationTargetYTile,
        Self::NavigationSourcePackedVertical,
        Self::NavigationSourceHorizontalTile,
        Self::NavigationDestinationPackedVertical,
        Self::NavigationDestinationHorizontalTile,
        Self::NavigationPathDiscardTitle,
        Self::NavigationWarpDiscardTitle,
        Self::NavigationPathDiscardNotice,
        Self::NavigationWarpDiscardNotice,
        Self::NavigationCancel,
        Self::NavigationDiscard,
        Self::NavigationPathErrorTitle,
        Self::NavigationWarpErrorTitle,
        Self::NavigationOk,
        Self::OverworldAppearancePortableTitle,
        Self::OverworldAppearanceNativeTitle,
        Self::OverworldAppearanceImportNative,
        Self::OverworldAppearanceExportNative,
        Self::OverworldAppearanceDefinitionsFormat,
        Self::OverworldAppearanceDefinition,
        Self::OverworldAppearanceEmptyNotice,
        Self::OverworldAppearanceSpriteId,
        Self::OverworldAppearanceInsertDefinition,
        Self::OverworldAppearanceRemoveDefinition,
        Self::OverworldAppearanceMoveToEnd,
        Self::OverworldAppearanceMoveDefinition,
        Self::OverworldAppearancePartsTitleFormat,
        Self::OverworldAppearancePartsCountFormat,
        Self::OverworldAppearancePart,
        Self::OverworldAppearanceReplacePart,
        Self::OverworldAppearanceRemovePart,
        Self::OverworldAppearanceCopyPart,
        Self::OverworldAppearancePasteOverPart,
        Self::OverworldAppearancePasteAfterPart,
        Self::OverworldAppearanceDuplicatePart,
        Self::OverworldAppearanceCopyComposition,
        Self::OverworldAppearanceReplaceComposition,
        Self::OverworldAppearanceAppendComposition,
        Self::OverworldAppearancePasteNewDefinition,
        Self::OverworldAppearanceMovePart,
        Self::OverworldAppearanceInsertPart,
        Self::OverworldAppearancePreviewTitle,
        Self::OverworldAppearancePreviewNotice,
        Self::OverworldAppearanceSaveNative,
        Self::OverworldAppearanceNativeSummaryFormat,
        Self::OverworldAppearanceNativeSpriteId,
        Self::OverworldAppearanceTooltip,
        Self::OverworldAppearanceDefinitionEnabled,
        Self::OverworldAppearanceDisablePositionText,
        Self::OverworldAppearanceApplyTooltip,
        Self::OverworldAppearanceExternalRanges,
        Self::OverworldAppearanceRangesNotice,
        Self::OverworldAppearanceGraphics,
        Self::OverworldAppearancePalette,
        Self::OverworldAppearanceApplyRangesFormat,
        Self::OverworldAppearanceDisplay,
        Self::OverworldAppearanceEditorShadow,
        Self::OverworldAppearanceMap16Tiles,
        Self::OverworldAppearanceTextLabel,
        Self::OverworldAppearanceX,
        Self::OverworldAppearanceY,
        Self::OverworldAppearanceApplyDisplay,
        Self::OverworldAppearanceCustomMap16,
        Self::OverworldAppearanceNativeTile,
        Self::OverworldAppearanceTopLeft,
        Self::OverworldAppearanceTopRight,
        Self::OverworldAppearanceBottomLeft,
        Self::OverworldAppearanceBottomRight,
        Self::OverworldAppearanceApplyMap16,
        Self::OverworldAppearanceNativePartsFormat,
        Self::OverworldAppearanceAddPart,
        Self::OverworldAppearanceRemovePartNative,
        Self::OverworldAppearanceSendBackward,
        Self::OverworldAppearanceBringForward,
        Self::OverworldAppearanceMap16,
        Self::OverworldAppearanceTranslucent,
        Self::OverworldAppearanceAddRange,
        Self::OverworldAppearanceKind,
        Self::OverworldAppearanceFirst,
        Self::OverworldAppearanceLast,
        Self::OverworldAppearanceBase,
        Self::OverworldAppearanceRemoveRange,
        Self::ApplicationGfxOverrideTitle,
        Self::ApplicationGfxOverrideLayer12,
        Self::ApplicationGfxOverrideLayer3,
        Self::ApplicationGfxOverrideNotice,
        Self::ApplicationGfxOverrideOk,
        Self::ApplicationGfxOverrideCancel,
        Self::ApplicationToolbarBack,
        Self::ApplicationToolbarForward,
        Self::ApplicationToolbarLevel,
        Self::ApplicationRecentEmpty,
        Self::ApplicationRecentClear,
        Self::ApplicationRecentClearTitle,
        Self::ApplicationRecentClearNotice,
        Self::ApplicationRecentYes,
        Self::ApplicationRecentNo,
        Self::ApplicationIpsWarningTitle,
        Self::ApplicationIpsWarningFormat,
        Self::ApplicationIpsRenameNotice,
        Self::ApplicationIpsSaveQuestion,
        Self::ApplicationIpsSaveAnyway,
        Self::ApplicationIpsCancel,
        Self::ApplicationTwoBppTitle,
        Self::ApplicationTwoBppQuestion,
        Self::ApplicationYes,
        Self::ApplicationNo,
        Self::ApplicationTruncateTitle,
        Self::ApplicationTruncateNotice,
        Self::ExAnimationDocumentTitle,
        Self::ExAnimationDocumentOpenTitle,
        Self::ExAnimationDocumentMaximumRecords,
        Self::ExAnimationDocumentOpen,
        Self::ExAnimationDocumentUndo,
        Self::ExAnimationDocumentRedo,
        Self::ExAnimationDocumentSave,
        Self::ExAnimationDocumentModified,
        Self::ExAnimationDocumentSaved,
        Self::ExAnimationDocumentDiscardTitle,
        Self::ExAnimationDocumentDiscardNotice,
        Self::ExAnimationDocumentCancel,
        Self::ExAnimationDocumentDiscard,
        Self::ExAnimationDocumentErrorTitle,
        Self::ExAnimationDocumentOk,
        Self::ExAnimationDocumentRecords,
        Self::ExAnimationDocumentRecordListFormat,
        Self::ExAnimationDocumentAppendRecord,
        Self::ExAnimationDocumentRemoveSelected,
        Self::ExAnimationDocumentSlotSettings,
        Self::ExAnimationDocumentSettingHex,
        Self::ExAnimationDocumentHeaderHex,
        Self::ExAnimationDocumentTriggerValueHex,
        Self::ExAnimationDocumentRecordFormat,
        Self::ExAnimationDocumentKindHex,
        Self::ExAnimationDocumentTriggerHex,
        Self::ExAnimationDocumentDestinationHex,
        Self::ExAnimationDocumentSourceWordsNotice,
        Self::ExAnimationDocumentSpecialTransferNotice,
        Self::ExAnimationDocumentApplyRecord,
        Self::PaletteDocumentTitle,
        Self::PaletteDocumentUndo,
        Self::PaletteDocumentRedo,
        Self::PaletteDocumentSave,
        Self::PaletteDocumentModified,
        Self::PaletteDocumentSaved,
        Self::PaletteDocumentDiscardTitle,
        Self::PaletteDocumentDiscardNotice,
        Self::PaletteDocumentCancel,
        Self::PaletteDocumentDiscard,
        Self::PaletteDocumentErrorTitle,
        Self::PaletteDocumentOk,
        Self::PaletteDocumentColorFormat,
        Self::RomPaletteTitle,
        Self::RomPaletteStaleNotice,
        Self::RomPaletteAllocation,
        Self::RomPaletteRangeSeparator,
        Self::RomPaletteCommit,
        Self::RomPaletteCommitReclaim,
        Self::RomPaletteStaged,
        Self::RomPaletteUnmodified,
        Self::RomPaletteColorFormat,
        Self::RomPaletteShortcutNotice,
        Self::RomPaletteMaskMode,
        Self::RomPaletteEnableAll,
        Self::RomPaletteDisableAll,
        Self::RomPaletteMaskNotice,
        Self::RomPaletteDiscardTitle,
        Self::RomPaletteDiscardNotice,
        Self::RomPaletteCancel,
        Self::RomPaletteDiscard,
        Self::RomPaletteErrorTitle,
        Self::RomPaletteOk,
        Self::RomPaletteImportRow,
        Self::RomPaletteExportRow,
        Self::RomPaletteRowTransferNotice,
        Self::RomPaletteImportRaw,
        Self::RomPaletteExportRaw,
        Self::RomPaletteRawTransferNotice,
        Self::RomExAnimationTitle,
        Self::RomExAnimationSwitchDomain,
        Self::RomExAnimationGlobalUnavailableFormat,
        Self::RomExAnimationSwitchBlocked,
        Self::RomExAnimationGlobalTarget,
        Self::RomExAnimationLevelTargetFormat,
        Self::RomExAnimationCommit,
        Self::RomExAnimationStaged,
        Self::RomExAnimationUnmodified,
        Self::RomExAnimationAppendRecord,
        Self::RomExAnimationSpecialTransferNotice,
        Self::RomExAnimationReplaceRecord,
        Self::RomExAnimationDiscardTitle,
        Self::RomExAnimationDiscardNotice,
        Self::RomExAnimationCancel,
        Self::RomExAnimationDiscard,
        Self::RomExAnimationErrorTitle,
        Self::RomExAnimationOk,
        Self::RomPaletteImportTpl,
        Self::RomPaletteExportTpl,
        Self::RomPaletteImportRgb,
        Self::RomPaletteExportRgb,
        Self::RomPaletteSupportedTransferNotice,
        Self::CustomSpriteEditorTitle,
        Self::CustomSpritePlacementsFormat,
        Self::CustomSpritePlacement,
        Self::CustomSpriteRecordsNotice,
        Self::CustomSpriteDescriptionNotice,
        Self::CustomSpriteCopyPlacement,
        Self::CustomSpritePastePlacement,
        Self::CustomSpriteHeaderHex,
        Self::CustomSpriteApplyHeader,
        Self::CustomSpriteSearch,
        Self::CustomSpriteReplaceSelected,
        Self::CustomSpriteRemoveSelected,
        Self::CustomSpriteInsertAt,
        Self::CustomSpriteMoveTo,
        Self::CustomSpriteUtf8Bom,
        Self::CustomSpriteCrlf,
        Self::CustomSpriteTrailingLineEnding,
        Self::CustomSpriteApplyFraming,
        Self::CustomSpriteUndo,
        Self::CustomSpriteRedo,
        Self::CustomSpriteSavePair,
        Self::CustomSpriteModified,
        Self::CustomSpriteSaved,
        Self::CustomSpriteDiscardTitle,
        Self::CustomSpriteUnsavedNotice,
        Self::CustomSpriteCancel,
        Self::CustomSpriteDiscard,
        Self::CustomSpriteErrorTitle,
        Self::CustomSpriteOk,
        Self::CustomObjectEditorTitle,
        Self::CustomObjectEntriesFormat,
        Self::CustomObjectSearch,
        Self::CustomObjectEntry,
        Self::CustomObjectBytesNotice,
        Self::CustomObjectDescriptionNotice,
        Self::CustomObjectCopy,
        Self::CustomObjectPaste,
        Self::CustomObjectReplaceSelected,
        Self::CustomObjectRemoveSelected,
        Self::CustomObjectInsertAt,
        Self::CustomObjectMoveTo,
        Self::CustomObjectUtf8Bom,
        Self::CustomObjectCrlf,
        Self::CustomObjectTrailingLineEnding,
        Self::CustomObjectApplyFraming,
        Self::CustomObjectUndo,
        Self::CustomObjectRedo,
        Self::CustomObjectSavePair,
        Self::CustomObjectModified,
        Self::CustomObjectSaved,
        Self::CustomObjectDiscardTitle,
        Self::CustomObjectUnsavedNotice,
        Self::CustomObjectCancel,
        Self::CustomObjectDiscard,
        Self::CustomObjectErrorTitle,
        Self::CustomObjectOk,
        Self::AppearanceEditorTitle,
        Self::AppearancePainterRecordsFormat,
        Self::AppearanceSelected,
        Self::AppearanceSourceLayer1,
        Self::AppearanceSourceLayer2,
        Self::AppearanceSourceSprite,
        Self::AppearanceSourceIdHex,
        Self::AppearanceTileIndexHex,
        Self::AppearanceXOffsetDecimal,
        Self::AppearanceYOffsetDecimal,
        Self::AppearancePaletteRow,
        Self::AppearanceHorizontalFlip,
        Self::AppearanceVerticalFlip,
        Self::AppearanceReplaceSelected,
        Self::AppearanceRemoveSelected,
        Self::AppearanceInsertBefore,
        Self::AppearanceMoveBefore,
        Self::AppearanceUndo,
        Self::AppearanceRedo,
        Self::AppearanceSave,
        Self::AppearanceModified,
        Self::AppearanceSaved,
        Self::AppearanceDiscardTitle,
        Self::AppearanceUnsavedNotice,
        Self::AppearanceCancel,
        Self::AppearanceDiscard,
        Self::AppearanceErrorTitle,
        Self::AppearanceOk,
        Self::Layer3DocumentEditorTitle,
        Self::Layer3DocumentStartPosition,
        Self::Layer3DocumentTilemapSize,
        Self::Layer3DocumentLiquidType,
        Self::Layer3DocumentRawFlags,
        Self::Layer3DocumentGraphicsFormat,
        Self::Layer3DocumentReservedNotice,
        Self::Layer3DocumentTilemapNotice,
        Self::Layer3DocumentRemapNotice,
        Self::Layer3DocumentApplyAll,
        Self::Layer3DocumentCopyTilemap,
        Self::Layer3DocumentPasteTilemap,
        Self::Layer3DocumentCopyRemap,
        Self::Layer3DocumentPasteRemap,
        Self::Layer3DocumentUndo,
        Self::Layer3DocumentRedo,
        Self::Layer3DocumentSave,
        Self::Layer3DocumentModified,
        Self::Layer3DocumentSaved,
        Self::Layer3DocumentDiscardTitle,
        Self::Layer3DocumentUnsavedNotice,
        Self::Layer3DocumentCancel,
        Self::Layer3DocumentDiscard,
        Self::Layer3DocumentErrorTitle,
        Self::Layer3DocumentOk,
        Self::MetadataEditorTitle,
        Self::MetadataLevelNames,
        Self::MetadataPlayerStarts,
        Self::MetadataSubmapSettings,
        Self::MetadataUndo,
        Self::MetadataRedo,
        Self::MetadataSave,
        Self::MetadataModified,
        Self::MetadataSaved,
        Self::MetadataLevelNameRecord,
        Self::MetadataLevelKeyHex,
        Self::MetadataTileBytesHex,
        Self::MetadataRawFlagsHex,
        Self::MetadataPlayerStartRecord,
        Self::MetadataPlayerKeyHex,
        Self::MetadataXHex,
        Self::MetadataYHex,
        Self::MetadataSettingsRecord,
        Self::MetadataMusicHex,
        Self::MetadataPaletteHex,
        Self::MetadataLayer1ScrollHex,
        Self::MetadataLayer2ScrollHex,
        Self::MetadataUnknownBytesHex,
        Self::MetadataUpsertName,
        Self::MetadataUpsertStart,
        Self::MetadataUpsertSettings,
        Self::MetadataRemoveSelected,
        Self::MetadataSubmapMain,
        Self::MetadataSubmapYoshiIsland,
        Self::MetadataSubmapVanillaDome,
        Self::MetadataSubmapForestIllusion,
        Self::MetadataSubmapValleyBowser,
        Self::MetadataSubmapSpecialWorld,
        Self::MetadataSubmapStarWorld,
        Self::MetadataDiscardTitle,
        Self::MetadataUnsavedNotice,
        Self::MetadataCancel,
        Self::MetadataDiscard,
        Self::MetadataErrorTitle,
        Self::MetadataOk,
        Self::OscEditorTitle,
        Self::OscSourceSummaryFormat,
        Self::OscReplaceSource,
        Self::OscDiagnosticsHeading,
        Self::OscParsedRecord,
        Self::OscNoMetadataRecords,
        Self::OscUndo,
        Self::OscRedo,
        Self::OscSave,
        Self::OscModified,
        Self::OscSaved,
        Self::OscDiscardTitle,
        Self::OscUnsavedNotice,
        Self::OscCancel,
        Self::OscDiscard,
        Self::OscErrorTitle,
        Self::OscOk,
        Self::SscEditorTitle,
        Self::SscSourceSummaryFormat,
        Self::SscAssetsSummaryFormat,
        Self::SscPaletteLoaded,
        Self::SscPaletteMissing,
        Self::SscReplaceSource,
        Self::SscDiagnosticsHeading,
        Self::SscParsedRecord,
        Self::SscNoMetadataRecords,
        Self::SscUndo,
        Self::SscRedo,
        Self::SscSave,
        Self::SscModified,
        Self::SscSaved,
        Self::SscDiscardTitle,
        Self::SscUnsavedNotice,
        Self::SscCancel,
        Self::SscDiscard,
        Self::SscErrorTitle,
        Self::SscOk,
        Self::DscEditorTitle,
        Self::DscSourceSummaryFormat,
        Self::DscSourceNotice,
        Self::DscReplaceSource,
        Self::DscDiagnosticsHeading,
        Self::DscParsedRecord,
        Self::DscNoRecoveredRecords,
        Self::DscUndo,
        Self::DscRedo,
        Self::DscSave,
        Self::DscModified,
        Self::DscSaved,
        Self::DscDiscardTitle,
        Self::DscUnsavedNotice,
        Self::DscCancel,
        Self::DscDiscard,
        Self::DscErrorTitle,
        Self::DscOk,
        Self::TilemapTitleScreenName,
        Self::TilemapCreditsName,
        Self::TilemapEditorTitleFormat,
        Self::TilemapDimensionsFormat,
        Self::TilemapStaleNotice,
        Self::TilemapRow,
        Self::TilemapColumn,
        Self::TilemapPlane,
        Self::TilemapPrimary,
        Self::TilemapSecondary,
        Self::TilemapWord,
        Self::TilemapLoadTile,
        Self::TilemapApplyTile,
        Self::TilemapCommit,
        Self::TilemapStaged,
        Self::TilemapUnchanged,
        Self::TilemapDiscardTitleFormat,
        Self::TilemapUnsavedNotice,
        Self::TilemapErrorTitleFormat,
        Self::EventNumberEditorTitle,
        Self::EventNumberDescription,
        Self::EventNumberStoredLengthFormat,
        Self::EventNumberStaleNotice,
        Self::EventNumberEvent,
        Self::EventNumberMappedEvent,
        Self::EventNumberLoadEntry,
        Self::EventNumberApplyEntry,
        Self::EventNumberCommit,
        Self::EventNumberStaged,
        Self::EventNumberUnchanged,
        Self::EventNumberDiscardTitle,
        Self::EventNumberUnsavedNotice,
        Self::EventNumberErrorTitle,
        Self::LevelNameEditorTitle,
        Self::LevelNameDescription,
        Self::LevelNameCountFormat,
        Self::LevelNameStaleNotice,
        Self::LevelNameLevel,
        Self::LevelNameTile,
        Self::LevelNameTileValue,
        Self::LevelNameLoadTile,
        Self::LevelNameApplyTile,
        Self::LevelNameCommit,
        Self::LevelNameStaged,
        Self::LevelNameUnchanged,
        Self::LevelNameDiscardTitle,
        Self::LevelNameUnsavedNotice,
        Self::LevelNameErrorTitle,
        Self::PlayerStartEditorTitle,
        Self::PlayerStartDescription,
        Self::PlayerStartReservedFormat,
        Self::PlayerStartStaleNotice,
        Self::PlayerStartPlayer,
        Self::PlayerStartMario,
        Self::PlayerStartLuigi,
        Self::PlayerStartLoad,
        Self::PlayerStartX,
        Self::PlayerStartY,
        Self::PlayerStartSubmap,
        Self::PlayerStartInvalid,
        Self::PlayerStartMainMap,
        Self::PlayerStartYoshisIsland,
        Self::PlayerStartVanillaDome,
        Self::PlayerStartForestIllusion,
        Self::PlayerStartValleyBowser,
        Self::PlayerStartSpecialWorld,
        Self::PlayerStartStarWorld,
        Self::PlayerStartApply,
        Self::PlayerStartCommit,
        Self::PlayerStartStaged,
        Self::PlayerStartUnchanged,
        Self::PlayerStartDiscardTitle,
        Self::PlayerStartUnsavedNotice,
        Self::PlayerStartErrorTitle,
        Self::SpecialEventEditorTitle,
        Self::SpecialEventDescription,
        Self::SpecialEventStaleNotice,
        Self::SpecialEventIndex,
        Self::SpecialEventSourceTile,
        Self::SpecialEventDestinationTile,
        Self::SpecialEventDirection,
        Self::SpecialEventLoadEntry,
        Self::SpecialEventApplyEntry,
        Self::SpecialEventCommit,
        Self::SpecialEventStaged,
        Self::SpecialEventUnchanged,
        Self::SpecialEventDiscardTitle,
        Self::SpecialEventUnsavedNotice,
        Self::SpecialEventErrorTitle,
        Self::EventRevealEditorTitle,
        Self::EventRevealDescription,
        Self::EventRevealCountFormat,
        Self::EventRevealStaleNotice,
        Self::EventRevealIndex,
        Self::EventRevealSourceTile,
        Self::EventRevealDestinationTile,
        Self::EventRevealTableCount,
        Self::EventRevealResizeTable,
        Self::EventRevealLoad,
        Self::EventRevealApply,
        Self::EventRevealCommit,
        Self::EventRevealStaged,
        Self::EventRevealUnchanged,
        Self::EventRevealDiscardTitle,
        Self::EventRevealUnsavedNotice,
        Self::EventRevealErrorTitle,
        Self::EventTilemapEditorTitle,
        Self::EventTilemapDescription,
        Self::EventTilemapLoadedStorageFormat,
        Self::EventTilemapPristineStorage,
        Self::EventTilemapInstalledStorage,
        Self::EventTilemapStaleNotice,
        Self::EventTilemapTileIndex,
        Self::EventTilemapPlane,
        Self::EventTilemapPrimaryLow,
        Self::EventTilemapPrimaryHigh,
        Self::EventTilemapSecondaryHigh,
        Self::EventTilemapByteValue,
        Self::EventTilemapLoadByte,
        Self::EventTilemapApplyByte,
        Self::EventTilemapCommit,
        Self::EventTilemapStaged,
        Self::EventTilemapUnchanged,
        Self::EventTilemapDiscardTitle,
        Self::EventTilemapUnsavedNotice,
        Self::EventTilemapErrorTitle,
        Self::OverworldSettingsEditorTitle,
        Self::OverworldSettingsDescription,
        Self::OverworldSettingsInstalled,
        Self::OverworldSettingsPristine,
        Self::OverworldSettingsStaleNotice,
        Self::OverworldSettingsSubmapRecord,
        Self::OverworldSettingsLoad,
        Self::OverworldSettingsWordFormat,
        Self::OverworldSettingsLayer3Header,
        Self::OverworldSettingsUseCustomTilemap,
        Self::OverworldSettingsUseCustomGraphics,
        Self::OverworldSettingsTilemapFile,
        Self::OverworldSettingsTilemapSize,
        Self::OverworldSettingsTilemapPosition,
        Self::OverworldSettingsAddressLayoutWords,
        Self::OverworldSettingsGraphicsFiles,
        Self::OverworldSettingsGfxFormat,
        Self::OverworldSettingsApplyLayer3,
        Self::OverworldSettingsPreservationNotice,
        Self::OverworldSettingsApplyRecord,
        Self::OverworldSettingsCommit,
        Self::OverworldSettingsStaged,
        Self::OverworldSettingsUnchanged,
        Self::OverworldSettingsDiscardTitle,
        Self::OverworldSettingsUnsavedNotice,
        Self::OverworldSettingsErrorTitle,
        Self::SecondaryExitDescription,
        Self::SecondaryExitStaleNotice,
        Self::SecondaryExitEntry,
        Self::SecondaryExitLoad,
        Self::SecondaryExitPositionMethod,
        Self::SecondaryExitDestinationFlags,
        Self::SecondaryExitXOverworldFlags,
        Self::SecondaryExitAdditionalFlags,
        Self::SecondaryExitApplyEntry,
        Self::SecondaryExitCommit,
        Self::SecondaryExitStaged,
        Self::SecondaryExitUnchanged,
        Self::SecondaryExitClearAllTitle,
        Self::SecondaryExitClearAllNotice,
        Self::SecondaryExitClearAll,
        Self::SecondaryExitDiscardTitle,
        Self::SecondaryExitUnsavedNotice,
        Self::SecondaryExitErrorTitle,
        Self::SharedPaletteEditorTitle,
        Self::SharedPaletteSummaryFormat,
        Self::SharedPaletteStaleNotice,
        Self::SharedPaletteImport,
        Self::SharedPaletteExport,
        Self::SharedPaletteTransferNotice,
        Self::SharedPalettePage,
        Self::SharedPalettePageOfFormat,
        Self::SharedPaletteSelectedFormat,
        Self::SharedPaletteBgr555,
        Self::SharedPaletteDecodeRaw,
        Self::SharedPaletteRed,
        Self::SharedPaletteGreen,
        Self::SharedPaletteBlue,
        Self::SharedPalettePreview,
        Self::SharedPaletteApplyRgb,
        Self::SharedPaletteApplyRaw,
        Self::SharedPaletteCopyRow,
        Self::SharedPalettePasteRow,
        Self::SharedPaletteCopyColor,
        Self::SharedPalettePasteColor,
        Self::SharedPaletteClipboardNotice,
        Self::SharedPaletteAuxiliaryBytes,
        Self::SharedPaletteStageAuxiliary,
        Self::SharedPaletteCommit,
        Self::SharedPaletteStaged,
        Self::SharedPaletteUnchanged,
        Self::SharedPaletteDiscardTitle,
        Self::SharedPaletteUnsavedNotice,
        Self::SharedPaletteErrorTitle,
        Self::GraphicsExternalRunningTitle,
        Self::GraphicsExternalWaitingFormat,
        Self::GraphicsExternalReloadNotice,
        Self::GraphicsExternalConsentTitle,
        Self::GraphicsExternalExecutableFormat,
        Self::GraphicsExternalStagedFileFormat,
        Self::GraphicsExternalArgumentsNotice,
        Self::GraphicsExternalArgumentFormat,
        Self::GraphicsExternalRun,
        Self::GraphicsOwnershipEditable,
        Self::GraphicsOwnershipFixed,
        Self::GraphicsOwnershipExAnimationFormat,
        Self::GraphicsOwnershipOriginalAnimationFormat,
        Self::GraphicsOwnershipLevelExAnimationFormat,
        Self::GraphicsOwnershipGlobalExAnimationFormat,
        Self::GraphicsOwnershipInvalid,
        Self::GraphicsDiscardTitle,
        Self::GraphicsUnsavedNotice,
        Self::GraphicsErrorTitle,
        Self::GraphicsEditorTitle,
        Self::PortableGraphicsEditorTitle,
        Self::PortableGraphicsDiscardTitle,
        Self::PortableGraphicsUnsavedNotice,
        Self::PortableGraphicsErrorTitle,
        Self::PortableGraphicsUndo,
        Self::PortableGraphicsRedo,
        Self::PortableGraphicsSave,
        Self::PortableGraphicsCopyTile,
        Self::PortableGraphicsPasteTile,
        Self::PortableGraphicsModified,
        Self::PortableGraphicsSaved,
        Self::PortableGraphicsNoTiles,
        Self::PortableGraphicsTileFormat,
        Self::PortableGraphicsCancel,
        Self::PortableGraphicsDiscard,
        Self::PortableGraphicsOk,
        Self::GraphicsRotateClockwise,
        Self::GraphicsFlipHorizontal,
        Self::GraphicsFlipVertical,
        Self::GraphicsPreviousPage,
        Self::GraphicsNextPage,
        Self::GraphicsPreviousPalette,
        Self::GraphicsNextPalette,
        Self::GraphicsColorMapFilters,
        Self::GraphicsApplyColorMapFilter,
        Self::GraphicsFilterFormat,
        Self::GraphicsStaleNotice,
        Self::GraphicsPaletteRow,
        Self::GraphicsDefaultPalette,
        Self::GraphicsUseJoined,
        Self::GraphicsJoinedNotice,
        Self::GraphicsConfiguredEditor,
        Self::GraphicsNone,
        Self::GraphicsEditConfigured,
        Self::GraphicsEditExecutable,
        Self::GraphicsInsertRaw,
        Self::GraphicsExtractRaw,
        Self::GraphicsExtractLevel,
        Self::GraphicsExtractLevelNotice,
        Self::GraphicsExtractStandard,
        Self::GraphicsExtractSpecial,
        Self::GraphicsSpecialNotice,
        Self::GraphicsExtractExGfx,
        Self::GraphicsExtractExGfxNotice,
        Self::GraphicsExtractAllGfx,
        Self::GraphicsInsertStandard,
        Self::GraphicsStagedEditNotice,
        Self::GraphicsInsertSpecial,
        Self::GraphicsInsertExGfx,
        Self::GraphicsInsertExGfxNotice,
        Self::GraphicsInsertAllGfx,
        Self::GraphicsAllocationPc,
        Self::GraphicsAllocationRangeSeparator,
        Self::GraphicsCommit,
        Self::GraphicsCommitReclaim,
        Self::GraphicsStagedChanges,
        Self::GraphicsNoStagedChanges,
        Self::GraphicsInternalCacheNotice,
        Self::GraphicsSaveLevelTitle,
        Self::GraphicsSaveLevelQuestion,
        Self::GraphicsSaveLevelPurpose,
        Self::GraphicsSaveLevelWarning,
        Self::GraphicsNoTiles,
        Self::GraphicsTileFormat,
        Self::GraphicsInternalTileNotice,
        Self::GraphicsCopyTile,
        Self::GraphicsPasteTile,
        Self::GraphicsFormatWarningTitle,
        Self::GraphicsFormatWarningBody,
        Self::GraphicsYes,
        Self::GraphicsNo,
        Self::GraphicsExtractingFormat,
        Self::GraphicsStagingFormat,
        Self::GraphicsBatchAtomicNotice,
        Self::GraphicsCancellingNotice,
        Self::GraphicsInsertingFormat,
        Self::GraphicsReadingFormat,
        Self::GraphicsImportAtomicNotice,
        Self::GraphicsToolbarGfxCompleteTitle,
        Self::GraphicsToolbarExGfxCompleteTitle,
        Self::GraphicsToolbarGfxCompleteFormat,
        Self::GraphicsToolbarExGfxCompleteFormat,
        Self::GraphicsToolbarErrorTitle,
        Self::RomMap16EditorTitle,
        Self::RomMap16StaleNotice,
        Self::RomMap16PreviewLevel,
        Self::RomMap16ObjectSet,
        Self::RomMap16FgPalette,
        Self::RomMap16Grid,
        Self::RomMap16GridNotice,
        Self::RomMap16GridColor,
        Self::RomMap16ZoomOut,
        Self::RomMap16ZoomReset,
        Self::RomMap16ZoomIn,
        Self::RomMap16PageNumber,
        Self::RomMap16PageNumberNotice,
        Self::RomMap16LockPages,
        Self::RomMap16UnlockPages,
        Self::RomMap16PreviewHexError,
        Self::RomMap16PreviewRangeError,
        Self::RomMap16SelectionNotice,
        Self::RomMap16Page,
        Self::RomMap16Tile,
        Self::RomMap16Quadrant,
        Self::RomMap16AddressFormat,
        Self::RomMap16CopyTile,
        Self::RomMap16PasteTile,
        Self::RomMap16Undo,
        Self::RomMap16Redo,
        Self::RomMap16Subtile,
        Self::RomMap16Palette,
        Self::RomMap16Priority,
        Self::RomMap16XFlip,
        Self::RomMap16YFlip,
        Self::RomMap16ApplySubtile,
        Self::RomMap16ActsLike,
        Self::RomMap16ApplyActsLike,
        Self::RomMap16NoActsLikeNotice,
        Self::RomMap16ProtectedNotice,
        Self::RomMap16UnlockTitle,
        Self::RomMap16LockTitle,
        Self::RomMap16UnlockWarning,
        Self::RomMap16LockQuestion,
        Self::RomMap16Unlock,
        Self::RomMap16Lock,
        Self::RomMap16AllocationPc,
        Self::RomMap16AllocationSeparator,
        Self::RomMap16Commit,
        Self::RomMap16CommitReclaim,
        Self::RomMap16Staged,
        Self::RomMap16Unchanged,
        Self::RomMap16DiscardTitle,
        Self::RomMap16UnsavedNotice,
        Self::RomMap16ErrorTitle,
        Self::RomMap16TransferImportComplete,
        Self::RomMap16TransferExportComplete,
        Self::RomMap16TransferTemplateNotice,
        Self::RomMap16TransferNativeOnlyNotice,
        Self::RomMap16TransferSelectedWidth,
        Self::RomMap16TransferSelectedHeight,
        Self::RomMap16TransferFileOrigin,
        Self::RomMap16TransferImportSelected,
        Self::RomMap16TransferExportSelected,
        Self::RomMap16TransferCopyRectangle,
        Self::RomMap16TransferPasteRectangle,
        Self::RomMap16TransferSelectedNotice,
        Self::RomMap16TransferImportPage,
        Self::RomMap16TransferExportPage,
        Self::RomMap16TransferPageNotice,
        Self::RomMap16TransferPageUnsupportedNotice,
        Self::RomMap16TransferImportForeground,
        Self::RomMap16TransferExportForeground,
        Self::RomMap16TransferImportBackground,
        Self::RomMap16TransferExportBackground,
        Self::RomMap16TransferLegacyCompleteNotice,
        Self::RomMap16SidecarHeading,
        Self::RomMap16SidecarExportM16,
        Self::RomMap16SidecarExportS16,
        Self::RomMap16SidecarConfirmTitle,
        Self::RomMap16SidecarConfirmQuestion,
        Self::RomMap16SidecarNo,
        Self::RomMap16SidecarYes,
        Self::RomMap16SnesHeading,
        Self::RomMap16SnesImportPalette,
        Self::RomMap16SnesPaletteRowPrefix,
        Self::RomMap16SnesOptimize,
        Self::RomMap16SnesLoad,
        Self::RomMap16SnesGraphicsOffset,
        Self::RomMap16SnesMapOffset,
        Self::RomMap16SnesColorFilter,
        Self::RomMap16SnesColorMap,
        Self::RomMap16SnesNotice,
        Self::RomMap16SnesPreviewTitle,
        Self::RomMap16SnesTargetPage,
        Self::RomMap16SnesPlacement,
        Self::RomMap16SnesGraphicsTiles,
        Self::RomMap16SnesCandidateDefinitions,
        Self::RomMap16SnesDefinitionsWritten,
        Self::RomMap16SnesIndexGridSpan,
        Self::RomMap16SnesPaletteLoaded,
        Self::RomMap16SnesPaletteNotLoaded,
        Self::RomMap16SnesStaleNotice,
        Self::RomMap16SnesPreviewNotice,
        Self::RomMap16SnesApply,
        Self::RomMap16SnesDiscard,
        Self::RomMap16BitmapOpeningTitle,
        Self::RomMap16BitmapReadingClipboard,
        Self::RomMap16BitmapTitle,
        Self::RomMap16BitmapStaleNotice,
        Self::RomMap16BitmapOptimize8x8,
        Self::RomMap16BitmapReuse8x8,
        Self::RomMap16BitmapReservedBlank,
        Self::RomMap16BitmapOptimize16x16,
        Self::RomMap16BitmapLayerPriority,
        Self::RomMap16BitmapConfiguredBlank,
        Self::RomMap16BitmapFirst8x8,
        Self::RomMap16BitmapBlank8x8,
        Self::RomMap16BitmapFirstMap16,
        Self::RomMap16BitmapReservedMap16,
        Self::RomMap16BitmapPlan,
        Self::RomMap16BitmapAllocation,
        Self::RomMap16BitmapExhausted,
        Self::RomMap16BitmapImport,
        Self::RomMap16BitmapCancel,
        Self::RomMap16BitmapPreviewZoom,
        Self::RomMap16BitmapResetPan,
        Self::RomMap16BitmapOriginal,
        Self::RomMap16BitmapConverted,
        Self::RomMap16BitmapHeading,
        Self::RomMap16BitmapLevelNotice,
        Self::RomMap16BitmapGfxSlot4,
        Self::RomMap16BitmapGfxSlot5,
        Self::RomMap16BitmapGfxNotice,
        Self::RomMap16BitmapChoose,
        Self::RomMap16BitmapPaste,
        Self::RomMap16BitmapMaximumColors,
        Self::RomMap16BitmapPriority,
        Self::RomMap16BitmapMedianCut,
        Self::RomMap16BitmapPopularity,
        Self::RomMap16BitmapAllowUnmarked,
        Self::RomMap16BitmapPrioritizeExact,
        Self::RomMap16BitmapPrioritizeExactNotice,
        Self::RomMap16BitmapHueTolerance,
        Self::RomMap16BitmapPaletteLegend,
        Self::RomMap16BitmapUniqueColors,
        Self::RomMap16BitmapMaintainDetail,
        Self::RomMap16BitmapReduceMethod1,
        Self::RomMap16BitmapReduceMethod2,
        Self::RomExpansionTitle,
        Self::RomExpansionTargetNotice,
        Self::RomExpansionAlignmentNotice,
        Self::RomExpansionLmTarget,
        Self::RomExpansion2MiB,
        Self::RomExpansion3MiB,
        Self::RomExpansion4MiB,
        Self::RomExpansionExLoRomHeading,
        Self::RomExpansionExLoRomNotice,
        Self::RomExpansionExLoRomConvert,
        Self::RomExpansionExLoRomRequires,
        Self::RomExpansionSa1Heading,
        Self::RomExpansion6MiB,
        Self::RomExpansion8MiB,
        Self::RomExpansionSa1Requires,
        Self::RomExpansionTarget,
        Self::RomExpansionFillByte,
        Self::RomExpansionSa1FixedNotice,
        Self::RomExpansionCancel,
        Self::RomExpansionApply,
        Self::RomExpansionExLoRomWarningTitle,
        Self::RomExpansionMapperWarning,
        Self::RomExpansionCompatibilityWarning,
        Self::RomExpansionUndoableNotice,
        Self::RomExpansionConvertRom,
        Self::RomExpansionSa1ConfirmTitle,
        Self::RomExpansionSa1ConfirmNotice,
        Self::RomExpansionSnes9xNotice,
        Self::RomExpansionZsnesNotice,
        Self::RomExpansionExpandRom,
        Self::RomExpansionErrorTitle,
        Self::RomExpansionOk,
        Self::RomExpandedSettingsTitle,
        Self::RomExpandedSettingsRecordNotice,
        Self::RomExpandedSettingsStaleNotice,
        Self::RomExpandedSettingsLayer3Heading,
        Self::RomExpandedSettingsLayer3Enable,
        Self::RomExpandedSettingsGfxFile,
        Self::RomExpandedSettingsLengthSelector,
        Self::RomExpandedSettingsDestinationSelector,
        Self::RomExpandedSettingsStageLayer3,
        Self::RomExpandedSettingsExpandedMode,
        Self::RomExpandedSettingsExpandedModeNotice,
        Self::RomExpandedSettingsStageExpandedMode,
        Self::RomExpandedSettingsBypassHeading,
        Self::RomExpandedSettingsBypassEnable,
        Self::RomExpandedSettingsStageBypass,
        Self::RomExpandedSettingsBoundaryHeading,
        Self::RomExpandedSettingsBoundaryAir,
        Self::RomExpandedSettingsStageBoundary,
        Self::RomExpandedSettingsWordsHeading,
        Self::RomExpandedSettingsWord,
        Self::RomExpandedSettingsStageWords,
        Self::RomExpandedSettingsCommit,
        Self::RomExpandedSettingsStaged,
        Self::RomExpandedSettingsUnchanged,
        Self::RomExpandedSettingsDiscardTitle,
        Self::RomExpandedSettingsUnsavedNotice,
        Self::RomExpandedSettingsCancel,
        Self::RomExpandedSettingsDiscard,
        Self::RomExpandedSettingsErrorTitle,
        Self::RomExpandedSettingsOk,
        Self::RomExpandedSettingsGfxSlotFormat,
        Self::ExpandedSettingsDocumentTitle,
        Self::ExpandedSettingsRecoveredNotice,
        Self::ExpandedSettingsApplyLayer3,
        Self::ExpandedSettingsApplyExpandedMode,
        Self::ExpandedSettingsApplyBypass,
        Self::ExpandedSettingsApplyBoundary,
        Self::ExpandedSettingsWordsNotice,
        Self::ExpandedSettingsApplyWords,
        Self::ExpandedSettingsUndo,
        Self::ExpandedSettingsRedo,
        Self::ExpandedSettingsSave,
        Self::ExpandedSettingsModified,
        Self::ExpandedSettingsSaved,
        Self::ExpandedSettingsUnsavedTitle,
        Self::ExpandedSettingsDiscardQuestion,
        Self::ExpandedSettingsErrorTitle,
        Self::LevelRestrictionEditingWarning,
        Self::LevelRestrictionAcknowledge,
        Self::LevelRestrictionRestoreTitle,
        Self::LevelRestrictionRestoreNotice,
        Self::LevelRestrictionRetryRestore,
        Self::LevelRestrictionIpsTitle,
        Self::LevelRestrictionIpsQuestion,
        Self::LevelRestrictionYes,
        Self::LevelRestrictionNo,
        Self::LevelRestrictionSavingTitle,
        Self::LevelRestrictionSavingForIps,
        Self::LevelRestrictionSaveRequired,
        Self::LevelRestrictionRetrySave,
        Self::LevelRestrictionCompleteTitle,
        Self::LevelRestrictionCompleteNotice,
        Self::LevelRestrictionOk,
        Self::LevelRestrictionSavingForClose,
        Self::LevelRestrictionStillOpen,
        Self::LevelRestrictionRetrySaveClose,
        Self::LevelRestrictionErrorTitle,
        Self::OverworldAnimationThisMap,
        Self::OverworldAnimationGlobal,
        Self::OverworldAnimationGlobalReadOnly,
        Self::OverworldAnimationSetting,
        Self::OverworldAnimationHeader,
        Self::OverworldAnimationApplyGlobals,
        Self::OverworldAnimationTrigger,
        Self::OverworldAnimationEnabled,
        Self::OverworldAnimationValue,
        Self::OverworldAnimationApplyTrigger,
        Self::OverworldAnimationRecord,
        Self::OverworldAnimationKind,
        Self::OverworldAnimationRecordTrigger,
        Self::OverworldAnimationDestination,
        Self::OverworldAnimationDestinationFlag,
        Self::OverworldAnimationSourceWords,
        Self::OverworldAnimationSpecialNotice,
        Self::OverworldAnimationAppend,
        Self::OverworldAnimationReplace,
        Self::OverworldAnimationRemove,
        Self::OverworldAnimationCopyRecord,
        Self::OverworldAnimationPasteRecord,
        Self::OverworldAnimationFramePrefix,
        Self::OverworldAnimationCopyFrame,
        Self::OverworldAnimationPasteFrame,
        Self::OverworldAnimationOptionsHeading,
        Self::OverworldAnimationMapSelector,
        Self::OverworldAnimationOriginalPalette,
        Self::OverworldAnimationOriginalTiles,
        Self::OverworldAnimationGlobalFeature,
        Self::OverworldAnimationMapFeature,
        Self::OverworldAnimationOriginalLightning,
        Self::OverworldAnimationOptionsUnsupported,
        Self::OverworldAnimationRuntimeRequired,
        Self::OverworldAnimationInstallRuntime,
        Self::OverworldAnimationInstallRuntimeNotice,
        Self::OverworldAnimationInstallBlocked,
        Self::OverworldAnimationPreviewHeading,
        Self::OverworldAnimationPlay,
        Self::OverworldAnimationPause,
        Self::OverworldAnimationReset,
        Self::OverworldAnimationStepTimer,
        Self::OverworldAnimationPhaseTick,
        Self::OverworldAnimationTimerNotice,
        Self::OverworldAnimationCustom,
        Self::OverworldAnimationOneShot,
        Self::OverworldAnimationManualFrame,
        Self::OverworldAnimationActive,
        Self::OverworldAnimationEventPrefix,
        Self::OverworldAnimationPassed,
        Self::OverworldAnimationEventManualNotice,
        Self::OverworldAnimationNoRecordsNotice,
        Self::TitleRecordingTitle,
        Self::TitleRecordingDescription,
        Self::TitleRecordingNoPlayback,
        Self::TitleRecordingStaleNotice,
        Self::TitleRecordingBytesPresent,
        Self::TitleRecordingEnterPayload,
        Self::TitleRecordingMinimalPayload,
        Self::TitleRecordingNormalizeHex,
        Self::TitleRecordingCommit,
        Self::TitleRecordingModified,
        Self::TitleRecordingUnchanged,
        Self::TitleRecordingRecorderHeading,
        Self::TitleRecordingRecorderAbsentNotice,
        Self::TitleRecordingInstallRecorder,
        Self::TitleRecordingRecorderInstalledNotice,
        Self::TitleRecordingUninstallRecorder,
        Self::TitleRecordingFilesHeading,
        Self::TitleRecordingImportNative,
        Self::TitleRecordingImportZsnes,
        Self::TitleRecordingImportSnes9x,
        Self::TitleRecordingExportNative,
        Self::TitleRecordingExportZsnes,
        Self::TitleRecordingTransferNotice,
        Self::TitleRecordingDiscardTitle,
        Self::TitleRecordingUnsavedNotice,
        Self::TitleRecordingCancel,
        Self::TitleRecordingDiscard,
        Self::TitleRecordingErrorTitle,
        Self::TitleRecordingOk,
        Self::OverworldMessageTitle,
        Self::OverworldMessageDescription,
        Self::OverworldMessageStorageStatus,
        Self::OverworldMessageStaleNotice,
        Self::OverworldMessageTableCount,
        Self::OverworldMessageResize,
        Self::OverworldMessageIndex,
        Self::OverworldMessageColumn,
        Self::OverworldMessageTileValue,
        Self::OverworldMessageDiscardTitle,
        Self::OverworldMessageUnsavedNotice,
        Self::OverworldMessageErrorTitle,
        Self::BossMessageTitle,
        Self::BossMessageDescription,
        Self::BossMessageStaleNotice,
        Self::BossMessageIndex,
        Self::BossMessageColumn,
        Self::BossMessageTileValue,
        Self::BossMessageDiscardTitle,
        Self::BossMessageUnsavedNotice,
        Self::BossMessageErrorTitle,
        Self::MessageEditorRow,
        Self::MessageEditorLoadTile,
        Self::MessageEditorApplyTile,
        Self::MessageEditorCommit,
        Self::MessageEditorStaged,
        Self::MessageEditorUnchanged,
        Self::MessageEditorCancel,
        Self::MessageEditorDiscard,
        Self::MessageEditorOk,
        Self::RomMetadataTitle,
        Self::RomMetadataDescription,
        Self::RomMetadataSummary,
        Self::RomMetadataStaleNotice,
        Self::RomMetadataRegion,
        Self::RomMetadataAttribution,
        Self::RomMetadataAttributionRange,
        Self::RomMetadataVramVersion,
        Self::RomMetadataVramVersionRange,
        Self::RomMetadataFeatureRecord,
        Self::RomMetadataFeatureRecordRange,
        Self::RomMetadataByteIndex,
        Self::RomMetadataByteValue,
        Self::RomMetadataLoadByte,
        Self::RomMetadataApplyByte,
        Self::RomMetadataCommit,
        Self::RomMetadataStaged,
        Self::RomMetadataUnchanged,
        Self::RomMetadataDiscardTitle,
        Self::RomMetadataUnsavedNotice,
        Self::RomMetadataCancel,
        Self::RomMetadataDiscard,
        Self::RomMetadataErrorTitle,
        Self::RomMetadataOk,
        Self::LegacyBypassFgBgTitle,
        Self::LegacyBypassSpriteTitle,
        Self::LegacyBypassDescription,
        Self::LegacyBypassEnable,
        Self::LegacyBypassListRow,
        Self::LegacyBypassRegularRow,
        Self::LegacyBypassRegularNotice,
        Self::LegacyBypassZeroFallback,
        Self::LegacyBypassStaleNotice,
        Self::LegacyBypassStage,
        Self::LegacyBypassCommit,
        Self::LegacyBypassStaged,
        Self::LegacyBypassUnchanged,
        Self::LegacyBypassDiscardTitle,
        Self::LegacyBypassUnsavedNotice,
        Self::LegacyBypassCancel,
        Self::LegacyBypassDiscard,
        Self::LegacyBypassErrorTitle,
        Self::LegacyBypassOk,
        Self::CopierHeaderTitle,
        Self::CopierHeaderLogicalRomFormat,
        Self::CopierHeaderCurrentStateFormat,
        Self::CopierHeaderTarget,
        Self::CopierHeaderAbsent,
        Self::CopierHeaderPresent,
        Self::CopierHeaderFillByte,
        Self::CopierHeaderPreservationNotice,
        Self::CopierHeaderUseCanonical,
        Self::CopierHeaderCancel,
        Self::CopierHeaderConvert,
        Self::CopierHeaderErrorTitle,
        Self::CopierHeaderOk,
        Self::IpsApplyTitle,
        Self::IpsApplyHeaderNotice,
        Self::IpsApplySummaryFormat,
        Self::IpsApplyIdentityNotice,
        Self::IpsApplyStaleNotice,
        Self::IpsApplyCancel,
        Self::IpsApplyAction,
        Self::IpsApplyErrorTitle,
        Self::IpsApplyOk,
        Self::IpsCreateOriginalPrompt,
        Self::IpsCreateModifiedPrompt,
        Self::IpsCreateTitle,
        Self::IpsCreateOriginalFormat,
        Self::IpsCreateModifiedFormat,
        Self::IpsCreateOutputFormat,
        Self::IpsCreateProgress,
        Self::IpsCreateCompletedTitle,
        Self::IpsCreateCompletedFormat,
        Self::IpsCreateErrorTitle,
        Self::IpsCreateOk,
        Self::RatsReclaimTitle,
        Self::RatsReclaimOwnershipNotice,
        Self::RatsReclaimSummaryFormat,
        Self::RatsReclaimFillByte,
        Self::RatsReclaimTransactionNotice,
        Self::RatsReclaimStaleNotice,
        Self::RatsReclaimCancel,
        Self::RatsReclaimAction,
        Self::RatsReclaimErrorTitle,
        Self::RatsReclaimOk,
        Self::RevisionPatchTitle,
        Self::RevisionPatchIdentityFormat,
        Self::RevisionPatchPayloadSummaryFormat,
        Self::RevisionPatchRangeNotice,
        Self::RevisionPatchSearchStart,
        Self::RevisionPatchSearchEnd,
        Self::RevisionPatchExpansionFill,
        Self::RevisionPatchAtomicNotice,
        Self::RevisionPatchStaleNotice,
        Self::RevisionPatchCancel,
        Self::RevisionPatchInstall,
        Self::RevisionPatchErrorTitle,
        Self::RevisionPatchOk,
        Self::BuiltInRuntimeTitle,
        Self::BuiltInRuntimeTarget,
        Self::BuiltInRuntimeFamily,
        Self::BuiltInRuntimeExpandedSettings,
        Self::BuiltInRuntimeCompleteLayer3,
        Self::BuiltInRuntimeLfix3,
        Self::BuiltInRuntimeMap16,
        Self::BuiltInRuntimeExAnimation,
        Self::BuiltInRuntimeLayer2,
        Self::BuiltInRuntimeSprite19,
        Self::BuiltInRuntimeSupportPatchB,
        Self::BuiltInRuntimeLz2Speed,
        Self::BuiltInRuntimeSharedPalettes,
        Self::BuiltInRuntimeExpandedSettingsDescription,
        Self::BuiltInRuntimeCompleteLayer3Description,
        Self::BuiltInRuntimeLfix3Description,
        Self::BuiltInRuntimeMap16Description,
        Self::BuiltInRuntimeExAnimationDescription,
        Self::BuiltInRuntimeLayer2Description,
        Self::BuiltInRuntimeSprite19Description,
        Self::BuiltInRuntimeSupportPatchBDescription,
        Self::BuiltInRuntimeLz2SpeedDescription,
        Self::BuiltInRuntimeSharedPalettesDescription,
        Self::BuiltInRuntimeAlreadyInstalled,
        Self::BuiltInRuntimeAtomicNotice,
        Self::BuiltInRuntimeStaleNotice,
        Self::BuiltInRuntimeCancel,
        Self::BuiltInRuntimeMigrate,
        Self::BuiltInRuntimeInstall,
        Self::BuiltInRuntimeErrorTitle,
        Self::BuiltInRuntimeOk,
        Self::BuiltInRuntimeMigrateLfix3Gen1,
        Self::BuiltInRuntimeMigrateLfix3Gen2,
        Self::BuiltInRuntimeMigrateMap16Stage1,
        Self::BuiltInRuntimeMigrateMap16Stage2,
        Self::BuiltInRuntimeMigrateMap16Stage3,
        Self::BuiltInRuntimeMigrateExAnimationPointers,
        Self::BuiltInRuntimeMigrateExAnimationTable,
        Self::BuiltInRuntimeMigrateLayer2Format100,
        Self::BuiltInRuntimeMigrateLayer2Format101,
        Self::BuiltInRuntimeMigrateLayer2Format102,
        Self::RomLoaderMissingHeaderTitle,
        Self::RomLoaderMissingHeaderQuestion,
        Self::RomLoaderAddHeader,
        Self::RomLoaderCancel,
        Self::RomLoaderOpeningTitle,
        Self::RomLoaderOpeningProgress,
        Self::MwlImportTitle,
        Self::MwlImportReadingFormat,
        Self::MwlImportReadingSidecarsFormat,
        Self::MwlImportMissingPalette,
        Self::MwlImportCommittingFormat,
        Self::MwlImportCommittingNotesFormat,
        Self::MwlImportClose,
        Self::MwlImportInsertedFormat,
        Self::MwlImportFailedFormat,
        Self::MwlBatchImportTitle,
        Self::MwlBatchImportDirectoryFormat,
        Self::MwlBatchImportSummaryFormat,
        Self::MwlBatchImportAllocationSearch,
        Self::MwlBatchImportRangeSeparator,
        Self::MwlBatchImportStart,
        Self::MwlBatchImportCancelNotice,
        Self::MwlBatchImportCancel,
        Self::MwlBatchImportClose,
        Self::MwlBatchImportCancelled,
        Self::MwlBatchImportCompleteFormat,
        Self::MwlBatchImportReadingFormat,
        Self::MwlBatchImportCommittingFormat,
        Self::MwlBatchImportPreparedFormat,
        Self::MwlBatchImportInsertedFormat,
        Self::MwlBatchImportReadFailedFormat,
        Self::MwlBatchImportInsertFailedFormat,
        Self::MwlBatchImportCommitFailedFormat,
        Self::MwlBatchImportDiscardedRead,
        Self::MwlBatchExportProgressTitle,
        Self::MwlBatchExportTemplateFormat,
        Self::MwlBatchExportAtomicNotice,
        Self::MwlBatchExportCancellationRequested,
        Self::MwlBatchExportCancel,
        Self::MwlBatchExportResultTitle,
        Self::MwlBatchExportCompletedFormat,
        Self::MwlBatchExportCancelled,
        Self::MwlBatchExportClose,
        Self::VramPatchTitle,
        Self::VramPatchDescription,
        Self::VramPatchDeferredNotice,
        Self::VramPatchType,
        Self::VramPatchNone,
        Self::VramPatchNoneHelp,
        Self::VramPatchNormal,
        Self::VramPatchNormalHelp,
        Self::VramPatchHd16x9,
        Self::VramPatchHd21x9,
        Self::VramPatchUnknownNotice,
        Self::VramPatchCancel,
        Self::VramPatchOk,
        Self::VramPatchErrorTitle,
        Self::VramPatchStatusNone,
        Self::VramPatchStatusNormal,
        Self::VramPatchStatusHd,
        Self::LegacyBypassTransferCompleteTitle,
        Self::LegacyBypassTransferCompleteFormat,
        Self::LegacyBypassTransferDestinationFallback,
        Self::LegacyBypassTransferErrorTitle,
        Self::LegacyBypassTransferOk,
        Self::VanillaLevelZoomTitle,
        Self::VanillaLevelZoomIn,
        Self::VanillaLevelZoomOut,
        Self::VanillaLevelZoomFilter,
        Self::VanillaLevelConditionalMap16Title,
        Self::VanillaLevelConditionalMap16RuntimeFlag,
        Self::VanillaLevelConditionalMap16AlwaysShow,
        Self::VanillaLevelConditionalMap16RemoveFlag,
        Self::VanillaLevelApply,
        Self::VanillaLevelCancel,
        Self::VanillaLevelDirectMap16RemapTitle,
        Self::VanillaLevelHexSourceDestinationPairs,
        Self::VanillaLevelDirectMap16RemapHelp,
        Self::VanillaLevelBackgroundMap16BankTitle,
        Self::VanillaLevelBackgroundMap16BankHelp,
        Self::VanillaLevelBank,
        Self::VanillaLevelOk,
        Self::VanillaLevelBackgroundTileRemapTitle,
        Self::VanillaLevelBackgroundTileOffset,
        Self::VanillaLevelBackgroundTileRemapHelp,
        Self::VanillaLevelPropertiesTitle,
        Self::VanillaLevelManualEditTitle,
        Self::VanillaLevelLayer1ObjectFormat,
        Self::VanillaLevelLayer2ObjectFormat,
        Self::VanillaLevelSpriteRecordFormat,
        Self::VanillaLevelApplyProperties,
        Self::VanillaLevelSelectEntityForProperties,
        Self::VanillaLevelManualSingleSelection,
        Self::VanillaLevelSpriteTokenFormat,
        Self::VanillaLevelApplyCompleteRecord,
        Self::VanillaLevelSelectEntityForManualEdit,
        Self::VanillaLevelAddStructures,
        Self::VanillaLevelHexFilter,
        Self::VanillaLevelHexNameFilter,
        Self::VanillaLevelClear,
        Self::VanillaLevelChooseStandardObject,
        Self::VanillaLevelHandlerMapUnavailable,
        Self::VanillaLevelStandardDefinitionsUnavailable,
        Self::VanillaLevelSwitchPreviewsUnavailable,
        Self::VanillaLevelStandardObject,
        Self::VanillaLevelAddCustomOscObject,
        Self::VanillaLevelCustomObject,
        Self::VanillaLevelAddExtendedObjects,
        Self::VanillaLevelChooseExtendedObject,
        Self::VanillaLevelExtendedDefinitionsUnavailable,
        Self::VanillaLevelExtendedObject,
        Self::VanillaLevelInsertAfterSelection,
        Self::VanillaLevelApplyScreenJump,
        Self::VanillaLevelApplyScreenExit,
        Self::VanillaLevelApplyObjectFields,
        Self::VanillaLevelApplyRawRecord,
        Self::VanillaLevelRemoveObject,
        Self::VanillaLevelMoveUp,
        Self::VanillaLevelMoveDown,
        Self::VanillaLevelCopy,
        Self::VanillaLevelPasteAfterSelection,
        Self::VanillaLevelPasteMap16Rectangle,
        Self::VanillaLevelExistingSpritesFormat,
        Self::VanillaLevelChooseExistingSprite,
        Self::VanillaLevelChooseExistingSpritePlaceholder,
        Self::VanillaLevelPlacementActive,
        Self::VanillaLevelRawSpriteStream,
        Self::VanillaLevelSpritesStored,
        Self::VanillaLevelAddStandardSprites,
        Self::VanillaLevelChooseStandardSprite,
        Self::VanillaLevelStandardSprite,
        Self::VanillaLevelAddCustomSprites,
        Self::VanillaLevelStageSpriteHeader,
        Self::VanillaLevelReplaceRecord,
        Self::VanillaLevelApplySpriteFields,
        Self::VanillaLevelRemoveSprite,
        Self::VanillaLevelCopyRecord,
        Self::VanillaLevelPasteRecordAfterSelection,
        Self::VanillaLevelPlaceOnCanvas,
        Self::VanillaLevelApplyFields,
        Self::VanillaLevelHeaderCountsFormat,
        Self::VanillaLevelMode,
        Self::VanillaLevelBackgroundPalette,
        Self::VanillaLevelLastScreen,
        Self::VanillaLevelBackgroundColor,
        Self::VanillaLevelSpriteTileset,
        Self::VanillaLevelDefaultMusic,
        Self::VanillaLevelCustomMusicBypass,
        Self::VanillaLevelEnabled,
        Self::VanillaLevelCustomMusicTrack,
        Self::VanillaLevelTimeLimit,
        Self::VanillaLevelCustomTimeBypass,
        Self::VanillaLevelCustomTime,
        Self::VanillaLevelForceTimeReset,
        Self::VanillaLevelForegroundPalette,
        Self::VanillaLevelSpritePalette,
        Self::VanillaLevelObjectTileset,
        Self::VanillaLevelLayer1VerticalScroll,
        Self::VanillaLevelStageHeader,
        Self::VanillaLevelResetStagedValues,
        Self::VanillaLevelResetLayer2Title,
        Self::VanillaLevelResetLayer2Format,
        Self::VanillaLevelResetLayer2Help,
        Self::VanillaLevelResetLayer2Apply,
        Self::VanillaLevelMainEntrance,
        Self::VanillaLevelEntranceExactRecord,
        Self::VanillaLevelPosition,
        Self::VanillaLevelLayer2ScrollPreset,
        Self::VanillaLevelVerticalSettings,
        Self::VanillaLevelScreenMethod,
        Self::VanillaLevelModeScreen,
        Self::VanillaLevelMidwayInstalled,
        Self::VanillaLevelFlags,
        Self::VanillaLevelAdditionalFlags,
        Self::VanillaLevelHighPosition,
        Self::VanillaLevelMidwayNotInstalled,
        Self::VanillaLevelInstallMidway,
        Self::VanillaLevelStageEntrance,
        Self::VanillaLevelResetEntrance,
        Self::VanillaLevelCommitEntrances,
        Self::VanillaLevelCurrentUnavailable,
        Self::VanillaLevelExitTableHelp,
        Self::VanillaLevelScreen,
        Self::VanillaLevelPresent,
        Self::VanillaLevelDestinationFlags,
        Self::VanillaLevelApplyAllExits,
        Self::VanillaLevelResetExits,
        Self::VanillaLevelInvalidExitScreens,
        Self::VanillaLevelInvalidExitSaveHelp,
        Self::VanillaLevelDisableWarningFormat,
        Self::VanillaLevelSaveAnywayQuestion,
        Self::VanillaLevelSaveAnyway,
        Self::VanillaLevelScanExitsTitle,
        Self::VanillaLevelNoInvalidExits,
        Self::VanillaLevelInvalidExitFixHelp,
        Self::VanillaLevelLayer2,
        Self::VanillaLevelLayer2TilemapStatusFormat,
        Self::VanillaLevelMap16Word,
        Self::VanillaLevelStageSelectedTile,
        Self::VanillaLevelLayer2PaintHelp,
        Self::VanillaLevelSharedBackgroundReadOnly,
        Self::VanillaLevelLayer2ObjectCountFormat,
        Self::VanillaLevelBackgroundCanvas,
        Self::VanillaLevelCanvasPlaceHelp,
        Self::VanillaLevelCanvasSelectHelp,
        Self::VanillaLevelDuplicateSelected,
        Self::VanillaLevelDeleteSelected,
        Self::VanillaLevelGamePixels,
        Self::VanillaLevelViewport,
        Self::VanillaLevelSelectionOverGame,
        Self::VanillaLevelSelectionOverGameHelp,
        Self::VanillaLevelCanvasTool,
        Self::VanillaLevelSelectMove,
        Self::VanillaLevelPlaceObject,
        Self::VanillaLevelPlaceSprite,
        Self::VanillaLevelPaintLayer2Tile,
        Self::VanillaLevelPlaceLayer2Object,
        Self::VanillaLevelZoom,
        Self::VanillaLevelReset,
        Self::VanillaLevelCamera,
        Self::VanillaLevelScreenMinus,
        Self::VanillaLevelScreenPlus,
        Self::VanillaLevelEntrance,
        Self::VanillaLevelObjectPlacementWarningFormat,
        Self::VanillaLevelSpriteCountFormat,
        Self::VanillaLevelSpriteCountWarning,
        Self::VanillaLevelVerticalFireballWarning,
        Self::VanillaLevelSaveTitle,
        Self::VanillaLevelSaveBeforeContinuing,
        Self::VanillaLevelSave,
        Self::VanillaLevelDiscard,
        Self::VanillaLevelSaveBeforeExitFormat,
        Self::VanillaLevelObjectFormat,
        Self::VanillaLevelNoSelectedObject,
        Self::VanillaLevelNativeScreenExit,
        Self::VanillaLevelSourceScreen,
        Self::VanillaLevelScreenExitEncodingHelp,
        Self::VanillaLevelScreenJumpFormat,
        Self::VanillaLevelLowByteFirst,
        Self::VanillaLevelHighByteFirst,
        Self::VanillaLevelFirstEncodedComponent,
        Self::VanillaLevelSecondEncodedComponent,
        Self::VanillaLevelAdvanceScreen,
        Self::VanillaLevelPreviewZoomOut,
        Self::VanillaLevelPreviewZoomIn,
        Self::VanillaLevelPreviewZoomDefault,
        Self::VanillaLevelSpriteMemory,
        Self::VanillaLevelSpriteBuoyancy1,
        Self::VanillaLevelWaterLavaInteraction,
        Self::VanillaLevelSpriteBuoyancy2,
        Self::VanillaLevelWaterLavaDisableLayerInteraction,
        Self::VanillaLevelRecordBytes,
        Self::VanillaLevelSpriteNumber,
        Self::VanillaLevelX,
        Self::VanillaLevelYLowBits,
        Self::VanillaLevelExtraBits,
    ];

    #[must_use]
    pub const fn english(self) -> &'static str {
        match self {
            Self::MwlDocumentEditorTitle => "Portable MWL Editor",
            Self::MwlDocumentVersionFormat => "Preserved MWL version: {version}",
            Self::MwlDocumentFlagsHex => "Flags (hex)",
            Self::MwlDocumentAttributionNotice => "Attribution (exactly 48 hexadecimal bytes):",
            Self::MwlDocumentLevelNumberNotice => {
                "Level number (hex; blank if header is not exact 64 bytes)"
            }
            Self::MwlDocumentApplyHeader => "Apply recovered MWL header fields",
            Self::MwlDocumentLayer3Heading => "Layer 3 expanded settings",
            Self::MwlDocumentLayer3Unavailable => {
                "This MWL has no exact expanded-settings section."
            }
            Self::MwlDocumentLayer3Enable => "Enable custom Layer 3 tilemap",
            Self::MwlDocumentLayer3File => "GFX/ExGFX file",
            Self::MwlDocumentLengthSelector => "Length selector",
            Self::MwlDocumentDestinationSelector => "Destination selector",
            Self::MwlDocumentExpandedMode => "Expanded mode (8 hex digits)",
            Self::MwlDocumentApplyLayer3 => "Apply Layer 3 expanded settings",
            Self::MwlDocumentEntranceHeading => "Packed entrance settings",
            Self::MwlDocumentEntranceNotice => {
                "Lossless Lunar Magic fields. Bit meanings vary by level mode and installed runtime."
            }
            Self::MwlDocumentMainPosition => "Main position",
            Self::MwlDocumentMainVertical => "Main vertical settings",
            Self::MwlDocumentMainScreenMethod => "Main screen/method",
            Self::MwlDocumentMainModeScreen => "Main level mode/screen",
            Self::MwlDocumentMainFlags => "Main flags",
            Self::MwlDocumentMainHighPosition => "Main high position",
            Self::MwlDocumentMainAdditionalFlags => "Main additional flags",
            Self::MwlDocumentMidwayPosition => "Midway position",
            Self::MwlDocumentMidwayFlags => "Midway flags",
            Self::MwlDocumentMidwayHighPosition => "Midway high position",
            Self::MwlDocumentMidwayAdditionalFlags => "Midway additional flags",
            Self::MwlDocumentSeparateLayer2Scroll => "Use separate Layer 2 scroll settings",
            Self::MwlDocumentOriginalScrollPreset => "Original paired preset",
            Self::MwlDocumentHorizontalSelector => "Horizontal selector",
            Self::MwlDocumentVerticalSelector => "Vertical selector",
            Self::MwlDocumentSpriteSpawning => "Sprite spawning",
            Self::MwlDocumentVerticalSpawnRange => "Vertical spawn range for horizontal levels",
            Self::MwlDocumentSmartSpawn => "Enable Smart Spawn (spawn on scroll)",
            Self::MwlDocumentSectionLevelHeader => "Level header",
            Self::MwlDocumentSectionLayer1 => "Layer 1",
            Self::MwlDocumentSectionLayer2 => "Layer 2",
            Self::MwlDocumentSectionSprites => "Sprites",
            Self::MwlDocumentSectionPalette => "Palette",
            Self::MwlDocumentSectionSecondaryExits => "Secondary exits",
            Self::MwlDocumentSectionExAnimation => "ExAnimation",
            Self::MwlDocumentSectionExpandedHeader => "Expanded header",
            Self::MwlDocumentSectionLengthFormat => "Current section length: {length} bytes",
            Self::MwlDocumentSectionBytes => "Opaque section bytes:",
            Self::MwlDocumentReplaceSection => "Replace selected section atomically",
            Self::MwlDocumentUndo => "Undo",
            Self::MwlDocumentRedo => "Redo",
            Self::MwlDocumentSave => "Save",
            Self::MwlDocumentModified => "Modified",
            Self::MwlDocumentSaved => "Saved",
            Self::MwlDocumentDiscardTitle => "Unsaved MWL document",
            Self::MwlDocumentUnsavedNotice => "Discard unsaved MWL changes?",
            Self::MwlDocumentCancel => "Cancel",
            Self::MwlDocumentDiscard => "Discard",
            Self::MwlDocumentErrorTitle => "MWL editor error",
            Self::MwlDocumentOk => "OK",
            Self::MwlObjectHeading => "Typed Layer 1 objects",
            Self::MwlObjectCountFormat => "{count} ordered standard/extended/custom object records",
            Self::MwlObjectHeader => "Exact five-byte legacy level header:",
            Self::MwlObjectStageHeader => "Stage exact header",
            Self::MwlObjectRecord => "Object record (3–8 hexadecimal bytes):",
            Self::MwlObjectCommit => "Commit typed Layer 1 objects",
            Self::MwlObjectRecoveredFields => {
                "Recovered packed fields (hex; coordinates are orientation-neutral nibbles):"
            }
            Self::MwlObjectCommandId => "Command ID",
            Self::MwlObjectParameter => "Parameter",
            Self::MwlObjectFirstCoordinate => "First coordinate",
            Self::MwlObjectSecondCoordinate => "Second coordinate",
            Self::MwlObjectAdvancesScreen => "Advances screen",
            Self::MwlObjectStageFields => "Stage recovered fields",
            Self::MwlObjectJumpEncodingFormat => "Screen-jump encoding: {encoding}",
            Self::MwlObjectResolvedScreenFormat => "Resolved screen: {screen}{suffix}",
            Self::MwlObjectOutsideScreenSuffix => " (outside 00-1F; retained losslessly)",
            Self::MwlObjectJumpTarget => "Packed jump target (hex)",
            Self::MwlObjectStageJumpTarget => "Stage screen-jump target",
            Self::MwlInsertBefore => "Insert before",
            Self::MwlReplace => "Replace",
            Self::MwlDelete => "Delete",
            Self::MwlMoveUp => "Move up",
            Self::MwlMoveDown => "Move down",
            Self::MwlSpriteHeading => "Typed sprite stream",
            Self::MwlSpriteExpanded => "Expanded sprite framing",
            Self::MwlSpriteTokenCountFormat => "{count} ordered sprite tokens",
            Self::MwlSpriteStageHeader => "Stage header",
            Self::MwlSpriteRecordBytes => "Record bytes",
            Self::MwlSpriteUpperYToken => "Upper-Y token",
            Self::MwlSpriteControlToken => "Control token",
            Self::MwlSpriteCommit => "Commit typed sprite stream",
            Self::MwlSpriteLengthNotice => {
                "Sprite record-length interpretation (table, sprite ID, bytes; hex):"
            }
            Self::MwlSpriteSetLength => "Set length",
            Self::MwlSpriteResetLengths => "Reset standard lengths",
            Self::MwlSpriteRecoveredFields => {
                "Recovered `yyyyEESY / XXXXssss / NNNNNNNN` fields (hex):"
            }
            Self::MwlSpriteYLow => "Y low 5 bits",
            Self::MwlSpriteExtraBits => "Extra bits",
            Self::MwlSpriteScreen => "Screen",
            Self::MwlSpriteX => "X",
            Self::MwlSpriteNumber => "Sprite number",
            Self::MwlSpriteStageFields => "Stage recovered sprite fields",
            Self::MwlOptionalImportHeading => "Typed palette and ExAnimation import",
            Self::MwlOptionalMaximumRecords => "Maximum ExAnimation records:",
            Self::MwlOptionalImport => "Import from MWL…",
            Self::MwlOptionalInterpret => "Interpret current sections…",
            Self::MwlOptionalImportNotice => {
                "Select a source MWL and its exact 256-byte size-mode table. Other target sections are preserved."
            }
            Self::MwlOptionalHeading => "Typed MWL optional assets",
            Self::MwlOptionalPalette => "Palette",
            Self::MwlOptionalExAnimation => "ExAnimation",
            Self::MwlOptionalPaletteMetadata => "Palette metadata",
            Self::MwlOptionalExAnimationMetadata => "ExAnimation metadata",
            Self::MwlOptionalColorFormat => "Color {index} / BGR555 {value}",
            Self::MwlOptionalFeaturesHeading => "Super GFX Bypass animation options",
            Self::MwlOptionalPaletteAnimation => "Palette animation",
            Self::MwlOptionalVanillaAnimation => "Vanilla animated tiles",
            Self::MwlOptionalGlobalAnimation => "Global ExAnimation",
            Self::MwlOptionalLevelAnimation => "Level ExAnimation",
            Self::MwlOptionalApplyFeatures => "Apply animation options",
            Self::MwlOptionalPreservedNibbleFormat => "Preserved unrelated low nibble: {value}",
            Self::MwlOptionalCreateAnimation => "Create empty ExAnimation section",
            Self::MwlOptionalSetting => "Setting",
            Self::MwlOptionalHeader => "Header",
            Self::MwlOptionalApplyGlobals => "Apply ExAnimation globals",
            Self::MwlOptionalTrigger => "Trigger",
            Self::MwlOptionalTriggerEnabled => "Trigger enabled",
            Self::MwlOptionalApplyTrigger => "Apply trigger",
            Self::MwlOptionalKind => "Kind",
            Self::MwlOptionalDestination => "Destination",
            Self::MwlOptionalDestinationFlag => "Destination flag",
            Self::MwlOptionalSourceWords => "Source words, one frame per line",
            Self::MwlOptionalAppendRecord => "Append record",
            Self::MwlOptionalReplaceRecord => "Replace record",
            Self::MwlOptionalRemoveRecord => "Remove record",
            Self::MwlOptionalFrameHeading => "Semantic frame edit",
            Self::MwlOptionalSourceWordList => "Source word(s)",
            Self::MwlOptionalMoveBefore => "Move before ",
            Self::MwlOptionalInsertFrame => "Insert frame",
            Self::MwlOptionalReplaceFrame => "Replace frame",
            Self::MwlOptionalRemoveFrame => "Remove frame",
            Self::MwlOptionalMoveFrame => "Move frame",
            Self::MwlOptionalWord0 => "Word 0",
            Self::MwlOptionalWord1 => "Word 1",
            Self::MwlOptionalApplyMetadata => "Apply metadata",
            Self::Map16SidecarEditorTitle => "Native Map16 Sidecar Editor",
            Self::Map16SidecarInterpretTitle => "Map16 sidecar interpretation",
            Self::Map16SidecarM16Kind => ".m16 — exact 0x2000-byte custom-object table",
            Self::Map16SidecarS16Kind => ".s16 — sparse sprite Map16 workspace",
            Self::Map16SidecarCancel => "Cancel",
            Self::Map16SidecarOpen => "Open",
            Self::Map16SidecarM16Exact => ".m16 exact",
            Self::Map16SidecarS16Canonical => ".s16 sparse canonical",
            Self::Map16SidecarSummaryFormat => {
                "Kind: {kind}; raw dwords: {count}; 16×16 definitions: {tile_count}; save bytes: {encoded_len}"
            }
            Self::Map16SidecarRawEntry => "Raw entry",
            Self::Map16SidecarRawDword => "Raw little-endian dword (hex)",
            Self::Map16SidecarApplyRaw => "Apply raw entry",
            Self::Map16SidecarDefinitionFormat => "Current decoded definition {index}",
            Self::Map16SidecarQuadrant => "Quadrant",
            Self::Map16SidecarTile => "8×8 tile (hex)",
            Self::Map16SidecarPalette => "Palette",
            Self::Map16SidecarPriority => "Priority",
            Self::Map16SidecarHorizontalFlip => "Horizontal flip",
            Self::Map16SidecarVerticalFlip => "Vertical flip",
            Self::Map16SidecarApplySubtile => "Apply decoded subtile",
            Self::Map16SidecarUndo => "Undo",
            Self::Map16SidecarRedo => "Redo",
            Self::Map16SidecarSave => "Save",
            Self::Map16SidecarModified => "Modified",
            Self::Map16SidecarSaved => "Saved",
            Self::Map16SidecarDiscardTitle => "Unsaved native Map16 sidecar",
            Self::Map16SidecarDiscardNotice => "Discard unsaved raw-entry changes?",
            Self::Map16SidecarDiscard => "Discard",
            Self::Map16SidecarErrorTitle => "Native Map16 sidecar error",
            Self::Map16SidecarOk => "OK",
            Self::ToolbarEditorTitle => "Customize Toolbar",
            Self::ToolbarEditorNotice => {
                "Buttons are shown from top to bottom. Text keys follow the active language catalog."
            }
            Self::ToolbarEditorDefaultNotice => {
                "The built-in toolbar is active. Add a button to create a custom layout."
            }
            Self::ToolbarEditorMoveUp => "Move up",
            Self::ToolbarEditorMoveDown => "Move down",
            Self::ToolbarEditorRemove => "Remove",
            Self::ToolbarEditorAddButton => "Add Button",
            Self::ToolbarEditorAddSeparator => "Add Separator",
            Self::ToolbarEditorApply => "Apply",
            Self::ToolbarEditorUseDefault => "Use Built-in Toolbar",
            Self::ToolbarEditorCancel => "Cancel",
            Self::ToolbarEditorSeparator => "Separator",
            Self::RestoreAutomaticTitle => "Automatic Restore Point",
            Self::RestoreInterval => "Create a full point after this many deltas",
            Self::RestoreDaily => "Create one full point per day",
            Self::RestoreDestructive => "Create a full point before destructive ROM operations",
            Self::RestoreContinuityNotice => {
                "A ROM timestamp or checksum continuity break always forces a full point."
            }
            Self::RestoreAppend => "Append",
            Self::RestoreCancel => "Cancel",
            Self::RestoreAutomaticComplete => "Automatic restore point appended.",
            Self::RestoreArchiveFormat => "Archive: {path}",
            Self::RestoreOriginalFormat => "Original: {path}",
            Self::RestoreTargetFormat => "Restore target: {path}",
            Self::RestoreId => "ID",
            Self::RestoreDateTime => "Date and time",
            Self::RestoreType => "Type",
            Self::RestoreDescription => "Description",
            Self::RestoreAm => "AM",
            Self::RestorePm => "PM",
            Self::RestoreReversion => "Reversion",
            Self::RestoreFull => "Full",
            Self::RestoreDelta => "Delta",
            Self::RestoreReplaceWarning => {
                "The selected existing ROM will be replaced atomically. Close it in the editor first."
            }
            Self::RestoreRunningTitle => "Restoring ROM",
            Self::RestorePointFormat => "Restore point: {id}",
            Self::RestoreRunningTargetFormat => "Target: {path}",
            Self::RestoreRunningNotice => "Validating and publishing the reconstructed ROM…",
            Self::RestoreCompleteTitle => "ROM restored",
            Self::RestoreErrorTitle => "Restore-point error",
            Self::RestoreOk => "OK",
            Self::RestoreAssociatedOne => " and 1 associated file",
            Self::RestoreAssociatedManyFormat => " and {count} associated files",
            Self::RestoreCompleteFormat => {
                "Restored point {id} to {path} ({bytes} bytes{associated})."
            }
            Self::LevelUsageOutputFormat => "Output: {path}",
            Self::LevelUsageProgressTitle => "Analyzing Level Usage",
            Self::LevelUsageLevelsFormat => "{completed} / {total} levels",
            Self::LevelUsageScanningFormat => "Scanning level {level}…",
            Self::LevelUsageCancel => "Cancel",
            Self::LevelUsageCompleteFormat => {
                "Created {path} ({bytes} bytes, {diagnostics} diagnostics)."
            }
            Self::LevelUsageCompleteTitle => "Level usage analysis complete",
            Self::LevelUsageErrorTitle => "Level usage analysis error",
            Self::LevelUsageOk => "OK",
            Self::GraphicsMigrationAllocationNotice => {
                "End-exclusive logical-PC allocation range (hexadecimal)."
            }
            Self::GraphicsMigrationStart => "Start",
            Self::GraphicsMigrationEnd => "End",
            Self::GraphicsMigrationErrorTitle => "Graphics migration error",
            Self::GraphicsMigrationOk => "OK",
            Self::ShortcutEditorTitle => "Keyboard Shortcuts",
            Self::ShortcutEditorGestureNotice => {
                "Use portable gestures such as primary+s, primary+shift+s, alt+f4, or escape."
            }
            Self::ShortcutEditorPrimaryNotice => {
                "Primary means Command on macOS and Ctrl on other platforms."
            }
            Self::ShortcutEditorRemove => "Remove",
            Self::ShortcutEditorAdd => "Add shortcut",
            Self::ShortcutEditorApply => "Apply",
            Self::ShortcutEditorClearAll => "Clear All",
            Self::ShortcutEditorCancel => "Cancel",
            Self::PathEditorTitle => "Portable Overworld Path Editor",
            Self::PathEditorPolicyTitle => "Path validation policy",
            Self::PathEditorReciprocalPolicy => {
                "Require reciprocal edges unless explicitly one-way"
            }
            Self::PathEditorCancel => "Cancel",
            Self::PathEditorOpen => "Open",
            Self::PathEditorNodes => "Nodes",
            Self::PathEditorEdges => "Edges",
            Self::PathEditorUndo => "Undo",
            Self::PathEditorRedo => "Redo",
            Self::PathEditorSave => "Save",
            Self::PathEditorModified => "Modified",
            Self::PathEditorSaved => "Saved",
            Self::PathEditorNode => "Node",
            Self::PathEditorEdge => "Edge",
            Self::PathEditorUpsertNode => "Upsert node",
            Self::PathEditorUpsertEdge => "Upsert edge",
            Self::PathEditorRemoveSelected => "Remove selected",
            Self::PathEditorStableId => "Stable ID (hex)",
            Self::PathEditorX => "X (hex)",
            Self::PathEditorY => "Y (hex)",
            Self::PathEditorLevel => "Level (hex, blank = none)",
            Self::PathEditorRawFlags => "Raw flags (hex)",
            Self::PathEditorFromNode => "From node (hex)",
            Self::PathEditorToNode => "To node (hex)",
            Self::PathEditorExit => "Exit (hex, blank = none)",
            Self::PathEditorOneWay => "Deliberately one-way",
            Self::PathEditorReciprocalPair => "Apply/remove reciprocal pair atomically",
            Self::PathEditorReverseExit => "Reverse exit (hex, blank = none)",
            Self::PathEditorReverseRawFlags => "Reverse raw flags (hex)",
            Self::PathEditorDiscardTitle => "Unsaved overworld paths",
            Self::PathEditorDiscardNotice => "Discard unsaved path changes?",
            Self::PathEditorDiscard => "Discard",
            Self::PathEditorErrorTitle => "Path editor error",
            Self::PathEditorOk => "OK",
            Self::PathEditorDirectionUp => "Up",
            Self::PathEditorDirectionRight => "Right",
            Self::PathEditorDirectionDown => "Down",
            Self::PathEditorDirectionLeft => "Left",
            Self::ExternalToolRunningTitleFormat => "External tool {tool} running",
            Self::ExternalToolWaitingFormat => "Waiting for external tool {tool}",
            Self::ExternalToolStop => "Stop",
            Self::ExternalToolAllowTitle => "Allow external tool?",
            Self::ExternalToolIdFormat => "Tool ID: {tool}",
            Self::ExternalToolExecutableFormat => "Executable: {path}",
            Self::ExternalToolWorkingDirectoryFormat => "Working directory: {path}",
            Self::ExternalToolInherited => "<inherited>",
            Self::ExternalToolArgumentsNotice => {
                "Arguments are passed directly without a command shell:"
            }
            Self::ExternalToolArgumentFormat => "argument[{index}] = {argument}",
            Self::ExternalToolDeny => "Deny",
            Self::ExternalToolRun => "Run",
            Self::ExternalToolCompletedFormat => "External tool {tool} completed successfully",
            Self::ExternalToolStoppedFormat => "External tool {tool} stopped",
            Self::NativeLevelDocumentTitle => "Native Level Stream Editor",
            Self::NativeLevelDocumentSourceFormat => "Source level: {level}  |  {framing} framing",
            Self::NativeLevelDocumentExpandedFraming => "expanded",
            Self::NativeLevelDocumentLegacyFraming => "legacy",
            Self::NativeLevelDocumentLegacyHeaderFormat => "Legacy header: {bytes}",
            Self::NativeLevelDocumentUndo => "Undo",
            Self::NativeLevelDocumentRedo => "Redo",
            Self::NativeLevelDocumentSave => "Save",
            Self::NativeLevelDocumentApplySpriteHeader => "Apply sprite header",
            Self::NativeLevelDocumentModified => "Modified",
            Self::NativeLevelDocumentSaved => "Saved",
            Self::NativeLevelDocumentDiscardTitle => "Unsaved native level",
            Self::NativeLevelDocumentDiscardNotice => {
                "Discard unsaved native-level stream changes?"
            }
            Self::NativeLevelDocumentCancel => "Cancel",
            Self::NativeLevelDocumentDiscard => "Discard",
            Self::NativeLevelDocumentErrorTitle => "Native-level editor error",
            Self::NativeLevelDocumentOk => "OK",
            Self::NativeLevelDocumentIndex => "Index",
            Self::NativeLevelDocumentObjectsFormat => "Objects ({count})",
            Self::NativeLevelDocumentLoadSelected => "Load selected",
            Self::NativeLevelDocumentInsert => "Insert",
            Self::NativeLevelDocumentReplace => "Replace",
            Self::NativeLevelDocumentRemove => "Remove",
            Self::NativeLevelDocumentApplyObjectFields => "Apply object fields",
            Self::NativeLevelDocumentCopy => "Copy",
            Self::NativeLevelDocumentPaste => "Paste",
            Self::NativeLevelDocumentSpriteTokensFormat => "Sprite tokens ({count})",
            Self::NativeLevelDocumentLoadRecord => "Load record",
            Self::NativeLevelDocumentInsertRecord => "Insert record",
            Self::NativeLevelDocumentReplaceRecord => "Replace record",
            Self::NativeLevelDocumentRemoveToken => "Remove token",
            Self::NativeLevelDocumentApplySpriteFields => "Apply sprite fields",
            Self::NativeLevelDocumentCopyRecord => "Copy record",
            Self::NativeLevelDocumentPasteRecord => "Paste record",
            Self::NativeLevelDocumentObjectCommand => "Command",
            Self::NativeLevelDocumentObjectParameter => "Parameter",
            Self::NativeLevelDocumentObjectFirstCoordinate => "First coordinate",
            Self::NativeLevelDocumentObjectSecondCoordinate => "Second coordinate",
            Self::NativeLevelDocumentScreen => "Screen",
            Self::NativeLevelDocumentObjectPerpendicularHigh => "Perpendicular coordinate high bit",
            Self::NativeLevelDocumentSpriteNumber => "Sprite number",
            Self::NativeLevelDocumentSpriteX => "X",
            Self::NativeLevelDocumentSpriteYLow => "Y (low 5 bits)",
            Self::NativeLevelDocumentSpriteExtraBits => "Extra bits",
            Self::NativeLevelDocumentSpriteMemory => "Sprite memory",
            Self::NativeLevelDocumentSpriteBuoyancy1 => "Sprite buoyancy 1",
            Self::NativeLevelDocumentSpriteInteraction => "Water/lava interaction",
            Self::NativeLevelDocumentSpriteBuoyancy2 => "Sprite buoyancy 2",
            Self::NativeLevelDocumentSpriteDisableLayerInteraction => {
                "Water/lava; disable Layer 2/3 interaction"
            }
            Self::NativeAssetsTitle => "Native Level Assets Editor",
            Self::NativeAssetsOpenTitle => "Open native level assets",
            Self::NativeAssetsMaximumRecordsNotice => {
                "Maximum ExAnimation records from the matching revision profile:"
            }
            Self::NativeAssetsCancel => "Cancel",
            Self::NativeAssetsOpen => "Open",
            Self::NativeAssetsUndo => "Undo",
            Self::NativeAssetsRedo => "Redo",
            Self::NativeAssetsSaveAggregate => "Save aggregate",
            Self::NativeAssetsModified => "Modified",
            Self::NativeAssetsSaved => "Saved",
            Self::NativeAssetsDiscardTitle => "Unsaved native assets",
            Self::NativeAssetsDiscardNotice => {
                "Discard changes across all native level-asset domains?"
            }
            Self::NativeAssetsDiscard => "Discard",
            Self::NativeAssetsErrorTitle => "Native-assets editor error",
            Self::NativeAssetsOk => "OK",
            Self::NativeAssetsTabLevel => "Level",
            Self::NativeAssetsTabLayer2 => "Layer 2",
            Self::NativeAssetsTabPalette => "Palette",
            Self::NativeAssetsTabExAnimation => "ExAnimation",
            Self::NativeAssetsTabSettings => "Settings",
            Self::NativeAssetsLevelSourceFormat => "Source slot {slot}; header {header}",
            Self::NativeAssetsLevelHeader => "Level header",
            Self::NativeAssetsLevelMode => "Level mode",
            Self::NativeAssetsBackgroundPalette => "Background palette",
            Self::NativeAssetsLastScreen => "Last screen",
            Self::NativeAssetsBackgroundColor => "Background color",
            Self::NativeAssetsSpriteTileset => "Sprite tileset",
            Self::NativeAssetsDefaultMusic => "Default music selector",
            Self::NativeAssetsTimeLimit => "Time limit selector",
            Self::NativeAssetsCustomTimeBypass => "Custom time bypass",
            Self::NativeAssetsEnabled => "Enabled",
            Self::NativeAssetsCustomTimeHex => "Custom time (hex)",
            Self::NativeAssetsForceTimeReset => "Force time reset",
            Self::NativeAssetsForegroundPalette => "Foreground palette",
            Self::NativeAssetsSpritePalette => "Sprite palette",
            Self::NativeAssetsObjectTileset => "Object tileset",
            Self::NativeAssetsLayer1VerticalScroll => "Layer 1 vertical scroll",
            Self::NativeAssetsStageHeader => "Stage header changes",
            Self::NativeAssetsResetHeader => "Reset staged values",
            Self::NativeAssetsMoveUp => "Move up",
            Self::NativeAssetsMoveDown => "Move down",
            Self::NativeAssetsApplyHeader => "Apply header",
            Self::NativeAssetsVerticalSpawnRange => "Vertical spawn range for horizontal levels",
            Self::NativeAssetsSmartSpawn => "Enable Smart Spawn (spawn on scroll)",
            Self::NativeAssetsApplySpawn => "Apply spawn settings",
            Self::NativeAssetsSpawnUnavailable => {
                "Spawn settings require an authenticated current Lfix3 runtime."
            }
            Self::NativeAssetsPaletteColorFormat => "Color {index} / {value}",
            Self::NativeAssetsPaletteOwnershipEditable => "Ownership: editable",
            Self::NativeAssetsPaletteOwnershipFixed => "Ownership: fixed (read-only)",
            Self::NativeAssetsPaletteOwnershipExAnimationFormat => {
                "Ownership: ExAnimation record {record} (read-only)"
            }
            Self::NativeAssetsPaletteOwnershipInvalid => "Ownership: invalid (read-only)",
            Self::NativeAssetsPaletteCopyColor => "Copy color",
            Self::NativeAssetsPalettePasteColor => "Paste color",
            Self::NativeAssetsPaletteCopyRow => "Copy row",
            Self::NativeAssetsPalettePasteRow => "Paste row",
            Self::NativeAssetsPaletteShortcutNotice => {
                "Ctrl+left/right copies or pastes a color; add Alt for its complete row."
            }
            Self::NativeAssetsLayer2ObjectsFormat => "Layer 2 objects ({count})",
            Self::NativeAssetsLayer2TilemapFormat => "Layer 2 tilemap ({count} words)",
            Self::NativeAssetsLayer2InstalledDescriptorFormat => {
                "Installed descriptor ${descriptor} · active Map16 bank ${bank}"
            }
            Self::NativeAssetsLayer2LegacyDescriptor => {
                "Pristine/legacy descriptor · active Map16 bank $0"
            }
            Self::NativeAssetsLayer2SelectionNotice => {
                "Click a Map16 cell, or Shift-click a second cell to select a rectangle. Applying fills every selected cell with the complete 16-bit tile word."
            }
            Self::NativeAssetsLayer2SelectionFormat => {
                "Canvas selection: ({ax}, {ay}) to ({cx}, {cy}) · {count} {unit}"
            }
            Self::NativeAssetsLayer2SelectionOne => "cell",
            Self::NativeAssetsLayer2SelectionMany => "cells",
            Self::NativeAssetsLayer2StorageIndex => "Storage index",
            Self::NativeAssetsLayer2ClearSelection => "Clear canvas selection",
            Self::NativeAssetsLayer2RemapTitle => "Remap Map16 tiles",
            Self::NativeAssetsLayer2RemapNotice => {
                "Enter Lunar Magic source,destination pairs using displayed $8000–$FFFF values. Ranges and the +, −, M, and R prefixes are supported."
            }
            Self::NativeAssetsLayer2GlobalOffset => "Global offset",
            Self::NativeAssetsLayer2SelectionOnly => "Selected rectangle only",
            Self::NativeAssetsLayer2ApplyRemap => "Apply remap",
            Self::NativeAssetsLayer2RemapHelp => {
                "Apply the complete program as one undoable edit. Cross-bank mappings persist when this ROM profile supplies Lunar Magic's installed descriptor table; pristine/legacy layouts reject them before mutation."
            }
            Self::NativeAssetsLayer2TileWord => "16-bit tile word",
            Self::NativeAssetsLayer2Load => "Load",
            Self::NativeAssetsLayer2FillSelectionFormat => "Fill {count} selected cells",
            Self::NativeAssetsLayer2ApplyTile => "Apply tile",
            Self::NativeAssetsLayer2FloodCursor => "Flood fill from cursor",
            Self::NativeAssetsLayer2FloodHelp => {
                "Replace the four-connected region matching the cursor's complete 16-bit word. Lunar Magic normalizes the replacement to a 12-bit Map16 index."
            }
            Self::NativeAssetsLayer2MoveSelection => "Move selection",
            Self::NativeAssetsLayer2MoveHelp => {
                "Move the complete rectangle by one Map16 cell as one undoable edit."
            }
            Self::NativeAssetsLayer2ResizeSelection => "Resize selection",
            Self::NativeAssetsLayer2ResizeHelp => {
                "Grow (+) or shrink (−) this edge by one cell, repeating the original selection pattern from the resized top-left corner."
            }
            Self::NativeAssetsLayer2CapturePattern => "Capture fill pattern",
            Self::NativeAssetsLayer2CapturePatternHelp => {
                "Retain the selected rectangle as a visual row-major Map16 pattern. Then click any destination cell and apply it to that connected region."
            }
            Self::NativeAssetsLayer2FloodCaptured => "Flood fill with captured pattern",
            Self::NativeAssetsLayer2FloodPatternFormat => {
                "Flood fill with {width}×{height} pattern"
            }
            Self::NativeAssetsLayer2PatternHelp => {
                "Repeat the captured rectangle from the connected region's minimum X/Y corner, matching Lunar Magic's pattern anchoring."
            }
            Self::NativeAssetsLayer2CopySelection => "Copy selection",
            Self::NativeAssetsLayer2CutSelection => "Cut selection",
            Self::NativeAssetsLayer2PasteAnchor => "Paste at anchor",
            Self::NativeAssetsLayer2CellHelpFormat => {
                "Canvas ({x}, {y}) · storage index ${index} · word ${word}"
            }
            Self::NativeAssetsAnimationRecordsFormat => "Records ({count})",
            Self::NativeAssetsAnimationKind => "Kind",
            Self::NativeAssetsAnimationTrigger => "Trigger",
            Self::NativeAssetsAnimationDestination => "Destination",
            Self::NativeAssetsAnimationDestinationFlag => "Destination flag",
            Self::NativeAssetsAnimationSourceWords => "Source words, one frame per line",
            Self::NativeAssetsAnimationAppend => "Append",
            Self::NativeAssetsAnimationReplace => "Replace",
            Self::NativeAssetsAnimationRemove => "Remove",
            Self::NativeAssetsAnimationSetting => "Setting",
            Self::NativeAssetsAnimationHeader => "Header",
            Self::NativeAssetsAnimationApplySlots => "Apply slot settings",
            Self::NativeAssetsAnimationEnabled => "Enabled",
            Self::NativeAssetsAnimationApplyTrigger => "Apply trigger",
            Self::NativeAssetsAnimationCopyRecord => "Copy record",
            Self::NativeAssetsAnimationPasteRecord => "Paste record",
            Self::NativeAssetsAnimationFramePrefix => "Frame ",
            Self::NativeAssetsAnimationCopyFrame => "Copy frame",
            Self::NativeAssetsAnimationPasteFrame => "Paste frame",
            Self::NativeAssetsSettingsUnavailable => {
                "This aggregate has no expanded-settings record."
            }
            Self::NativeAssetsSettingsLayer3Title => "Custom Layer 3 tilemap graphics",
            Self::NativeAssetsSettingsLayer3Enable => "Enable custom Layer 3 tilemap",
            Self::NativeAssetsSettingsGfxFile => "GFX/ExGFX file",
            Self::NativeAssetsSettingsLengthSelector => "Length selector",
            Self::NativeAssetsSettingsDestinationSelector => "Destination selector",
            Self::NativeAssetsSettingsApplyLayer3 => "Apply Layer 3 tilemap settings",
            Self::NativeAssetsSettingsExpandedMode => "Expanded mode",
            Self::NativeAssetsSettingsExpandedModeNotice => {
                "Exact 32-bit mode packed from the high nibbles of words 8–F."
            }
            Self::NativeAssetsSettingsApplyExpandedMode => "Apply Layer 3 expanded mode",
            Self::NativeAssetsSettingsBypassTitle => "Super GFX Bypass",
            Self::NativeAssetsSettingsBypassEnable => "Use per-level GFX/ExGFX files",
            Self::NativeAssetsSettingsApplyBypass => "Apply Super GFX bypass",
            Self::NativeAssetsSettingsBoundaryTitle => "Sprite boundary interaction",
            Self::NativeAssetsSettingsBoundaryAir => {
                "Sprites beyond level boundaries interact with air instead of water"
            }
            Self::NativeAssetsSettingsBoundaryNotice => {
                "Lunar Magic recommends enabling this for tide levels."
            }
            Self::NativeAssetsSettingsApplyBoundary => "Apply sprite boundary interaction",
            Self::NativeAssetsSettingsRawWordsNotice => {
                "Raw expanded words (unproven fields remain editable and lossless):"
            }
            Self::NativeAssetsSettingsWordFormat => "Word {index}",
            Self::NativeAssetsSettingsApplyWords => "Apply all words",
            Self::NativeAssetsSettingsAnimationOptions => "Animation options",
            Self::NativeAssetsSettingsAnimationUnavailable => {
                "This profile does not declare installed animation-feature storage."
            }
            Self::NativeAssetsSettingsPaletteAnimation => "Palette animation",
            Self::NativeAssetsSettingsVanillaAnimation => "Vanilla animated tiles",
            Self::NativeAssetsSettingsGlobalAnimation => "Global ExAnimation",
            Self::NativeAssetsSettingsLevelAnimation => "Level ExAnimation",
            Self::NativeAssetsSettingsPreservedNibbleFormat => {
                "Preserved unrelated low nibble: {value}"
            }
            Self::NativeAssetsSettingsApplyAnimation => "Apply animation options",
            Self::RomNativeAssetsTitle => "ROM Native Level Assets",
            Self::RomNativeAssetsStaleNotice => {
                "The ROM changed. Close and reopen this workspace before committing."
            }
            Self::RomNativeAssetsBusyNotice => {
                "Level import or commit preparation is active; staged editing is temporarily disabled."
            }
            Self::RomNativeAssetsReservedModeFormat => {
                "Mode ${mode} is reserved. Lunar Magic compatibility uses mode $00 instead."
            }
            Self::RomNativeAssetsUndo => "Undo",
            Self::RomNativeAssetsRedo => "Redo",
            Self::RomNativeAssetsModified => "Modified",
            Self::RomNativeAssetsUnmodified => "Unmodified",
            Self::RomNativeAssetsAllocation => "Allocation search (logical PC hex, end-exclusive)",
            Self::RomNativeAssetsRangeSeparator => "..",
            Self::RomNativeAssetsPaletteImportFull => "Import full .lmpal…",
            Self::RomNativeAssetsPaletteExportFull => "Export full .lmpal…",
            Self::RomNativeAssetsPaletteFullNotice => {
                "Every import is staged through the active ownership map; exports snapshot the current staged palette and never overwrite an existing file."
            }
            Self::RomNativeAssetsPaletteImportRaw => "Import raw…",
            Self::RomNativeAssetsPaletteExportRaw => "Export raw…",
            Self::RomNativeAssetsPaletteImportTpl => "Import TPL v2…",
            Self::RomNativeAssetsPaletteExportTpl => "Export TPL v2…",
            Self::RomNativeAssetsPaletteImportRgb => "Import RGB24…",
            Self::RomNativeAssetsPaletteExportRgb => "Export RGB24…",
            Self::RomNativeAssetsPaletteNativeNotice => {
                "Raw/TPL/RGB imports automatically apply a same-name .palmask when present; full exports remove a stale mask sidecar."
            }
            Self::RomNativeAssetsDiscardTitle => "Discard staged native assets?",
            Self::RomNativeAssetsDiscardNotice => {
                "The staged cross-domain changes have not been committed to the ROM."
            }
            Self::RomNativeAssetsCancel => "Cancel",
            Self::RomNativeAssetsDiscard => "Discard",
            Self::RomNativeAssetsErrorTitle => "ROM native-assets error",
            Self::RomNativeAssetsOk => "OK",
            Self::RomNativeAssetsMwlExportComplete => "Export complete MWL…",
            Self::RomNativeAssetsMwlImportComplete => "Import complete MWL…",
            Self::RomNativeAssetsMwlExportLegacy => "Export legacy multi-file level…",
            Self::RomNativeAssetsMwlImportLegacy => "Import legacy multi-file level…",
            Self::RomNativeAssetsMwlExportAll => "Export all MWLs…",
            Self::RomNativeAssetsMwlExportModified => "Export modified MWLs…",
            Self::RomNativeAssetsMwlBatchTitle => "Exporting levels",
            Self::RomNativeAssetsMwlBatchPathFormat => "Creating numbered MWLs from {path}",
            Self::RomNativeAssetsMwlBatchNotice => {
                "Cancellation takes effect before grouped publication starts."
            }
            Self::RomNativeAssetsMwlBatchCancelling => "Cancelling after the current level…",
            Self::RomNativeAssetsImageExportFull => "Export full level image…",
            Self::RomNativeAssetsImageExportPngBatch => "Export multiple level PNGs…",
            Self::RomNativeAssetsImageExportBmpBatch => "Export multiple level BMPs…",
            Self::RomNativeAssetsImageModifiedOnly => "Only levels stored in expanded ROM space",
            Self::RomNativeAssetsImageAutoScreens => "Auto-set image screen count",
            Self::RomNativeAssetsImageExportedPathFormat => "Exported full level image to {path}.",
            Self::RomNativeAssetsImageBatchResultFormat => {
                "{exported} level images exported; {skipped} unrenderable levels skipped."
            }
            Self::RomNativeAssetsImageBatchCancelled => "Level image export cancelled.",
            Self::RomNativeAssetsImageBatchTitle => "Exporting level images",
            Self::RomNativeAssetsImageBatchPathFormat => {
                "Staging numbered {format} images from {path}"
            }
            Self::RomNativeAssetsImageBatchModifiedSelection => {
                "Selection: levels whose Layer 1 data is in expanded ROM space"
            }
            Self::RomNativeAssetsImageBatchAllSelection => "Selection: all level slots",
            Self::RomNativeAssetsImageBatchProgressFormat => "{completed} / {total}",
            Self::RomNativeAssetsImageBatchNotice => {
                "Files become visible only after the complete batch is staged."
            }
            Self::RomNativeAssetsValidateGfx => "Validate selected Super GFX files",
            Self::RomNativeAssetsPreviewStart => "Start live bypass-aware preview",
            Self::RomNativeAssetsPreviewStop => "Stop live bypass-aware preview",
            Self::RomNativeAssetsPreviewCamera => "Preview camera",
            Self::RomNativeAssetsPreviewXPrefix => "X ",
            Self::RomNativeAssetsPreviewYPrefix => "Y ",
            Self::RomNativeAssetsPreviewReset => "Reset view",
            Self::RomNativeAssetsPreviewMap16Grid => "Map16 grid",
            Self::RomNativeAssetsPreviewSelectionFormat => "Selected Map16 cell X ${x}, Y ${y}",
            Self::RomNativeAssetsPreviewClearSelection => "Clear selection",
            Self::RomNativeAssetsPreviewHoverNotice => {
                "Click to select a Map16 cell; drag to pan; Ctrl/Command-wheel zooms"
            }
            Self::RomNativeAssetsCommit => "Commit all domains to ROM",
            Self::RomNativeAssetsCommitReclaim => "Commit and reclaim with LMRATS01 evidence",
            Self::RomNativeAssetsStaged => "Staged aggregate changes",
            Self::RomNativeAssetsNoStaged => "No staged changes",
            Self::RomNativeAssetsLayer2ResetTitle => "Reset Layer 2 for level mode change?",
            Self::RomNativeAssetsLayer2ResetChangeFormat => {
                "Changing level mode ${from} to ${to} switches Layer 2 storage formats."
            }
            Self::RomNativeAssetsLayer2ResetNotice => {
                "Lunar Magic clears the tilemap workspace when entering a tilemap-backed mode. Object-backed data remains available if you switch back before saving."
            }
            Self::RomNativeAssetsLayer2ResetAction => "Reset Layer 2 and stage changes",
            Self::RomNativeAssetsMwlBatchResultFormat => {
                "{count} levels were exported successfully."
            }
            Self::RomNativeAssetsMwlBatchCancelled => "Batch MWL export cancelled.",
            Self::RomNativeAssetsLegacyCompatibilityFormat => {
                "Legacy import compatibility: {diagnostics}"
            }
            Self::RomNativeAssetsPreviewRendered => {
                "Rendered installed Layer 2 and Layer 1 object streams with the selected Super GFX files, installed Map16 definitions, and staged level palette."
            }
            Self::RomNativeAssetsPreviewUnresolvedFormat => {
                "Rendered the installed object layers with unresolved definitions: {diagnostics}"
            }
            Self::RomNativeAssetsInspectionHeadingFormat => {
                "Resolved staged Map16 cell X ${x}, Y ${y} in painter order"
            }
            Self::RomNativeAssetsInspectionNoMap16 => {
                "No Layer 2 or Layer 1 placement resolves at this cell."
            }
            Self::RomNativeAssetsInspectionSpriteHeading => {
                "Overlapping staged sprite-preview parts in painter order"
            }
            Self::RomNativeAssetsInspectionNoSprite => {
                "No materialized sprite-preview part overlaps this cell."
            }
            Self::RomOverworldOpenTitle => "Open native overworld",
            Self::RomOverworldOpenSlot => "Profile overworld slot (hex)",
            Self::RomOverworldCancel => "Cancel",
            Self::RomOverworldOpen => "Open",
            Self::RomOverworldDiscardTitle => "Discard staged overworld changes?",
            Self::RomOverworldDiscardPlayableNotice => {
                "Playable terrain or route-link changes have not been committed."
            }
            Self::RomOverworldDiscardCompleteNotice => {
                "Overworld payload or per-map animation-option changes have not been committed."
            }
            Self::RomOverworldDiscard => "Discard",
            Self::RomOverworldErrorTitle => "ROM overworld error",
            Self::RomOverworldOk => "OK",
            Self::RomOverworldCompleteTitle => "ROM Complete Overworld Editor",
            Self::RomOverworldPlayableTitle => "ROM Playable Main Overworld Layer 2 Editor",
            Self::RomOverworldImportComplete => "Import complete .lmow…",
            Self::RomOverworldExportComplete => "Export complete .lmow…",
            Self::RomOverworldCompleteTransferNotice => {
                "Complete transfer stages or exports all nine modeled overworld domains together."
            }
            Self::RomOverworldImportAnimation => "Import animation .lmexan…",
            Self::RomOverworldExportAnimation => "Export animation .lmexan…",
            Self::RomOverworldAnimationTransferNotice => {
                "Animation transfer changes only the active overworld animation domain."
            }
            Self::RomOverworldStaleNotice => {
                "The ROM changed; reopen before editing or committing."
            }
            Self::RomOverworldPlayableMapNotice => {
                "Gameplay-consumed SMW US main-map Layer 2 (128x64 tiles)"
            }
            Self::RomOverworldAllocation => "Allocation logical PC hex",
            Self::RomOverworldRangeSeparator => "..",
            Self::RomOverworldCommitPlayable => "Commit playable Layer 2 map",
            Self::RomOverworldPlayableStaged => "Staged playable map changes",
            Self::RomOverworldPlayableUnmodified => "No staged map changes",
            Self::RomOverworldRouteBlocksTerrain => {
                "Commit or discard the staged route-link edit before changing terrain."
            }
            Self::RomOverworldRouteTitle => "Gameplay route links",
            Self::RomOverworldRouteNotice => {
                "Native source/destination endpoints and engine target bytes (hexadecimal)."
            }
            Self::RomOverworldRouteCanvasNotice => {
                "Route tools use the left plane for submap 00. On the right shared submap sheet, enter submap 01-06 first; a click retains that endpoint's submap ID."
            }
            Self::RomOverworldRouteUnavailable => "No gameplay route links are installed.",
            Self::RomOverworldRouteLink => "Link",
            Self::RomOverworldRouteSourceX => "Source X",
            Self::RomOverworldRouteSourceY => "Source Y",
            Self::RomOverworldRouteSourceSubmap => "Source submap",
            Self::RomOverworldRouteDestinationX => "Destination X",
            Self::RomOverworldRouteDestinationY => "Destination Y",
            Self::RomOverworldRouteDestinationSubmap => "Destination submap",
            Self::RomOverworldRouteTargetX => "Target X tile",
            Self::RomOverworldRouteTargetY => "Target Y tile",
            Self::RomOverworldRouteDirection => "Direction",
            Self::RomOverworldRouteOneWay => "One-way (no return endpoint)",
            Self::RomOverworldRouteOrderNotice => {
                "Canvas route tools use Lunar Magic's Up, Down, Left, Right order and exact edge offsets."
            }
            Self::RomOverworldRouteReload => "Reload link",
            Self::RomOverworldRouteApply => "Apply route link",
            Self::RomOverworldRouteCommit => "Commit route links",
            Self::RomOverworldTerrainBlocksRoute => {
                "Commit or discard the staged terrain edit before changing route links."
            }
            Self::RomOverworldRouteStaged => "Staged gameplay route changes",
            Self::RomOverworldLayer2Tilemap => "Layer 2 packed 8x8 tilemap",
            Self::RomOverworldTileWord => "SNES tilemap word",
            Self::RomOverworldApplyLayerTile => "Apply layer tile",
            Self::RomOverworldTabRecords => "Records",
            Self::RomOverworldTabPalette => "Palette",
            Self::RomOverworldTabAnimation => "Animation",
            Self::RomOverworldTabNativeSprites => "Native sprites",
            Self::RomOverworldSpriteTitle => "Native custom overworld sprite stream",
            Self::RomOverworldSpriteNotice => {
                "Seven map-local lists, variable record widths, and Lunar Magic's 24-sprite-per-map limit."
            }
            Self::RomOverworldSpriteCanvasNotice => {
                "Canvas: Ctrl/Command-click toggles, drag empty space selects, Ctrl/Command+A selects all, Delete removes, right-drag duplicates the selected group, and Alt-right-click edits one sprite."
            }
            Self::RomOverworldSpriteMap => "Map",
            Self::RomOverworldSpriteIndex => "Record / insertion index",
            Self::RomOverworldSpriteId => "ID (hex)",
            Self::RomOverworldSpriteX => "X pixels (hex)",
            Self::RomOverworldSpriteY => "Y pixels (hex)",
            Self::RomOverworldSpriteScreen => "Screen (hex)",
            Self::RomOverworldSpriteExtension => "Extension bytes (hex)",
            Self::RomOverworldSpriteLoad => "Load selected",
            Self::RomOverworldSpriteUseCanvas => "Use canvas selection",
            Self::RomOverworldSpritePlace => "Place at canvas cursor",
            Self::RomOverworldSpriteRequiredFormat => "ID {id} requires {count} extension byte(s).",
            Self::RomOverworldSpriteFillExtension => "Fill extension with zeroes",
            Self::RomOverworldSpriteInsert => "Insert",
            Self::RomOverworldSpriteReplace => "Replace",
            Self::RomOverworldSpriteDelete => "Delete",
            Self::RomOverworldSpriteMoveUp => "Move up",
            Self::RomOverworldSpriteMoveDown => "Move down",
            Self::RomOverworldSpriteCountFormat => {
                "Map {map}: {count}/24 records; {selected} selected"
            }
            Self::RomOverworldSpritePropertiesTitle => "Custom overworld sprite properties",
            Self::RomOverworldSpriteRecordFormat => "Map {map} record {record}",
            Self::RomOverworldSpriteApply => "Apply",
            Self::RomOverworldSaveTransitionTitle => "Save overworld to ROM?",
            Self::RomOverworldSaveTransitionNotice => {
                "The overworld has staged changes. Save before continuing?"
            }
            Self::RomOverworldSave => "Save",
            Self::RomOverworldCommitAll => "Commit all staged overworld changes",
            Self::RomOverworldCommitReclaim => "Commit and reclaim all nine",
            Self::RomOverworldStaged => "Staged overworld changes",
            Self::RomOverworldUnmodified => "No staged changes",
            Self::RomOverworldDirectTilePicker => "Visual 8x8 tile picker",
            Self::RomOverworldPaletteRow => "Palette row",
            Self::RomOverworldGraphicsPreviewUnavailable => {
                "The current overworld graphics cannot be previewed."
            }
            Self::RomOverworldLayer1 => "Layer 1",
            Self::RomOverworldLayer2 => "Layer 2",
            Self::RomOverworldMap16Tile => "Map16 tile",
            Self::RomOverworldAnimationDestinations => "Rendered graphics destinations",
            Self::RomOverworldAnimationDestinationNotice => {
                "Ctrl+Shift+click an attributed 8x8 tile to select its last-writing local or global ExAnimation record."
            }
            Self::RomOverworldAnimationCacheUnavailable => {
                "The current animated graphics cache could not be rendered."
            }
            Self::RomOverworldAnimationOwnerFormat => {
                "Tile {tile}: {domain} ExAnimation record {record}"
            }
            Self::RomOverworldAnimationNoOwnerFormat => "Tile {tile}: no ExAnimation owner",
            Self::RomOverworldMap16Picker => "Visual Map16 tile picker",
            Self::RomOverworldMap16Page => "Map16 page",
            Self::RomOverworldMap16PreviewUnavailable => {
                "This Map16 page cannot be previewed with the current overworld assets."
            }
            Self::RomOverworldCompletedReveals => "Completed event reveals",
            Self::RomOverworldPreviewUnavailable => {
                "Overworld preview unavailable; property editing remains available."
            }
            Self::RomOverworldToolSelect => "Select",
            Self::RomOverworldToolBrush => "Brush",
            Self::RomOverworldToolRectangle => "Rectangle",
            Self::RomOverworldToolFill => "Fill",
            Self::RomOverworldToolNativeSprite => "Place/move native sprite",
            Self::RomOverworldToolRouteSource => "Set route source",
            Self::RomOverworldToolRouteDestination => "Set route destination",
            Self::RomOverworldAnimationRate7_5 => "7.5 fps",
            Self::RomOverworldAnimationRate15 => "15 fps",
            Self::RomOverworldAnimationRate30 => "30 fps",
            Self::RomOverworldAnimationRate60 => "60 fps",
            Self::RomOverworldAnimationSubstep => "substep",
            Self::RomOverworldAnimationSubsteps => "substeps",
            Self::RomOverworldAnimationTriggerPrefix => "#",
            Self::RomOverworldAnimationManualFramePrefix => "frame $",
            Self::NativePreviewPreparing => "Preparing native preview…",
            Self::NativePreviewUnavailableFormat => "Preview unavailable: {error}",
            Self::ExternalToolConfigAddSnes => "Add SNES emulator",
            Self::ExternalToolConfigAddGba => "Add GBA emulator",
            Self::ExternalToolConfigAddTileEditor => "Add tile editor",
            Self::ExternalToolConfigRemove => "Remove",
            Self::ExternalToolConfigEmptyNotice => "Add an emulator or external tool to begin.",
            Self::ExternalToolConfigStableId => "Stable ID",
            Self::ExternalToolConfigDisplayName => "Display name",
            Self::ExternalToolConfigArgumentsNotice => {
                "One direct process argument per line; use {rom}, {project_dir}, {level_hex}, or {level_dec}."
            }
            Self::ExternalToolConfigWorkingDirectory => "Working directory template (optional)",
            Self::ExternalToolConfigRunAfter => "Run automatically after:",
            Self::ExternalToolConfigRomOpened => "ROM opened",
            Self::ExternalToolConfigRomSaved => "ROM saved",
            Self::ExternalToolConfigLevelChanged => "Level changed",
            Self::OverworldPaletteColorFormat => "Color {index} — BGR555 {color}",
            Self::OverworldPaletteAnimationOwnerFormat => {
                "Animation ownership: {domain} record {record} (Ctrl+Shift+click to navigate)"
            }
            Self::OverworldPaletteEditable => "Ownership: editable",
            Self::OverworldPaletteFixed => "Ownership: fixed (read-only)",
            Self::OverworldPaletteExAnimationFormat => {
                "Ownership: ExAnimation record {record} (read-only)"
            }
            Self::OverworldPaletteInvalid => "Ownership: invalid (read-only)",
            Self::OverworldPaletteCopyColor => "Copy color",
            Self::OverworldPalettePasteColor => "Paste color",
            Self::OverworldPaletteCopyRow => "Copy row",
            Self::OverworldPalettePasteRow => "Paste row",
            Self::OverworldPaletteGestureNotice => {
                "Ctrl+left/right copies or pastes a color; add Alt for its complete row."
            }
            Self::OverworldRecordsReveals => "Reveals",
            Self::OverworldRecordsEndpoints => "Endpoints",
            Self::OverworldRecordsMessages => "Messages",
            Self::OverworldRecordsSprites => "Sprites",
            Self::OverworldRecordsNoReveals => {
                "This fixed-shape document contains no event reveals."
            }
            Self::OverworldRecordsReveal => "Reveal",
            Self::OverworldRecordsSourceTile => "Source tile (hex)",
            Self::OverworldRecordsDestinationTile => "Destination tile (hex)",
            Self::OverworldRecordsApplyReveal => "Apply reveal",
            Self::OverworldRecordsMoveSelection => "Move event-tile selection",
            Self::OverworldRecordsFirstPrefix => "First ",
            Self::OverworldRecordsLastPrefix => "Last ",
            Self::OverworldRecordsXTilesPrefix => "X tiles ",
            Self::OverworldRecordsYTilesPrefix => "Y tiles ",
            Self::OverworldRecordsMoveNotice => {
                "The complete selection uses Lunar Magic's seam-aware shared displacement and 6x6 footprint bounds."
            }
            Self::OverworldRecordsMoveSelected => "Move selected event tiles",
            Self::OverworldRecordsNoEndpoints => "This fixed-shape document contains no endpoints.",
            Self::OverworldRecordsEndpoint => "Endpoint",
            Self::OverworldRecordsXHex => "X (hex)",
            Self::OverworldRecordsYHex => "Y (hex)",
            Self::OverworldRecordsSubmapHex => "Submap (hex)",
            Self::OverworldRecordsApplyEndpoint => "Apply endpoint",
            Self::OverworldRecordsNoMessages => "This fixed-shape document contains no messages.",
            Self::OverworldRecordsMessage => "Message",
            Self::OverworldRecordsColumn => "Column",
            Self::OverworldRecordsRow => "Row",
            Self::OverworldRecordsTileHex => "Tile (hex)",
            Self::OverworldRecordsCopyMessage => "Copy message",
            Self::OverworldRecordsPasteMessage => "Paste message",
            Self::OverworldRecordsApplyMessageTile => "Apply message tile",
            Self::OverworldRecordsNoSprites => "This fixed-shape document contains no sprites.",
            Self::OverworldRecordsSprite => "Sprite",
            Self::OverworldRecordsIdHex => "ID (hex)",
            Self::OverworldRecordsUnownedExtension => "Unowned extension bytes:",
            Self::OverworldRecordsCopySprite => "Copy sprite",
            Self::OverworldRecordsPasteSprite => "Paste sprite",
            Self::OverworldRecordsApplySprite => "Apply sprite",
            Self::OverworldDocumentTitle => "Portable Complete Overworld Editor",
            Self::OverworldDocumentOpenTitle => "Open complete overworld",
            Self::OverworldDocumentMaximumRecords => {
                "Maximum ExAnimation records from this revision/profile:"
            }
            Self::OverworldDocumentOpen => "Open",
            Self::OverworldDocumentUndo => "Undo",
            Self::OverworldDocumentRedo => "Redo",
            Self::OverworldDocumentSave => "Save",
            Self::OverworldDocumentModified => "Modified",
            Self::OverworldDocumentSaved => "Saved",
            Self::OverworldDocumentTilemap => "Tilemap",
            Self::OverworldDocumentCoordinateFormat => "Coordinate {x}, {y}",
            Self::OverworldDocumentMap16Tile => "Map16 tile (hex)",
            Self::OverworldDocumentApplyTile => "Apply tile",
            Self::OverworldDocumentCompletedReveals => "Completed reveals",
            Self::OverworldDocumentPreviewUnavailable => {
                "Preview unavailable; property editing remains available."
            }
            Self::OverworldDocumentDiscardTitle => "Unsaved complete overworld",
            Self::OverworldDocumentDiscardNotice => "Discard unsaved overworld changes?",
            Self::OverworldDocumentCancel => "Cancel",
            Self::OverworldDocumentDiscard => "Discard",
            Self::OverworldDocumentErrorTitle => "Overworld editor error",
            Self::OverworldDocumentOk => "OK",
            Self::LevelAuxScreenExits => "Screen exits",
            Self::LevelAuxSecondaryExits => "Secondary exits",
            Self::LevelAuxMap16Overrides => "Map16 overrides",
            Self::LevelAuxScreenExit => "Screen exit",
            Self::LevelAuxEncodedValue => "Encoded value (hex)",
            Self::LevelAuxSecondaryExit => "Secondary exit",
            Self::LevelAuxOverride => "Override",
            Self::LevelAuxUpsert => "Upsert",
            Self::LevelAuxRemoveSelected => "Remove selected",
            Self::LevelAuxAppend => "Append",
            Self::LevelAuxReplace => "Replace",
            Self::LevelAuxRemove => "Remove",
            Self::LevelAuxDestination => "Destination (hex)",
            Self::LevelAuxPositionMethod => "Position/method (hex)",
            Self::LevelAuxScreen => "Screen (hex)",
            Self::LevelAuxX => "X (hex)",
            Self::LevelAuxY => "Y (hex)",
            Self::LevelAuxDestinationFlags => "Destination flags (hex)",
            Self::LevelAuxXOverworldFlags => "X/overworld flags (hex)",
            Self::LevelAuxAdditionalFlags => "Additional flags (hex)",
            Self::LevelAuxIndex => "Index (hex)",
            Self::LevelAuxTopLeft => "Top left (hex)",
            Self::LevelAuxTopRight => "Top right (hex)",
            Self::LevelAuxBottomLeft => "Bottom left (hex)",
            Self::LevelAuxBottomRight => "Bottom right (hex)",
            Self::LevelAuxActsLike => "Acts Like (hex)",
            Self::LevelAdvancedExpandedHeader => "Expanded header",
            Self::LevelAdvancedLayer3 => "Layer 3",
            Self::LevelAdvancedEnableLayer3 => "Enable Layer 3 with recovered defaults",
            Self::LevelAdvancedStartPosition => "Start position",
            Self::LevelAdvancedTilemapSize => "Tilemap size",
            Self::LevelAdvancedLiquidType => "Liquid/type",
            Self::LevelAdvancedFlags => "Flags",
            Self::LevelAdvancedGraphicsFormat => "Graphics {slot}",
            Self::LevelAdvancedReservedBytes => "Reserved bytes (exactly 16 hexadecimal bytes):",
            Self::LevelAdvancedRawTilemap => "Raw tilemap bytes:",
            Self::LevelAdvancedRemapBytes => "Literal remap-command bytes:",
            Self::LevelAdvancedApplyLayer3 => "Apply Layer 3",
            Self::LevelAdvancedDisableLayer3 => "Disable Layer 3",
            Self::LevelAdvancedCopyTilemap => "Copy tilemap",
            Self::LevelAdvancedPasteTilemap => "Paste tilemap",
            Self::LevelAdvancedCopyRemap => "Copy remap commands",
            Self::LevelAdvancedPasteRemap => "Paste remap commands",
            Self::LevelAdvancedExpandedEnabled => "Expanded header enabled",
            Self::LevelAdvancedExpandedNotice => {
                "Enable the exact 16-word expanded record to edit its opaque fields."
            }
            Self::LevelAdvancedSuperGfx => "Super GFX Bypass",
            Self::LevelAdvancedUsePerLevelGfx => "Use per-level GFX/ExGFX files",
            Self::LevelAdvancedRawExpandedWords => {
                "Raw expanded words (unproven fields remain editable and lossless):"
            }
            Self::LevelAdvancedFieldFormat => "Field {index}",
            Self::LevelCoreHeader => "Header",
            Self::LevelCoreObjects => "Objects",
            Self::LevelCoreSprites => "Sprites",
            Self::LevelCoreEntrances => "Entrances",
            Self::LevelCoreExitsMap16 => "Exits/Map16",
            Self::LevelCoreAdvanced => "Advanced",
            Self::LevelCoreLayer1 => "Layer 1",
            Self::LevelCoreLayer2 => "Layer 2",
            Self::LevelCoreRecord => "Record",
            Self::LevelCoreObjectBytes => "Lossless encoded bytes (3–8 bytes):",
            Self::LevelCoreSpriteBytes => "Lossless revision-sized encoded bytes:",
            Self::LevelCoreAppend => "Append",
            Self::LevelCoreReplace => "Replace",
            Self::LevelCoreRemove => "Remove",
            Self::LevelCoreCopy => "Copy",
            Self::LevelCorePaste => "Paste",
            Self::LevelCoreStreamHeaderFormat => "Stream header: {header}",
            Self::LevelCoreEntrance => "Entrance",
            Self::LevelCoreMain => "Main",
            Self::LevelCoreMidway => "Midway",
            Self::LevelCoreSecondary => "Secondary",
            Self::LevelCoreX => "X (hex)",
            Self::LevelCoreY => "Y (hex)",
            Self::LevelCoreScreen => "Screen (hex)",
            Self::LevelCoreAction => "Action (hex)",
            Self::LevelCoreRawFlags => "Raw flags (hex)",
            Self::LevelCoreLevelNumber => "Level number",
            Self::LevelCoreBackgroundPalette => "Background palette",
            Self::LevelCoreLastScreen => "Last screen",
            Self::LevelCoreLevelMode => "Level mode",
            Self::LevelCoreBackgroundColor => "Background color",
            Self::LevelCoreSpriteTileset => "Sprite tileset",
            Self::LevelCoreDefaultMusicSelector => "Default music selector",
            Self::LevelCoreTimeLimitSelector => "Time limit selector",
            Self::LevelCoreSpritePalette => "Sprite palette",
            Self::LevelCoreForegroundPalette => "Foreground palette",
            Self::LevelCoreObjectTileset => "Object tileset",
            Self::LevelCoreLayer1VerticalScroll => "Layer 1 vertical scroll",
            Self::LevelDocumentTitle => "Portable Complete Level Editor",
            Self::LevelDocumentDimensionsTitle => "Level dimensions",
            Self::LevelDocumentDimensionsNotice => "Enter exact row-major tilemap dimensions:",
            Self::LevelDocumentLayer1Width => "Layer 1 width",
            Self::LevelDocumentLayer1Height => "Layer 1 height",
            Self::LevelDocumentLayer2Width => "Layer 2 width",
            Self::LevelDocumentLayer2Height => "Layer 2 height",
            Self::LevelDocumentCancel => "Cancel",
            Self::LevelDocumentOpen => "Open",
            Self::LevelDocumentUndo => "Undo",
            Self::LevelDocumentRedo => "Redo",
            Self::LevelDocumentSave => "Save",
            Self::LevelDocumentModified => "Modified",
            Self::LevelDocumentSaved => "Saved",
            Self::LevelDocumentLayer1 => "Layer 1",
            Self::LevelDocumentLayer2 => "Layer 2",
            Self::LevelDocumentPreviewUnavailable => {
                "Preview unavailable; editing remains available."
            }
            Self::LevelDocumentTilemap => "Tilemap",
            Self::LevelDocumentCoordinateFormat => "Coordinate {x}, {y}",
            Self::LevelDocumentMap16Tile => "Map16 tile (hex)",
            Self::LevelDocumentApplyTile => "Apply tile",
            Self::LevelDocumentDiscardTitle => "Unsaved complete level",
            Self::LevelDocumentDiscardNotice => "Discard unsaved level changes?",
            Self::LevelDocumentDiscard => "Discard",
            Self::LevelDocumentErrorTitle => "Level editor error",
            Self::LevelDocumentOk => "OK",
            Self::Map16DocumentTitle => "Portable Map16 Page Editor",
            Self::Map16DocumentSave => "Save",
            Self::Map16DocumentModified => "Modified",
            Self::Map16DocumentSaved => "Saved",
            Self::Map16DocumentPreviewUnavailable => "Preview unavailable",
            Self::Map16DocumentTileFormat => "Tile {tile}",
            Self::Map16DocumentSubtileHex => "8×8 tile (hex)",
            Self::Map16DocumentHorizontalFlip => "Horizontal flip",
            Self::Map16DocumentVerticalFlip => "Vertical flip",
            Self::Map16DocumentTopLeft => "Top left",
            Self::Map16DocumentTopRight => "Top right",
            Self::Map16DocumentBottomLeft => "Bottom left",
            Self::Map16DocumentBottomRight => "Bottom right",
            Self::Map16DocumentDiscardTitle => "Unsaved Map16 page",
            Self::Map16DocumentDiscardNotice => "Discard unsaved Map16 changes?",
            Self::Map16DocumentCancel => "Cancel",
            Self::Map16DocumentDiscard => "Discard",
            Self::Map16DocumentErrorTitle => "Map16 error",
            Self::Map16DocumentOk => "OK",
            Self::VanillaGraphicsHeadingFormat => "GFX{slot} — built-in SMW graphics editor",
            Self::VanillaGraphicsSplitPointers => {
                "Vanilla split pointer planes detected automatically."
            }
            Self::VanillaGraphicsPaintColor => "Paint color",
            Self::VanillaGraphicsRelocationNotice => {
                "Graphics relocation needs one expanded free-space bank."
            }
            Self::VanillaGraphicsExpandRom => "Expand ROM to 1 MiB",
            Self::VanillaGraphicsCommit => "Commit graphics changes to ROM",
            Self::VanillaGraphicsNoTiles => "No tiles in this graphics file.",
            Self::NavigationPathTitle => "ROM Overworld Path Links",
            Self::NavigationWarpTitle => "ROM Overworld Warp Links",
            Self::NavigationPathNotice => {
                "Lossless source/destination endpoints and engine target bytes. Hexadecimal."
            }
            Self::NavigationWarpNotice => {
                "Four lossless coordinate words per warp. Packed vertical fields remain opaque."
            }
            Self::NavigationPathCountFormat => "Staged path links: {count}",
            Self::NavigationWarpCountFormat => "Staged warp links: {count}",
            Self::NavigationStaleNotice => {
                "The ROM changed after this table was opened. Reopen before committing."
            }
            Self::NavigationPathTableCount => "Table count (00–80)",
            Self::NavigationWarpTableCount => "Table count (000–100)",
            Self::NavigationResizeTable => "Resize table",
            Self::NavigationLoadLink => "Load link",
            Self::NavigationApplyLink => "Apply link",
            Self::NavigationCommitLinks => "Commit links to ROM",
            Self::NavigationStaged => "Staged",
            Self::NavigationUnchanged => "Unchanged",
            Self::NavigationIndex => "Index",
            Self::NavigationSourceX => "Source X",
            Self::NavigationSourceY => "Source Y",
            Self::NavigationSourceSubmap => "Source submap",
            Self::NavigationDestinationX => "Destination X",
            Self::NavigationDestinationY => "Destination Y",
            Self::NavigationDestinationSubmap => "Destination submap",
            Self::NavigationTargetXTile => "Target X tile",
            Self::NavigationTargetYTile => "Target Y tile",
            Self::NavigationSourcePackedVertical => "Source packed vertical",
            Self::NavigationSourceHorizontalTile => "Source horizontal tile",
            Self::NavigationDestinationPackedVertical => "Destination packed vertical",
            Self::NavigationDestinationHorizontalTile => "Destination horizontal tile",
            Self::NavigationPathDiscardTitle => "Discard path-link changes?",
            Self::NavigationWarpDiscardTitle => "Discard warp-link changes?",
            Self::NavigationPathDiscardNotice => {
                "The staged path-link table has not been committed."
            }
            Self::NavigationWarpDiscardNotice => {
                "The staged warp-link table has not been committed."
            }
            Self::NavigationCancel => "Cancel",
            Self::NavigationDiscard => "Discard",
            Self::NavigationPathErrorTitle => "Path-link editor error",
            Self::NavigationWarpErrorTitle => "Warp-link editor error",
            Self::NavigationOk => "OK",
            Self::OverworldAppearancePortableTitle => "Portable Overworld Appearance Editor",
            Self::OverworldAppearanceNativeTitle => "Native Overworld Appearance Editor",
            Self::OverworldAppearanceImportNative => "Import Native Pair",
            Self::OverworldAppearanceExportNative => "Export Native Pair",
            Self::OverworldAppearanceDefinitionsFormat => "Sprite definitions: {count}",
            Self::OverworldAppearanceDefinition => "Definition",
            Self::OverworldAppearanceEmptyNotice => {
                "Insert a sprite definition before adding tile parts."
            }
            Self::OverworldAppearanceSpriteId => "Sprite ID (hex)",
            Self::OverworldAppearanceInsertDefinition => "Insert definition at index",
            Self::OverworldAppearanceRemoveDefinition => "Remove selected definition",
            Self::OverworldAppearanceMoveToEnd => "Move to end",
            Self::OverworldAppearanceMoveDefinition => "Move selected definition",
            Self::OverworldAppearancePartsTitleFormat => "Tile parts for sprite {sprite}",
            Self::OverworldAppearancePartsCountFormat => "Painter-ordered parts: {count}",
            Self::OverworldAppearancePart => "Part",
            Self::OverworldAppearanceReplacePart => "Replace selected part",
            Self::OverworldAppearanceRemovePart => "Remove selected part",
            Self::OverworldAppearanceCopyPart => "Copy part",
            Self::OverworldAppearancePasteOverPart => "Paste over part",
            Self::OverworldAppearancePasteAfterPart => "Paste after part",
            Self::OverworldAppearanceDuplicatePart => "Duplicate part",
            Self::OverworldAppearanceCopyComposition => "Copy composition",
            Self::OverworldAppearanceReplaceComposition => "Replace composition",
            Self::OverworldAppearanceAppendComposition => "Append composition",
            Self::OverworldAppearancePasteNewDefinition => "Paste as new definition",
            Self::OverworldAppearanceMovePart => "Move selected part",
            Self::OverworldAppearanceInsertPart => "Insert part at index",
            Self::OverworldAppearancePreviewTitle => "Composition preview",
            Self::OverworldAppearancePreviewNotice => {
                "Click to select; arrows move one part, Alt+arrows move all parts. Shift uses eight pixels; X/Y flip; Page Up/Down changes painter order; Insert duplicates; Delete removes."
            }
            Self::OverworldAppearanceSaveNative => "Save Native Pair",
            Self::OverworldAppearanceNativeSummaryFormat => {
                "{tooltips} tooltips, {appearances} appearances, {graphics} graphics ranges, {palettes} palette ranges"
            }
            Self::OverworldAppearanceNativeSpriteId => "Sprite ID",
            Self::OverworldAppearanceTooltip => "Tooltip",
            Self::OverworldAppearanceDefinitionEnabled => "Definition enabled",
            Self::OverworldAppearanceDisablePositionText => "Disable original position text",
            Self::OverworldAppearanceApplyTooltip => "Apply Tooltip",
            Self::OverworldAppearanceExternalRanges => "External Graphics and Palette Ranges",
            Self::OverworldAppearanceRangesNotice => {
                "Ranges retain their native kind, inclusive tile span, base, and file order."
            }
            Self::OverworldAppearanceGraphics => "Graphics",
            Self::OverworldAppearancePalette => "Palette",
            Self::OverworldAppearanceApplyRangesFormat => "Apply {kind} Ranges",
            Self::OverworldAppearanceDisplay => "Display Appearance",
            Self::OverworldAppearanceEditorShadow => "Editor shadow",
            Self::OverworldAppearanceMap16Tiles => "Map16 tiles",
            Self::OverworldAppearanceTextLabel => "Text label",
            Self::OverworldAppearanceX => "X",
            Self::OverworldAppearanceY => "Y",
            Self::OverworldAppearanceApplyDisplay => "Apply Appearance",
            Self::OverworldAppearanceCustomMap16 => "Custom Sprite Map16 Definition",
            Self::OverworldAppearanceNativeTile => "Native tile",
            Self::OverworldAppearanceTopLeft => "TL",
            Self::OverworldAppearanceTopRight => "TR",
            Self::OverworldAppearanceBottomLeft => "BL",
            Self::OverworldAppearanceBottomRight => "BR",
            Self::OverworldAppearanceApplyMap16 => "Apply Sprite Map16",
            Self::OverworldAppearanceNativePartsFormat => "Parts: {count}",
            Self::OverworldAppearanceAddPart => "Add Part",
            Self::OverworldAppearanceRemovePartNative => "Remove Part",
            Self::OverworldAppearanceSendBackward => "Send Backward",
            Self::OverworldAppearanceBringForward => "Bring Forward",
            Self::OverworldAppearanceMap16 => "Map16",
            Self::OverworldAppearanceTranslucent => "Translucent",
            Self::OverworldAppearanceAddRange => "Add",
            Self::OverworldAppearanceKind => "Kind",
            Self::OverworldAppearanceFirst => "First",
            Self::OverworldAppearanceLast => "Last",
            Self::OverworldAppearanceBase => "Base",
            Self::OverworldAppearanceRemoveRange => "Remove",
            Self::ApplicationGfxOverrideTitle => "GFX Display Override (in hex)",
            Self::ApplicationGfxOverrideLayer12 => "Layer 1/2",
            Self::ApplicationGfxOverrideLayer3 => "Layer 3",
            Self::ApplicationGfxOverrideNotice => {
                "Note that this is for design purposes only.  The ROM is not affected by these settings.  Set a slot to 7F to use the real setting."
            }
            Self::ApplicationGfxOverrideOk => "OK",
            Self::ApplicationGfxOverrideCancel => "Cancel",
            Self::ApplicationToolbarBack => "Back",
            Self::ApplicationToolbarForward => "Forward",
            Self::ApplicationToolbarLevel => "Level",
            Self::ApplicationRecentEmpty => "Open a Recent File",
            Self::ApplicationRecentClear => "Clear Recent Files",
            Self::ApplicationRecentClearTitle => "Clear Recent Files List?",
            Self::ApplicationRecentClearNotice => {
                "This will clear your recent files list. Are you sure you want to do this?"
            }
            Self::ApplicationRecentYes => "Yes",
            Self::ApplicationRecentNo => "No",
            Self::ApplicationIpsWarningTitle => "Check if ROMFileName.ips Exists",
            Self::ApplicationIpsWarningFormat => {
                "A same-name IPS file ({file}) exists beside the ROM. Some emulators automatically apply it, which can hide saved editor changes or cause other problems."
            }
            Self::ApplicationIpsRenameNotice => {
                "Rename or move the IPS file to avoid automatic patching."
            }
            Self::ApplicationIpsSaveQuestion => "Save the ROM anyway?",
            Self::ApplicationIpsSaveAnyway => "Save Anyway",
            Self::ApplicationIpsCancel => "Cancel",
            Self::ApplicationTwoBppTitle => "Lunar Magic Rust",
            Self::ApplicationTwoBppQuestion => "Switch 2bpp viewing mode?",
            Self::ApplicationYes => "Yes",
            Self::ApplicationNo => "No",
            Self::ApplicationTruncateTitle => "Remove data beyond max screens?",
            Self::ApplicationTruncateNotice => {
                "This will delete all objects and sprites beyond the current max screen limit for this level mode.  Proceed?"
            }
            Self::ExAnimationDocumentTitle => "Portable ExAnimation Editor",
            Self::ExAnimationDocumentOpenTitle => "Open ExAnimation",
            Self::ExAnimationDocumentMaximumRecords => {
                "Maximum records from this ROM revision/profile:"
            }
            Self::ExAnimationDocumentOpen => "Open",
            Self::ExAnimationDocumentUndo => "Undo",
            Self::ExAnimationDocumentRedo => "Redo",
            Self::ExAnimationDocumentSave => "Save",
            Self::ExAnimationDocumentModified => "Modified",
            Self::ExAnimationDocumentSaved => "Saved",
            Self::ExAnimationDocumentDiscardTitle => "Unsaved ExAnimation",
            Self::ExAnimationDocumentDiscardNotice => "Discard unsaved ExAnimation changes?",
            Self::ExAnimationDocumentCancel => "Cancel",
            Self::ExAnimationDocumentDiscard => "Discard",
            Self::ExAnimationDocumentErrorTitle => "ExAnimation error",
            Self::ExAnimationDocumentOk => "OK",
            Self::ExAnimationDocumentRecords => "Records",
            Self::ExAnimationDocumentRecordListFormat => "{index}: kind {kind}",
            Self::ExAnimationDocumentAppendRecord => "Append new record",
            Self::ExAnimationDocumentRemoveSelected => "Remove selected",
            Self::ExAnimationDocumentSlotSettings => "Slot settings",
            Self::ExAnimationDocumentSettingHex => "Setting (hex)",
            Self::ExAnimationDocumentHeaderHex => "Header (hex)",
            Self::ExAnimationDocumentTriggerValueHex => "Value (hex)",
            Self::ExAnimationDocumentRecordFormat => "Record {index}",
            Self::ExAnimationDocumentKindHex => "Kind (hex)",
            Self::ExAnimationDocumentTriggerHex => "Trigger (hex)",
            Self::ExAnimationDocumentDestinationHex => "Destination (hex)",
            Self::ExAnimationDocumentSourceWordsNotice => "Source words, one frame per line:",
            Self::ExAnimationDocumentSpecialTransferNotice => {
                "This special transfer kind has no ordinary source-word frame payload."
            }
            Self::ExAnimationDocumentApplyRecord => "Apply record",
            Self::PaletteDocumentTitle => "Portable Palette Editor",
            Self::PaletteDocumentUndo => "Undo",
            Self::PaletteDocumentRedo => "Redo",
            Self::PaletteDocumentSave => "Save",
            Self::PaletteDocumentModified => "Modified",
            Self::PaletteDocumentSaved => "Saved",
            Self::PaletteDocumentDiscardTitle => "Unsaved palette",
            Self::PaletteDocumentDiscardNotice => "Discard unsaved palette changes?",
            Self::PaletteDocumentCancel => "Cancel",
            Self::PaletteDocumentDiscard => "Discard",
            Self::PaletteDocumentErrorTitle => "Palette error",
            Self::PaletteDocumentOk => "OK",
            Self::PaletteDocumentColorFormat => "Color {index} — BGR555 {value}",
            Self::RomPaletteTitle => "ROM Palette Editor",
            Self::RomPaletteStaleNotice => "The ROM changed; reopen before editing or committing.",
            Self::RomPaletteAllocation => "Allocation logical PC hex",
            Self::RomPaletteRangeSeparator => "..",
            Self::RomPaletteCommit => "Commit palette to ROM",
            Self::RomPaletteCommitReclaim => "Commit and reclaim",
            Self::RomPaletteStaged => "Staged palette changes",
            Self::RomPaletteUnmodified => "No staged changes",
            Self::RomPaletteColorFormat => "Color {index} — raw BGR555 {value}",
            Self::RomPaletteShortcutNotice => {
                "Ctrl+left/right copies or pastes a color; add Alt for its complete 16-color row."
            }
            Self::RomPaletteMaskMode => "Palette mask edit mode",
            Self::RomPaletteEnableAll => "Enable all",
            Self::RomPaletteDisableAll => "Disable all",
            Self::RomPaletteMaskNotice => {
                "Click a color to enable/disable it for .palmask export; hold Alt to change its entire row."
            }
            Self::RomPaletteDiscardTitle => "Discard staged palette changes?",
            Self::RomPaletteDiscardNotice => "These changes have not been committed to the ROM.",
            Self::RomPaletteCancel => "Cancel",
            Self::RomPaletteDiscard => "Discard",
            Self::RomPaletteErrorTitle => "ROM palette error",
            Self::RomPaletteOk => "OK",
            Self::RomPaletteImportRow => "Import selected row…",
            Self::RomPaletteExportRow => "Export selected row…",
            Self::RomPaletteRowTransferNotice => {
                "Row transfer matches Lunar Magic's exact 32-byte, 16-color little-endian SNES format and targets the row selected when loading starts."
            }
            Self::RomPaletteImportRaw => "Import raw palette…",
            Self::RomPaletteExportRaw => "Export raw palette…",
            Self::RomPaletteRawTransferNotice => {
                "Raw transfer preserves all 257 native words and automatically applies a same-name .palmask sidecar when present."
            }
            Self::RomExAnimationTitle => "ROM ExAnimation Editor",
            Self::RomExAnimationSwitchDomain => "Switch level/global domain",
            Self::RomExAnimationGlobalUnavailableFormat => {
                "Global ExAnimation is unavailable: {error}"
            }
            Self::RomExAnimationSwitchBlocked => {
                "Commit or revert this domain before switching level/global targets."
            }
            Self::RomExAnimationGlobalTarget => "Global ExAnimation",
            Self::RomExAnimationLevelTargetFormat => "Level {level} ExAnimation",
            Self::RomExAnimationCommit => "Commit ExAnimation to ROM",
            Self::RomExAnimationStaged => "Staged animation changes",
            Self::RomExAnimationUnmodified => "No staged changes",
            Self::RomExAnimationAppendRecord => "Append form as record",
            Self::RomExAnimationSpecialTransferNotice => {
                "This transfer kind has no ordinary source-word payload."
            }
            Self::RomExAnimationReplaceRecord => "Replace record",
            Self::RomExAnimationDiscardTitle => "Discard staged ExAnimation changes?",
            Self::RomExAnimationDiscardNotice => {
                "These changes have not been committed to the ROM."
            }
            Self::RomExAnimationCancel => "Cancel",
            Self::RomExAnimationDiscard => "Discard",
            Self::RomExAnimationErrorTitle => "ROM ExAnimation error",
            Self::RomExAnimationOk => "OK",
            Self::RomPaletteImportTpl => "Import TPL v2…",
            Self::RomPaletteExportTpl => "Export TPL v2…",
            Self::RomPaletteImportRgb => "Import RGB24…",
            Self::RomPaletteExportRgb => "Export RGB24…",
            Self::RomPaletteSupportedTransferNotice => {
                "TPL/RGB transfer uses retained installed-to-supported ordering; an automatic same-name .palmask preserves unselected colors and clears selected row-zero entries 1–15."
            }
            Self::CustomSpriteEditorTitle => "Custom Sprite Placement Editor",
            Self::CustomSpritePlacementsFormat => "Synchronized placements: {count}",
            Self::CustomSpritePlacement => "Placement",
            Self::CustomSpriteRecordsNotice => {
                "One complete variable-width sprite record per line:"
            }
            Self::CustomSpriteDescriptionNotice => "Description (one line, UTF-8):",
            Self::CustomSpriteCopyPlacement => "Copy placement",
            Self::CustomSpritePastePlacement => "Paste placement",
            Self::CustomSpriteHeaderHex => "Header (hex)",
            Self::CustomSpriteApplyHeader => "Apply header",
            Self::CustomSpriteSearch => "Unicode description search",
            Self::CustomSpriteReplaceSelected => "Replace selected",
            Self::CustomSpriteRemoveSelected => "Remove selected",
            Self::CustomSpriteInsertAt => "Insert form at index",
            Self::CustomSpriteMoveTo => "Move selected to index",
            Self::CustomSpriteUtf8Bom => "UTF-8 BOM",
            Self::CustomSpriteCrlf => "CRLF (off = LF)",
            Self::CustomSpriteTrailingLineEnding => "Trailing line ending",
            Self::CustomSpriteApplyFraming => "Apply description framing",
            Self::CustomSpriteUndo => "Undo",
            Self::CustomSpriteRedo => "Redo",
            Self::CustomSpriteSavePair => "Save paired files",
            Self::CustomSpriteModified => "Modified",
            Self::CustomSpriteSaved => "Saved",
            Self::CustomSpriteDiscardTitle => "Unsaved custom-sprite library",
            Self::CustomSpriteUnsavedNotice => "Discard unsaved synchronized placement changes?",
            Self::CustomSpriteCancel => "Cancel",
            Self::CustomSpriteDiscard => "Discard",
            Self::CustomSpriteErrorTitle => "Custom-sprite editor error",
            Self::CustomSpriteOk => "OK",
            Self::CustomObjectEditorTitle => "Custom Object Library Editor",
            Self::CustomObjectEntriesFormat => "Synchronized entries: {count}",
            Self::CustomObjectSearch => "Unicode description search",
            Self::CustomObjectEntry => "Entry",
            Self::CustomObjectBytesNotice => "Object-group bytes (separate records with ';'):",
            Self::CustomObjectDescriptionNotice => "Description (one line, UTF-8):",
            Self::CustomObjectCopy => "Copy object",
            Self::CustomObjectPaste => "Paste object",
            Self::CustomObjectReplaceSelected => "Replace selected",
            Self::CustomObjectRemoveSelected => "Remove selected",
            Self::CustomObjectInsertAt => "Insert form at index",
            Self::CustomObjectMoveTo => "Move selected to index",
            Self::CustomObjectUtf8Bom => "UTF-8 BOM",
            Self::CustomObjectCrlf => "CRLF (off = LF)",
            Self::CustomObjectTrailingLineEnding => "Trailing line ending",
            Self::CustomObjectApplyFraming => "Apply description framing",
            Self::CustomObjectUndo => "Undo",
            Self::CustomObjectRedo => "Redo",
            Self::CustomObjectSavePair => "Save paired files",
            Self::CustomObjectModified => "Modified",
            Self::CustomObjectSaved => "Saved",
            Self::CustomObjectDiscardTitle => "Unsaved custom-object library",
            Self::CustomObjectUnsavedNotice => "Discard unsaved synchronized library changes?",
            Self::CustomObjectCancel => "Cancel",
            Self::CustomObjectDiscard => "Discard",
            Self::CustomObjectErrorTitle => "Custom-object editor error",
            Self::CustomObjectOk => "OK",
            Self::AppearanceEditorTitle => "Portable Entity Appearance Editor",
            Self::AppearancePainterRecordsFormat => "Painter-ordered records: {count}",
            Self::AppearanceSelected => "Selected",
            Self::AppearanceSourceLayer1 => "Layer 1 object",
            Self::AppearanceSourceLayer2 => "Layer 2 object",
            Self::AppearanceSourceSprite => "Sprite",
            Self::AppearanceSourceIdHex => "Source ID (hex)",
            Self::AppearanceTileIndexHex => "Tile index (hex)",
            Self::AppearanceXOffsetDecimal => "X offset (decimal)",
            Self::AppearanceYOffsetDecimal => "Y offset (decimal)",
            Self::AppearancePaletteRow => "Palette row",
            Self::AppearanceHorizontalFlip => "Horizontal flip",
            Self::AppearanceVerticalFlip => "Vertical flip",
            Self::AppearanceReplaceSelected => "Replace selected",
            Self::AppearanceRemoveSelected => "Remove selected",
            Self::AppearanceInsertBefore => "Insert form before index",
            Self::AppearanceMoveBefore => "Move selected before index",
            Self::AppearanceUndo => "Undo",
            Self::AppearanceRedo => "Redo",
            Self::AppearanceSave => "Save",
            Self::AppearanceModified => "Modified",
            Self::AppearanceSaved => "Saved",
            Self::AppearanceDiscardTitle => "Unsaved entity appearances",
            Self::AppearanceUnsavedNotice => "Discard unsaved appearance changes?",
            Self::AppearanceCancel => "Cancel",
            Self::AppearanceDiscard => "Discard",
            Self::AppearanceErrorTitle => "Appearance editor error",
            Self::AppearanceOk => "OK",
            Self::Layer3DocumentEditorTitle => "Portable Layer 3 Editor",
            Self::Layer3DocumentStartPosition => "Start position",
            Self::Layer3DocumentTilemapSize => "Tilemap size",
            Self::Layer3DocumentLiquidType => "Liquid/type",
            Self::Layer3DocumentRawFlags => "Raw flags",
            Self::Layer3DocumentGraphicsFormat => "Graphics {slot}",
            Self::Layer3DocumentReservedNotice => "Reserved bytes (exactly 16 hexadecimal bytes):",
            Self::Layer3DocumentTilemapNotice => "Raw tilemap bytes (maximum 0x2000):",
            Self::Layer3DocumentRemapNotice => "Literal remap-command bytes (maximum 0x10000):",
            Self::Layer3DocumentApplyAll => "Apply all Layer 3 fields atomically",
            Self::Layer3DocumentCopyTilemap => "Copy tilemap",
            Self::Layer3DocumentPasteTilemap => "Paste tilemap",
            Self::Layer3DocumentCopyRemap => "Copy remap commands",
            Self::Layer3DocumentPasteRemap => "Paste remap commands",
            Self::Layer3DocumentUndo => "Undo",
            Self::Layer3DocumentRedo => "Redo",
            Self::Layer3DocumentSave => "Save",
            Self::Layer3DocumentModified => "Modified",
            Self::Layer3DocumentSaved => "Saved",
            Self::Layer3DocumentDiscardTitle => "Unsaved Layer 3 document",
            Self::Layer3DocumentUnsavedNotice => "Discard unsaved Layer 3 changes?",
            Self::Layer3DocumentCancel => "Cancel",
            Self::Layer3DocumentDiscard => "Discard",
            Self::Layer3DocumentErrorTitle => "Layer 3 editor error",
            Self::Layer3DocumentOk => "OK",
            Self::MetadataEditorTitle => "Portable Overworld Metadata Editor",
            Self::MetadataLevelNames => "Level names",
            Self::MetadataPlayerStarts => "Player starts",
            Self::MetadataSubmapSettings => "Submap settings",
            Self::MetadataUndo => "Undo",
            Self::MetadataRedo => "Redo",
            Self::MetadataSave => "Save",
            Self::MetadataModified => "Modified",
            Self::MetadataSaved => "Saved",
            Self::MetadataLevelNameRecord => "Level-name record",
            Self::MetadataLevelKeyHex => "Level key (hex)",
            Self::MetadataTileBytesHex => "19 tile bytes (hex)",
            Self::MetadataRawFlagsHex => "Raw flags (hex)",
            Self::MetadataPlayerStartRecord => "Player-start record",
            Self::MetadataPlayerKeyHex => "Player key (hex)",
            Self::MetadataXHex => "X (hex)",
            Self::MetadataYHex => "Y (hex)",
            Self::MetadataSettingsRecord => "Settings record",
            Self::MetadataMusicHex => "Music (hex)",
            Self::MetadataPaletteHex => "Palette (hex)",
            Self::MetadataLayer1ScrollHex => "Layer 1 scroll (hex)",
            Self::MetadataLayer2ScrollHex => "Layer 2 scroll (hex)",
            Self::MetadataUnknownBytesHex => "5 unknown bytes (hex)",
            Self::MetadataUpsertName => "Upsert name",
            Self::MetadataUpsertStart => "Upsert start",
            Self::MetadataUpsertSettings => "Upsert settings",
            Self::MetadataRemoveSelected => "Remove selected",
            Self::MetadataSubmapMain => "Main map",
            Self::MetadataSubmapYoshiIsland => "Yoshi's Island",
            Self::MetadataSubmapVanillaDome => "Vanilla Dome",
            Self::MetadataSubmapForestIllusion => "Forest of Illusion",
            Self::MetadataSubmapValleyBowser => "Valley of Bowser",
            Self::MetadataSubmapSpecialWorld => "Special World",
            Self::MetadataSubmapStarWorld => "Star World",
            Self::MetadataDiscardTitle => "Unsaved overworld metadata",
            Self::MetadataUnsavedNotice => "Discard unsaved metadata changes?",
            Self::MetadataCancel => "Cancel",
            Self::MetadataDiscard => "Discard",
            Self::MetadataErrorTitle => "Metadata editor error",
            Self::MetadataOk => "OK",
            Self::OscEditorTitle => "Lossless OSC Custom-Object Metadata",
            Self::OscSourceSummaryFormat => {
                "Lossless source: {bytes} bytes; valid metadata records: {records}"
            }
            Self::OscReplaceSource => "Replace complete lossless source",
            Self::OscDiagnosticsHeading => "Recovered-record diagnostics",
            Self::OscParsedRecord => "Parsed record",
            Self::OscNoMetadataRecords => "No valid metadata records.",
            Self::OscUndo => "Undo",
            Self::OscRedo => "Redo",
            Self::OscSave => "Save",
            Self::OscModified => "Modified",
            Self::OscSaved => "Saved",
            Self::OscDiscardTitle => "Unsaved OSC sidecar",
            Self::OscUnsavedNotice => "Discard unsaved custom-object metadata changes?",
            Self::OscCancel => "Cancel",
            Self::OscDiscard => "Discard",
            Self::OscErrorTitle => "OSC sidecar error",
            Self::OscOk => "OK",
            Self::SscEditorTitle => "Lossless SSC Custom-Sprite Metadata",
            Self::SscSourceSummaryFormat => {
                "Lossless source: {bytes} bytes; valid metadata records: {records}"
            }
            Self::SscAssetsSummaryFormat => {
                "External sprite assets: {loaded}/{total} graphics slots; palette {palette}"
            }
            Self::SscPaletteLoaded => "loaded",
            Self::SscPaletteMissing => "not found",
            Self::SscReplaceSource => "Replace complete lossless source",
            Self::SscDiagnosticsHeading => "Recovered-record diagnostics",
            Self::SscParsedRecord => "Parsed record",
            Self::SscNoMetadataRecords => "No valid metadata records.",
            Self::SscUndo => "Undo",
            Self::SscRedo => "Redo",
            Self::SscSave => "Save",
            Self::SscModified => "Modified",
            Self::SscSaved => "Saved",
            Self::SscDiscardTitle => "Unsaved SSC sidecar",
            Self::SscUnsavedNotice => "Discard unsaved custom-sprite metadata changes?",
            Self::SscCancel => "Cancel",
            Self::SscDiscard => "Discard",
            Self::SscErrorTitle => "SSC sidecar error",
            Self::SscOk => "OK",
            Self::DscEditorTitle => "Lossless DSC Sidecar Editor",
            Self::DscSourceSummaryFormat => {
                "Lossless source: {bytes} bytes; valid parsed records: {records}"
            }
            Self::DscSourceNotice => {
                "Complete source bytes (malformed lines, BOM, line endings, and non-UTF-8 retained):"
            }
            Self::DscReplaceSource => "Replace complete lossless source",
            Self::DscDiagnosticsHeading => "Read-only recovered-record diagnostics",
            Self::DscParsedRecord => "Parsed record",
            Self::DscNoRecoveredRecords => {
                "No valid recovered records; all source bytes remain preserved."
            }
            Self::DscUndo => "Undo",
            Self::DscRedo => "Redo",
            Self::DscSave => "Save",
            Self::DscModified => "Modified",
            Self::DscSaved => "Saved",
            Self::DscDiscardTitle => "Unsaved DSC sidecar",
            Self::DscUnsavedNotice => "Discard unsaved lossless source changes?",
            Self::DscCancel => "Cancel",
            Self::DscDiscard => "Discard",
            Self::DscErrorTitle => "DSC sidecar error",
            Self::DscOk => "OK",
            Self::TilemapTitleScreenName => "Title-Screen Tilemap",
            Self::TilemapCreditsName => "Credits Tilemap",
            Self::TilemapEditorTitleFormat => "ROM {tilemap}",
            Self::TilemapDimensionsFormat => {
                "Exact {columns}×{rows} native tile words. Coordinates and values are hexadecimal."
            }
            Self::TilemapStaleNotice => {
                "The ROM changed after this tilemap was opened. Reopen before committing."
            }
            Self::TilemapRow => "Row",
            Self::TilemapColumn => "Column",
            Self::TilemapPlane => "Plane",
            Self::TilemapPrimary => "Primary",
            Self::TilemapSecondary => "Secondary",
            Self::TilemapWord => "Tile word",
            Self::TilemapLoadTile => "Load tile",
            Self::TilemapApplyTile => "Apply tile",
            Self::TilemapCommit => "Commit tilemap to ROM",
            Self::TilemapStaged => "Staged",
            Self::TilemapUnchanged => "Unchanged",
            Self::TilemapDiscardTitleFormat => "Discard {tilemap} changes?",
            Self::TilemapUnsavedNotice => "The staged tilemap has not been committed to the ROM.",
            Self::TilemapErrorTitleFormat => "{tilemap} editor error",
            Self::EventNumberEditorTitle => "ROM Overworld Event-Number Map",
            Self::EventNumberDescription => {
                "Complete 256-entry event-number mapping. Values are hexadecimal bytes."
            }
            Self::EventNumberStoredLengthFormat => "Current native stored length: {length}",
            Self::EventNumberStaleNotice => {
                "The ROM changed after this map was opened. Reopen before committing."
            }
            Self::EventNumberEvent => "Event",
            Self::EventNumberMappedEvent => "Mapped event",
            Self::EventNumberLoadEntry => "Load entry",
            Self::EventNumberApplyEntry => "Apply entry",
            Self::EventNumberCommit => "Commit map to ROM",
            Self::EventNumberStaged => "Staged",
            Self::EventNumberUnchanged => "Unchanged",
            Self::EventNumberDiscardTitle => "Discard event-number changes?",
            Self::EventNumberUnsavedNotice => {
                "The staged mapping has not been committed to the ROM."
            }
            Self::EventNumberErrorTitle => "Event-number editor error",
            Self::LevelNameEditorTitle => "ROM Overworld Level Names",
            Self::LevelNameDescription => {
                "Lossless 19-tile level-name records. Level, tile index, and value are hexadecimal."
            }
            Self::LevelNameCountFormat => "Staged name records: {count}",
            Self::LevelNameStaleNotice => {
                "The ROM changed after this table was opened. Reopen before committing."
            }
            Self::LevelNameLevel => "Level",
            Self::LevelNameTile => "Tile (00–12)",
            Self::LevelNameTileValue => "Tile value",
            Self::LevelNameLoadTile => "Load tile",
            Self::LevelNameApplyTile => "Apply tile",
            Self::LevelNameCommit => "Commit names to ROM",
            Self::LevelNameStaged => "Staged",
            Self::LevelNameUnchanged => "Unchanged",
            Self::LevelNameDiscardTitle => "Discard level-name changes?",
            Self::LevelNameUnsavedNotice => {
                "The staged level names have not been committed to the ROM."
            }
            Self::LevelNameErrorTitle => "Level-name editor error",
            Self::PlayerStartEditorTitle => "ROM Overworld Player Starts",
            Self::PlayerStartDescription => {
                "Exact two-player native start records. Coordinates are hexadecimal."
            }
            Self::PlayerStartReservedFormat => "Preserved adjacent option bytes: {bytes}",
            Self::PlayerStartStaleNotice => {
                "The ROM changed after these starts were opened. Reopen before committing."
            }
            Self::PlayerStartPlayer => "Player",
            Self::PlayerStartMario => "Mario",
            Self::PlayerStartLuigi => "Luigi",
            Self::PlayerStartLoad => "Load",
            Self::PlayerStartX => "X",
            Self::PlayerStartY => "Y",
            Self::PlayerStartSubmap => "Submap",
            Self::PlayerStartInvalid => "Invalid",
            Self::PlayerStartMainMap => "Main Map",
            Self::PlayerStartYoshisIsland => "Yoshi's Island",
            Self::PlayerStartVanillaDome => "Vanilla Dome",
            Self::PlayerStartForestIllusion => "Forest of Illusion",
            Self::PlayerStartValleyBowser => "Valley of Bowser",
            Self::PlayerStartSpecialWorld => "Special World",
            Self::PlayerStartStarWorld => "Star World",
            Self::PlayerStartApply => "Apply player",
            Self::PlayerStartCommit => "Commit starts to ROM",
            Self::PlayerStartStaged => "Staged",
            Self::PlayerStartUnchanged => "Unchanged",
            Self::PlayerStartDiscardTitle => "Discard player-start changes?",
            Self::PlayerStartUnsavedNotice => {
                "The staged start records have not been committed to the ROM."
            }
            Self::PlayerStartErrorTitle => "Player-start editor error",
            Self::SpecialEventEditorTitle => "ROM Overworld Special Events",
            Self::SpecialEventDescription => {
                "All 24 native special-event reveal records. Values are hexadecimal."
            }
            Self::SpecialEventStaleNotice => {
                "The ROM changed after this table was opened. Reopen before committing."
            }
            Self::SpecialEventIndex => "Index",
            Self::SpecialEventSourceTile => "Source tile",
            Self::SpecialEventDestinationTile => "Destination tile",
            Self::SpecialEventDirection => "Direction",
            Self::SpecialEventLoadEntry => "Load entry",
            Self::SpecialEventApplyEntry => "Apply entry",
            Self::SpecialEventCommit => "Commit table to ROM",
            Self::SpecialEventStaged => "Staged",
            Self::SpecialEventUnchanged => "Unchanged",
            Self::SpecialEventDiscardTitle => "Discard special-event changes?",
            Self::SpecialEventUnsavedNotice => {
                "The staged event table has not been committed to the ROM."
            }
            Self::SpecialEventErrorTitle => "Special-event editor error",
            Self::EventRevealEditorTitle => "ROM Overworld Event Reveals",
            Self::EventRevealDescription => {
                "Complete mixed-endian source/destination reveal table. Hexadecimal."
            }
            Self::EventRevealCountFormat => "Staged reveal records: {count}",
            Self::EventRevealStaleNotice => {
                "The ROM changed after this table was opened. Reopen before committing."
            }
            Self::EventRevealIndex => "Index",
            Self::EventRevealSourceTile => "Source tile (000–7FF)",
            Self::EventRevealDestinationTile => "Destination tile",
            Self::EventRevealTableCount => "Table count (01–FF)",
            Self::EventRevealResizeTable => "Resize table",
            Self::EventRevealLoad => "Load reveal",
            Self::EventRevealApply => "Apply reveal",
            Self::EventRevealCommit => "Commit reveals to ROM",
            Self::EventRevealStaged => "Staged",
            Self::EventRevealUnchanged => "Unchanged",
            Self::EventRevealDiscardTitle => "Discard event-reveal changes?",
            Self::EventRevealUnsavedNotice => "The staged reveal table has not been committed.",
            Self::EventRevealErrorTitle => "Event-reveal editor error",
            Self::EventTilemapEditorTitle => "ROM Overworld Event Tilemaps",
            Self::EventTilemapDescription => {
                "All 2,048 tiles in the primary low/high and secondary high-byte planes."
            }
            Self::EventTilemapLoadedStorageFormat => "Loaded storage: {storage}",
            Self::EventTilemapPristineStorage => "pristine zero workspaces",
            Self::EventTilemapInstalledStorage => "installed compressed streams",
            Self::EventTilemapStaleNotice => {
                "The ROM changed after these buffers were opened. Reopen before committing."
            }
            Self::EventTilemapTileIndex => "Tile index (000–7FF)",
            Self::EventTilemapPlane => "Plane",
            Self::EventTilemapPrimaryLow => "Primary low byte",
            Self::EventTilemapPrimaryHigh => "Primary high byte",
            Self::EventTilemapSecondaryHigh => "Secondary high byte",
            Self::EventTilemapByteValue => "Byte value",
            Self::EventTilemapLoadByte => "Load byte",
            Self::EventTilemapApplyByte => "Apply byte",
            Self::EventTilemapCommit => "Commit tilemaps to ROM",
            Self::EventTilemapStaged => "Staged",
            Self::EventTilemapUnchanged => "Unchanged",
            Self::EventTilemapDiscardTitle => "Discard event-tilemap changes?",
            Self::EventTilemapUnsavedNotice => {
                "The staged tilemap buffers have not been committed."
            }
            Self::EventTilemapErrorTitle => "Event-tilemap editor error",
            Self::OverworldSettingsEditorTitle => "ROM Overworld Global Settings",
            Self::OverworldSettingsDescription => {
                "Seven lossless 16-word special settings records. Values are hexadecimal."
            }
            Self::OverworldSettingsInstalled => "Expanded settings are installed.",
            Self::OverworldSettingsPristine => {
                "Pristine defaults; committing installs the recovered expanded-settings runtime."
            }
            Self::OverworldSettingsStaleNotice => {
                "The ROM changed after these settings were opened. Reopen before committing."
            }
            Self::OverworldSettingsSubmapRecord => "Submap record",
            Self::OverworldSettingsLoad => "Load",
            Self::OverworldSettingsWordFormat => "Word {index}",
            Self::OverworldSettingsLayer3Header => "Semantic Layer 3 settings",
            Self::OverworldSettingsUseCustomTilemap => "Use custom tilemap",
            Self::OverworldSettingsUseCustomGraphics => "Use custom graphics",
            Self::OverworldSettingsTilemapFile => "Tilemap file",
            Self::OverworldSettingsTilemapSize => "Tilemap size",
            Self::OverworldSettingsTilemapPosition => "Tilemap position",
            Self::OverworldSettingsAddressLayoutWords => "Address-layout words",
            Self::OverworldSettingsGraphicsFiles => "Graphics files",
            Self::OverworldSettingsGfxFormat => "GFX {index}",
            Self::OverworldSettingsApplyLayer3 => "Apply Layer 3 fields",
            Self::OverworldSettingsPreservationNotice => {
                "Semantic edits preserve opaque feature bits, reserved bytes, and high graphics-word nibbles."
            }
            Self::OverworldSettingsApplyRecord => "Apply record",
            Self::OverworldSettingsCommit => "Commit settings to ROM",
            Self::OverworldSettingsStaged => "Staged",
            Self::OverworldSettingsUnchanged => "Unchanged",
            Self::OverworldSettingsDiscardTitle => "Discard overworld-settings changes?",
            Self::OverworldSettingsUnsavedNotice => {
                "The staged settings have not been committed to the ROM."
            }
            Self::OverworldSettingsErrorTitle => "Overworld-settings editor error",
            Self::SecondaryExitDescription => {
                "Global 8,192-entry native table. Values are hexadecimal."
            }
            Self::SecondaryExitStaleNotice => {
                "The ROM changed after this table was opened. Reopen before committing."
            }
            Self::SecondaryExitEntry => "Entry",
            Self::SecondaryExitLoad => "Load",
            Self::SecondaryExitPositionMethod => "Position/method",
            Self::SecondaryExitDestinationFlags => "Destination flags",
            Self::SecondaryExitXOverworldFlags => "X/overworld flags",
            Self::SecondaryExitAdditionalFlags => "Additional flags",
            Self::SecondaryExitApplyEntry => "Apply entry",
            Self::SecondaryExitCommit => "Commit table to ROM",
            Self::SecondaryExitStaged => "Staged",
            Self::SecondaryExitUnchanged => "Unchanged",
            Self::SecondaryExitClearAllTitle => "Clear all secondary exits?",
            Self::SecondaryExitClearAllNotice => {
                "This stages 8,192 cleared entries. The ROM is unchanged until commit."
            }
            Self::SecondaryExitClearAll => "Clear all",
            Self::SecondaryExitDiscardTitle => "Discard staged secondary exits?",
            Self::SecondaryExitUnsavedNotice => {
                "The staged global table has not been committed to the ROM."
            }
            Self::SecondaryExitErrorTitle => "Secondary-exit editor error",
            Self::SharedPaletteEditorTitle => "Native Shared/Custom SMW Palettes",
            Self::SharedPaletteSummaryFormat => {
                "{backend} backend · {colors} colors · exact native .smwpal ordering"
            }
            Self::SharedPaletteStaleNotice => {
                "The ROM changed after this palette was opened. Reopen before committing."
            }
            Self::SharedPaletteImport => "Import complete .smwpal…",
            Self::SharedPaletteExport => "Export complete .smwpal…",
            Self::SharedPaletteTransferNotice => {
                "Complete transfer preserves exact legacy or expanded native byte ordering."
            }
            Self::SharedPalettePage => "Page",
            Self::SharedPalettePageOfFormat => "of {last}",
            Self::SharedPaletteSelectedFormat => "Selected color ${index}",
            Self::SharedPaletteBgr555 => "SNES BGR555",
            Self::SharedPaletteDecodeRaw => "Decode raw",
            Self::SharedPaletteRed => "Red",
            Self::SharedPaletteGreen => "Green",
            Self::SharedPaletteBlue => "Blue",
            Self::SharedPalettePreview => "████ Preview",
            Self::SharedPaletteApplyRgb => "Apply RGB color",
            Self::SharedPaletteApplyRaw => "Apply raw word",
            Self::SharedPaletteCopyRow => "Copy row",
            Self::SharedPalettePasteRow => "Paste row",
            Self::SharedPaletteCopyColor => "Copy color",
            Self::SharedPalettePasteColor => "Paste color",
            Self::SharedPaletteClipboardNotice => {
                "Ctrl+left/right uses the swatches; add Alt for a complete row."
            }
            Self::SharedPaletteAuxiliaryBytes => "Expanded auxiliary bytes",
            Self::SharedPaletteStageAuxiliary => "Stage auxiliary bytes",
            Self::SharedPaletteCommit => "Commit palette to ROM",
            Self::SharedPaletteStaged => "Staged",
            Self::SharedPaletteUnchanged => "Unchanged",
            Self::SharedPaletteDiscardTitle => "Discard shared-palette changes?",
            Self::SharedPaletteUnsavedNotice => {
                "The staged shared/custom palette has not been committed to the ROM."
            }
            Self::SharedPaletteErrorTitle => "Shared-palette editor error",
            Self::GraphicsExternalRunningTitle => "External graphics editor running",
            Self::GraphicsExternalWaitingFormat => "Waiting for {path}",
            Self::GraphicsExternalReloadNotice => {
                "The staged file will reload after the editor exits successfully."
            }
            Self::GraphicsExternalConsentTitle => "Open staged graphics externally?",
            Self::GraphicsExternalExecutableFormat => "Executable: {path}",
            Self::GraphicsExternalStagedFileFormat => "Staged file: {path}",
            Self::GraphicsExternalArgumentsNotice => {
                "Arguments are passed directly without a command shell:"
            }
            Self::GraphicsExternalArgumentFormat => "argument[{index}] = {argument}",
            Self::GraphicsExternalRun => "Run editor",
            Self::GraphicsOwnershipEditable => "Ownership: editable",
            Self::GraphicsOwnershipFixed => "Ownership: fixed (read-only)",
            Self::GraphicsOwnershipExAnimationFormat => {
                "Ownership: ExAnimation record {record} (read-only)"
            }
            Self::GraphicsOwnershipOriginalAnimationFormat => {
                "Ownership: original animation slot {slot} (read-only)"
            }
            Self::GraphicsOwnershipLevelExAnimationFormat => {
                "Ownership: level ExAnimation slot {slot} (read-only)"
            }
            Self::GraphicsOwnershipGlobalExAnimationFormat => {
                "Ownership: global ExAnimation slot {slot} (read-only)"
            }
            Self::GraphicsOwnershipInvalid => "Ownership: invalid (read-only)",
            Self::GraphicsDiscardTitle => "Discard staged graphics changes?",
            Self::GraphicsUnsavedNotice => "These changes have not been committed to the ROM.",
            Self::GraphicsErrorTitle => "ROM graphics error",
            Self::GraphicsEditorTitle => "ROM Graphics Editor",
            Self::PortableGraphicsEditorTitle => "Portable Graphics Editor",
            Self::PortableGraphicsDiscardTitle => "Unsaved graphics",
            Self::PortableGraphicsUnsavedNotice => "Discard unsaved graphics changes?",
            Self::PortableGraphicsErrorTitle => "Graphics error",
            Self::PortableGraphicsUndo => "Undo",
            Self::PortableGraphicsRedo => "Redo",
            Self::PortableGraphicsSave => "Save",
            Self::PortableGraphicsCopyTile => "Copy tile",
            Self::PortableGraphicsPasteTile => "Paste tile",
            Self::PortableGraphicsModified => "Modified",
            Self::PortableGraphicsSaved => "Saved",
            Self::PortableGraphicsNoTiles => "No graphics tiles",
            Self::PortableGraphicsTileFormat => "Tile {tile}",
            Self::PortableGraphicsCancel => "Cancel",
            Self::PortableGraphicsDiscard => "Discard",
            Self::PortableGraphicsOk => "OK",
            Self::GraphicsRotateClockwise => "Rotate 90°",
            Self::GraphicsFlipHorizontal => "Flip horizontal",
            Self::GraphicsFlipVertical => "Flip vertical",
            Self::GraphicsPreviousPage => "Previous page",
            Self::GraphicsNextPage => "Next page",
            Self::GraphicsPreviousPalette => "Previous palette",
            Self::GraphicsNextPalette => "Next palette",
            Self::GraphicsColorMapFilters => "Color-map filters…",
            Self::GraphicsApplyColorMapFilter => "Apply color-map filter",
            Self::GraphicsFilterFormat => "Filter {filter}",
            Self::GraphicsStaleNotice => "The ROM changed; reopen before editing or committing.",
            Self::GraphicsPaletteRow => "Palette row",
            Self::GraphicsDefaultPalette => "Default",
            Self::GraphicsUseJoined => "Use joined AllGFX.bin files",
            Self::GraphicsJoinedNotice => "Original global joined-GFX mode (command $24BD)",
            Self::GraphicsConfiguredEditor => "Configured graphics editor",
            Self::GraphicsNone => "None",
            Self::GraphicsEditConfigured => "Edit with configured tool",
            Self::GraphicsEditExecutable => "Edit with executable…",
            Self::GraphicsInsertRaw => "Insert raw GFX/ExGFX…",
            Self::GraphicsExtractRaw => "Extract raw GFX/ExGFX…",
            Self::GraphicsExtractLevel => "Extract current level GFX…",
            Self::GraphicsExtractLevelNotice => {
                "Choose a new directory for the active level's decoded FG/BG/SP files"
            }
            Self::GraphicsExtractStandard => "Extract all standard GFX…",
            Self::GraphicsExtractSpecial => "Extract GFX32/GFX33…",
            Self::GraphicsSpecialNotice => {
                "Uses the authenticated pristine SMW special-pointer operands"
            }
            Self::GraphicsExtractExGfx => "Extract installed ExGFX…",
            Self::GraphicsExtractExGfxNotice => {
                "Exports every nonempty ExGFX pointer from the installed table"
            }
            Self::GraphicsExtractAllGfx => "Extract AllGFX.bin…",
            Self::GraphicsInsertStandard => "Insert all standard GFX…",
            Self::GraphicsStagedEditNotice => {
                "Commit or discard staged tile edits before inserting a directory"
            }
            Self::GraphicsInsertSpecial => "Insert GFX32/GFX33…",
            Self::GraphicsInsertExGfx => "Insert ExGFX…",
            Self::GraphicsInsertExGfxNotice => {
                "Atomically inserts the canonical ExGFX files found in a directory"
            }
            Self::GraphicsInsertAllGfx => "Insert AllGFX.bin…",
            Self::GraphicsAllocationPc => "Allocation logical PC hex",
            Self::GraphicsAllocationRangeSeparator => "..",
            Self::GraphicsCommit => "Commit graphics to ROM",
            Self::GraphicsCommitReclaim => "Commit and reclaim",
            Self::GraphicsStagedChanges => "Staged graphics changes",
            Self::GraphicsNoStagedChanges => "No staged changes",
            Self::GraphicsInternalCacheNotice => {
                "Internal GFX data — transient working cache; F9 publishes current-level FG/BG/SP slots"
            }
            Self::GraphicsSaveLevelTitle => "Save level GFX to Graphics folder?",
            Self::GraphicsSaveLevelQuestion => "Do you want to save the current level GFX to file,",
            Self::GraphicsSaveLevelPurpose => "so it can be inserted to the ROM later?",
            Self::GraphicsSaveLevelWarning => {
                "Don't do this if you haven't extracted the graphics yet!"
            }
            Self::GraphicsNoTiles => "No graphics tiles",
            Self::GraphicsTileFormat => "Tile {index}",
            Self::GraphicsInternalTileNotice => {
                "Internal working-cache tile; edits are transient unless F9 owns its current-level file."
            }
            Self::GraphicsCopyTile => "Copy tile",
            Self::GraphicsPasteTile => "Paste tile",
            Self::GraphicsFormatWarningTitle => "Graphics Format Change Warning!",
            Self::GraphicsFormatWarningBody => {
                "The GFX are about to be inserted as 4bpp, but any ExGFX already in the ROM are still stored in 3bpp format.  Make sure to re-insert the ExGFX too after this so the program can store them as 4bpp as well (if you don't yet have an external copy of them, you should cancel this and extract the ExGFX first).  Unless for some reason you actually like looking at garbled graphics...\n\nProceed anyway?"
            }
            Self::GraphicsYes => "Yes",
            Self::GraphicsNo => "No",
            Self::GraphicsExtractingFormat => "Extracting {family} GFX",
            Self::GraphicsStagingFormat => "Staging {path}",
            Self::GraphicsBatchAtomicNotice => {
                "Files become visible only after the complete set is staged."
            }
            Self::GraphicsCancellingNotice => "Cancelling after the current file…",
            Self::GraphicsInsertingFormat => "Inserting {family} GFX",
            Self::GraphicsReadingFormat => "Reading {source}",
            Self::GraphicsImportAtomicNotice => {
                "The ROM changes only after the complete set validates."
            }
            Self::GraphicsToolbarGfxCompleteTitle => "GFX Extraction Complete!",
            Self::GraphicsToolbarExGfxCompleteTitle => "ExGFX Extraction Complete!",
            Self::GraphicsToolbarGfxCompleteFormat => {
                "All GFX files have been extracted to:\n{path}"
            }
            Self::GraphicsToolbarExGfxCompleteFormat => {
                "{count} ExGFX files have been extracted to:\n{path}"
            }
            Self::GraphicsToolbarErrorTitle => "Graphics extraction error",
            Self::RomMap16EditorTitle => "ROM Complete Map16 Editor",
            Self::RomMap16StaleNotice => "The ROM changed; reopen before editing or committing.",
            Self::RomMap16PreviewLevel => "Preview level",
            Self::RomMap16ObjectSet => "Object set",
            Self::RomMap16FgPalette => "FG palette",
            Self::RomMap16Grid => "16×16 grid",
            Self::RomMap16GridNotice => "F8 toggles the grid; Ctrl+Alt+F8 switches white/black.",
            Self::RomMap16GridColor => "Grid color",
            Self::RomMap16ZoomOut => "−",
            Self::RomMap16ZoomReset => "Reset zoom",
            Self::RomMap16ZoomIn => "+",
            Self::RomMap16PageNumber => "Page number",
            Self::RomMap16PageNumberNotice => "F1 toggles page numbers.",
            Self::RomMap16LockPages => "Lock built-in pages…",
            Self::RomMap16UnlockPages => "Unlock built-in pages…",
            Self::RomMap16PreviewHexError => "Preview level must be hexadecimal.",
            Self::RomMap16PreviewRangeError => "Preview level must be between 000 and 1FF.",
            Self::RomMap16SelectionNotice => {
                "Click a rendered 16×16 tile, or drag across tiles to select a rectangle."
            }
            Self::RomMap16Page => "Page",
            Self::RomMap16Tile => "Tile",
            Self::RomMap16Quadrant => "Quadrant",
            Self::RomMap16AddressFormat => "Map16 {page}:{tile}",
            Self::RomMap16CopyTile => "Copy tile",
            Self::RomMap16PasteTile => "Paste tile",
            Self::RomMap16Undo => "Undo",
            Self::RomMap16Redo => "Redo",
            Self::RomMap16Subtile => "8×8 tile",
            Self::RomMap16Palette => "Palette",
            Self::RomMap16Priority => "Priority",
            Self::RomMap16XFlip => "X flip",
            Self::RomMap16YFlip => "Y flip",
            Self::RomMap16ApplySubtile => "Apply subtile",
            Self::RomMap16ActsLike => "Acts Like",
            Self::RomMap16ApplyActsLike => "Apply Acts Like",
            Self::RomMap16NoActsLikeNotice => {
                "Background Map16 definitions do not have Acts-Like values."
            }
            Self::RomMap16ProtectedNotice => {
                "Built-in pages 00–01 are protected. Use Ctrl+F1 to unlock them."
            }
            Self::RomMap16UnlockTitle => "Unlock built-in Map16 pages?",
            Self::RomMap16LockTitle => "Lock built-in Map16 pages?",
            Self::RomMap16UnlockWarning => {
                "Pages 00–01 contain built-in game definitions. Editing them can affect many levels and imported files continue to preserve their graphics words."
            }
            Self::RomMap16LockQuestion => "Lock pages 00–01 against further manual edits?",
            Self::RomMap16Unlock => "Unlock",
            Self::RomMap16Lock => "Lock",
            Self::RomMap16AllocationPc => "Allocation logical PC hex",
            Self::RomMap16AllocationSeparator => "..",
            Self::RomMap16Commit => "Commit complete Map16 set to ROM",
            Self::RomMap16CommitReclaim => "Commit and reclaim",
            Self::RomMap16Staged => "Staged Map16 changes",
            Self::RomMap16Unchanged => "No staged changes",
            Self::RomMap16DiscardTitle => "Discard staged Map16 changes?",
            Self::RomMap16UnsavedNotice => {
                "These Map16 changes or bitmap import have not been committed to the ROM."
            }
            Self::RomMap16ErrorTitle => "ROM Map16 error",
            Self::RomMap16TransferImportComplete => "Import complete .map16…",
            Self::RomMap16TransferExportComplete => "Export complete .map16…",
            Self::RomMap16TransferTemplateNotice => {
                "Export preserves auxiliary and editor-state sections from the imported file."
            }
            Self::RomMap16TransferNativeOnlyNotice => {
                "Complete Lunar Magic .map16 transfer requires the native 256-page SMW workspace."
            }
            Self::RomMap16TransferSelectedWidth => "Selected range width",
            Self::RomMap16TransferSelectedHeight => "height",
            Self::RomMap16TransferFileOrigin => "Import at file origin",
            Self::RomMap16TransferImportSelected => "Import selected .map16…",
            Self::RomMap16TransferExportSelected => "Export selected .map16…",
            Self::RomMap16TransferCopyRectangle => "Copy rectangle",
            Self::RomMap16TransferPasteRectangle => "Paste rectangle",
            Self::RomMap16TransferSelectedNotice => {
                "Selected .map16 ranges use Lunar Magic's compact LM16 width, height, origin, band flags, definitions, and Acts Like sections. Width and height are hexadecimal; disable file-origin import to place at the selected tile."
            }
            Self::RomMap16TransferImportPage => "Import legacy page pair…",
            Self::RomMap16TransferExportPage => "Export legacy page pair…",
            Self::RomMap16TransferPageNotice => {
                "Legacy transfer atomically reads or creates Map16Page.bin (definitions) and Map16PageG.bin (Acts Like) for the selected foreground page."
            }
            Self::RomMap16TransferPageUnsupportedNotice => {
                "Legacy page pairs apply only to editable foreground pages 02–7F; built-in pages 00–01 and background pages use other Lunar Magic boundaries."
            }
            Self::RomMap16TransferImportForeground => "Import legacy foreground pair…",
            Self::RomMap16TransferExportForeground => "Export legacy foreground pair…",
            Self::RomMap16TransferImportBackground => "Import legacy background…",
            Self::RomMap16TransferExportBackground => "Export legacy background…",
            Self::RomMap16TransferLegacyCompleteNotice => {
                "Legacy complete transfer uses Map16FG.bin/Map16FGG.bin for all 128 foreground pages and Map16BG.bin for all 128 background pages."
            }
            Self::RomMap16SidecarHeading => "Associated custom Map16",
            Self::RomMap16SidecarExportM16 => "Export .m16",
            Self::RomMap16SidecarExportS16 => "Export .s16",
            Self::RomMap16SidecarConfirmTitle => "Export associated Map16 sidecar?",
            Self::RomMap16SidecarConfirmQuestion => {
                "Write the current {extension} buffer to {path}?"
            }
            Self::RomMap16SidecarNo => "No",
            Self::RomMap16SidecarYes => "Yes",
            Self::RomMap16SnesHeading => "SNES graphics set + screen tile map",
            Self::RomMap16SnesImportPalette => "Import palette row",
            Self::RomMap16SnesPaletteRowPrefix => "row ",
            Self::RomMap16SnesOptimize => "Optimize Map16 definitions",
            Self::RomMap16SnesLoad => "Load SNES tileset…",
            Self::RomMap16SnesGraphicsOffset => "Graphics offset",
            Self::RomMap16SnesMapOffset => "Map offset",
            Self::RomMap16SnesColorFilter => "Color-map filter",
            Self::RomMap16SnesColorMap => "Color map",
            Self::RomMap16SnesNotice => {
                "Loads the original .set/.bin plus 32×32 .map workflow with an optional 16-color .col/.pal row. Preview is revision-bound and blocks conflicting Map16 work."
            }
            Self::RomMap16SnesPreviewTitle => "SNES tileset import preview",
            Self::RomMap16SnesTargetPage => "Target Map16 page: ${page}",
            Self::RomMap16SnesPlacement => "Placement: {placement}",
            Self::RomMap16SnesGraphicsTiles => "Graphics tiles: {count}",
            Self::RomMap16SnesCandidateDefinitions => "Candidate definitions: {count}",
            Self::RomMap16SnesDefinitionsWritten => "Definitions written: {count}",
            Self::RomMap16SnesIndexGridSpan => "Index-grid span: {span}",
            Self::RomMap16SnesPaletteLoaded => "Palette row loaded: ${row}",
            Self::RomMap16SnesPaletteNotLoaded => "Palette row loaded: no",
            Self::RomMap16SnesStaleNotice => "The ROM changed; discard this preview.",
            Self::RomMap16SnesPreviewNotice => {
                "The decoded graphics, optional palette, candidate page, and background index grid are retained together for the atomic ROM-application milestone."
            }
            Self::RomMap16SnesApply => "Apply graphics + palette + Map16",
            Self::RomMap16SnesDiscard => "Discard preview",
            Self::RomMap16BitmapOpeningTitle => "Opening",
            Self::RomMap16BitmapReadingClipboard => "Reading clipboard bitmap",
            Self::RomMap16BitmapTitle => "Import Bitmap as Map16",
            Self::RomMap16BitmapStaleNotice => {
                "The ROM changed. Reopen the import before committing."
            }
            Self::RomMap16BitmapOptimize8x8 => "Optimize new 8×8 tiles",
            Self::RomMap16BitmapReuse8x8 => "Reuse existing tiles",
            Self::RomMap16BitmapReservedBlank => "Use reserved Map16 tile for blank blocks",
            Self::RomMap16BitmapOptimize16x16 => "Optimize 16×16 tiles",
            Self::RomMap16BitmapLayerPriority => "Layer priority",
            Self::RomMap16BitmapConfiguredBlank => "Use configured 8×8 tile for blank source tiles",
            Self::RomMap16BitmapFirst8x8 => "First 8×8 tile",
            Self::RomMap16BitmapBlank8x8 => "Blank 8×8 tile",
            Self::RomMap16BitmapFirstMap16 => "First Map16 tile",
            Self::RomMap16BitmapReservedMap16 => "Reserved Map16 tile",
            Self::RomMap16BitmapPlan => {
                "{colors} generated colors; {tiles} newly occupied 8×8 tiles"
            }
            Self::RomMap16BitmapAllocation => {
                "{blocks} source blocks placed using {tiles} new 16×16 tiles"
            }
            Self::RomMap16BitmapExhausted => {
                "Not enough blank 16×16 tiles; only the reported prefix will be imported."
            }
            Self::RomMap16BitmapImport => "Import into ROM",
            Self::RomMap16BitmapCancel => "Cancel",
            Self::RomMap16BitmapPreviewZoom => "Preview zoom",
            Self::RomMap16BitmapResetPan => "Reset pan",
            Self::RomMap16BitmapOriginal => "Original",
            Self::RomMap16BitmapConverted => "Converted",
            Self::RomMap16BitmapHeading => "Bitmap to Map16",
            Self::RomMap16BitmapLevelNotice => {
                "The preview level and its real object tileset are used."
            }
            Self::RomMap16BitmapGfxSlot4 => "Editable GFX slot 4",
            Self::RomMap16BitmapGfxSlot5 => "slot 5",
            Self::RomMap16BitmapGfxNotice => {
                "Enter hexadecimal GFX/ExGFX file numbers. Blank slots cannot store new tiles."
            }
            Self::RomMap16BitmapChoose => "Choose PNG/BMP…",
            Self::RomMap16BitmapPaste => "Paste bitmap from clipboard",
            Self::RomMap16BitmapMaximumColors => "Maximum colors",
            Self::RomMap16BitmapPriority => "Priority",
            Self::RomMap16BitmapMedianCut => "Median Cut",
            Self::RomMap16BitmapPopularity => "Popularity",
            Self::RomMap16BitmapAllowUnmarked => "Allow modifying colors not marked reserved",
            Self::RomMap16BitmapPrioritizeExact => "Prioritize exact existing-palette matches",
            Self::RomMap16BitmapPrioritizeExactNotice => {
                "Lunar Magic 3.63 stores this checked preference, but disables its control and has no conversion-path reader"
            }
            Self::RomMap16BitmapHueTolerance => "Reusable-color hue tolerance",
            Self::RomMap16BitmapPaletteLegend => {
                "Palette entries: F = free, U = reusable, X = reserved"
            }
            Self::RomMap16BitmapUniqueColors => "Give higher priority to unique colors",
            Self::RomMap16BitmapMaintainDetail => {
                "Maintain detail (assign each bitmap color separately)"
            }
            Self::RomMap16BitmapReduceMethod1 => "Reduce colors, method 1 (for high-color images)",
            Self::RomMap16BitmapReduceMethod2 => "Reduce colors, method 2 (for high-color images)",
            Self::RomExpansionTitle => "Expand ROM",
            Self::RomExpansionTargetNotice => "Target logical ROM size in hexadecimal bytes.",
            Self::RomExpansionAlignmentNotice => {
                "The target must be larger, 32 KiB aligned, and mapper-addressable."
            }
            Self::RomExpansionLmTarget => "Lunar Magic target:",
            Self::RomExpansion2MiB => "2 MiB",
            Self::RomExpansion3MiB => "3 MiB",
            Self::RomExpansion4MiB => "4 MiB",
            Self::RomExpansionExLoRomHeading => "64-Mbit ExLoROM",
            Self::RomExpansionExLoRomNotice => {
                "Uses Lunar Magic's recovered mapper conversion, including relocation, compatibility metadata, inaccessible-bank locks, and checksum preservation."
            }
            Self::RomExpansionExLoRomConvert => "Convert to 64-Mbit ExLoROM…",
            Self::RomExpansionExLoRomRequires => {
                "Requires a checksum-valid 512 KiB–4 MiB SMW LoROM."
            }
            Self::RomExpansionSa1Heading => "SA-1 expansion",
            Self::RomExpansion6MiB => "6 MiB",
            Self::RomExpansion8MiB => "8 MiB",
            Self::RomExpansionSa1Requires => {
                "These fixed targets are available only for an SA-1 ROM."
            }
            Self::RomExpansionTarget => "Target",
            Self::RomExpansionFillByte => "Fill byte",
            Self::RomExpansionSa1FixedNotice => {
                "SA-1 ROMs must use the fixed 6 MiB or 8 MiB action above."
            }
            Self::RomExpansionCancel => "Cancel",
            Self::RomExpansionApply => "Expand transactionally",
            Self::RomExpansionExLoRomWarningTitle => "64 Mbit ExLoROM Expansion Warning",
            Self::RomExpansionMapperWarning => {
                "This changes the ROM mapper and relocates ROM data."
            }
            Self::RomExpansionCompatibilityWarning => {
                "Some external patches and tools may not support 64-Mbit ExLoROM files. Save a backup before distributing or applying third-party patches."
            }
            Self::RomExpansionUndoableNotice => {
                "The conversion is a single undoable operation in this editor."
            }
            Self::RomExpansionConvertRom => "Convert ROM",
            Self::RomExpansionSa1ConfirmTitle => "Expand SA-1 ROM?",
            Self::RomExpansionSa1ConfirmNotice => "This will expand the SA-1 ROM to {mib} MiB.",
            Self::RomExpansionSnes9xNotice => {
                "If using Snes9x, this requires version 1.54+ or FuSoYa's custom 8MB Snes9x build."
            }
            Self::RomExpansionZsnesNotice => {
                "If using Snes9x, this requires version 1.54+ or FuSoYa's custom 8MB Snes9x build. ZSNES requires FuSoYa's custom 8MB build."
            }
            Self::RomExpansionExpandRom => "Expand ROM",
            Self::RomExpansionErrorTitle => "ROM expansion error",
            Self::RomExpansionOk => "OK",
            Self::RomExpandedSettingsTitle => "ROM Expanded Settings",
            Self::RomExpandedSettingsRecordNotice => {
                "Exact installed 32-byte record; unknown words remain lossless."
            }
            Self::RomExpandedSettingsStaleNotice => {
                "The ROM changed after this editor was opened. Close and reopen it before committing."
            }
            Self::RomExpandedSettingsLayer3Heading => "Custom Layer 3 tilemap graphics",
            Self::RomExpandedSettingsLayer3Enable => "Enable custom Layer 3 tilemap",
            Self::RomExpandedSettingsGfxFile => "GFX/ExGFX file",
            Self::RomExpandedSettingsLengthSelector => "Length selector",
            Self::RomExpandedSettingsDestinationSelector => "Destination selector",
            Self::RomExpandedSettingsStageLayer3 => "Stage Layer 3 settings",
            Self::RomExpandedSettingsExpandedMode => "Expanded mode",
            Self::RomExpandedSettingsExpandedModeNotice => {
                "Exact 32-bit mode packed from the high nibbles of words 8–F."
            }
            Self::RomExpandedSettingsStageExpandedMode => "Stage Layer 3 expanded mode",
            Self::RomExpandedSettingsBypassHeading => "Super GFX Bypass",
            Self::RomExpandedSettingsBypassEnable => "Use per-level GFX/ExGFX files",
            Self::RomExpandedSettingsStageBypass => "Stage Super GFX bypass",
            Self::RomExpandedSettingsBoundaryHeading => "Sprite boundary interaction",
            Self::RomExpandedSettingsBoundaryAir => {
                "Sprites beyond level boundaries interact with air instead of water"
            }
            Self::RomExpandedSettingsStageBoundary => "Stage sprite boundary interaction",
            Self::RomExpandedSettingsWordsHeading => "All sixteen exact native words",
            Self::RomExpandedSettingsWord => "Word {index}",
            Self::RomExpandedSettingsStageWords => "Stage all words",
            Self::RomExpandedSettingsCommit => "Commit to ROM",
            Self::RomExpandedSettingsStaged => "Staged",
            Self::RomExpandedSettingsUnchanged => "Unchanged",
            Self::RomExpandedSettingsDiscardTitle => "Discard staged ROM settings?",
            Self::RomExpandedSettingsUnsavedNotice => {
                "These staged settings have not been committed to the ROM."
            }
            Self::RomExpandedSettingsCancel => "Cancel",
            Self::RomExpandedSettingsDiscard => "Discard",
            Self::RomExpandedSettingsErrorTitle => "ROM expanded-settings error",
            Self::RomExpandedSettingsOk => "OK",
            Self::RomExpandedSettingsGfxSlotFormat => "{slot}",
            Self::ExpandedSettingsDocumentTitle => "Expanded Settings Editor",
            Self::ExpandedSettingsRecoveredNotice => "Recovered Layer 3 tilemap settings",
            Self::ExpandedSettingsApplyLayer3 => "Apply Layer 3 settings",
            Self::ExpandedSettingsApplyExpandedMode => "Apply Layer 3 expanded mode",
            Self::ExpandedSettingsApplyBypass => "Apply Super GFX bypass",
            Self::ExpandedSettingsApplyBoundary => "Apply sprite boundary interaction",
            Self::ExpandedSettingsWordsNotice => {
                "All values below are exact native 16-bit words; unknown meanings are preserved."
            }
            Self::ExpandedSettingsApplyWords => "Apply all sixteen words atomically",
            Self::ExpandedSettingsUndo => "Undo",
            Self::ExpandedSettingsRedo => "Redo",
            Self::ExpandedSettingsSave => "Save",
            Self::ExpandedSettingsModified => "Modified",
            Self::ExpandedSettingsSaved => "Saved",
            Self::ExpandedSettingsUnsavedTitle => "Unsaved expanded settings",
            Self::ExpandedSettingsDiscardQuestion => "Discard unsaved expanded-settings changes?",
            Self::ExpandedSettingsErrorTitle => "Expanded-settings editor error",
            Self::LevelRestrictionEditingWarning => {
                "After restriction, performing additional editing operations on the locked ROM is not recommended."
            }
            Self::LevelRestrictionAcknowledge => {
                "I understand that the original tool cannot reverse this operation."
            }
            Self::LevelRestrictionRestoreTitle => "Create Full Restore Point",
            Self::LevelRestrictionRestoreNotice => {
                "A full restore point is required by the enabled destructive-operation policy before IPS creation can continue."
            }
            Self::LevelRestrictionRetryRestore => "Retry Restore Point",
            Self::LevelRestrictionIpsTitle => "Create an IPS patch?",
            Self::LevelRestrictionIpsQuestion => {
                "Do you want to create an IPS for this locked ROM?"
            }
            Self::LevelRestrictionYes => "Yes",
            Self::LevelRestrictionNo => "No",
            Self::LevelRestrictionSavingTitle => "Saving restricted ROM",
            Self::LevelRestrictionSavingForIps => "Saving the restricted ROM before IPS creation…",
            Self::LevelRestrictionSaveRequired => {
                "The restricted ROM must be saved before an IPS can be created."
            }
            Self::LevelRestrictionRetrySave => "Retry Save",
            Self::LevelRestrictionCompleteTitle => "Level Access Restriction Complete",
            Self::LevelRestrictionCompleteNotice => {
                "Your modified levels are no longer accessible by Lunar Magic. Performing any additional operations on this ROM is not recommended."
            }
            Self::LevelRestrictionOk => "OK",
            Self::LevelRestrictionSavingForClose => "Saving the restricted ROM before closing it…",
            Self::LevelRestrictionStillOpen => {
                "The restricted ROM is still open and has not been saved."
            }
            Self::LevelRestrictionRetrySaveClose => "Retry Save and Close",
            Self::LevelRestrictionErrorTitle => "Level access restriction error",
            Self::OverworldAnimationThisMap => "This map",
            Self::OverworldAnimationGlobal => "Global",
            Self::OverworldAnimationGlobalReadOnly => {
                "Global destination owner selected. This record is shown read-only here; use the ROM ExAnimation editor to modify the global domain."
            }
            Self::OverworldAnimationSetting => "Setting (hex)",
            Self::OverworldAnimationHeader => "Header (hex)",
            Self::OverworldAnimationApplyGlobals => "Apply animation globals",
            Self::OverworldAnimationTrigger => "Trigger",
            Self::OverworldAnimationEnabled => "Enabled",
            Self::OverworldAnimationValue => "Value",
            Self::OverworldAnimationApplyTrigger => "Apply trigger",
            Self::OverworldAnimationRecord => "Record",
            Self::OverworldAnimationKind => "Kind (hex)",
            Self::OverworldAnimationRecordTrigger => "Trigger (hex)",
            Self::OverworldAnimationDestination => "Destination (hex)",
            Self::OverworldAnimationDestinationFlag => "Destination flag",
            Self::OverworldAnimationSourceWords => "Source words, one frame per line:",
            Self::OverworldAnimationSpecialNotice => {
                "This special transfer kind has no ordinary frame payload."
            }
            Self::OverworldAnimationAppend => "Append",
            Self::OverworldAnimationReplace => "Replace",
            Self::OverworldAnimationRemove => "Remove",
            Self::OverworldAnimationCopyRecord => "Copy record",
            Self::OverworldAnimationPasteRecord => "Paste record",
            Self::OverworldAnimationFramePrefix => "Frame ",
            Self::OverworldAnimationCopyFrame => "Copy frame",
            Self::OverworldAnimationPasteFrame => "Paste frame",
            Self::OverworldAnimationOptionsHeading => "Per-map animation options",
            Self::OverworldAnimationMapSelector => {
                "Map (main, Yoshi, Vanilla, Forest, Valley, Special, Star)"
            }
            Self::OverworldAnimationOriginalPalette => "Original palette animation",
            Self::OverworldAnimationOriginalTiles => "Original animated tiles",
            Self::OverworldAnimationGlobalFeature => "Global ExAnimation",
            Self::OverworldAnimationMapFeature => "This map's ExAnimation",
            Self::OverworldAnimationOriginalLightning => "Original lightning",
            Self::OverworldAnimationOptionsUnsupported => {
                "Per-map option operands are not authenticated for this ROM profile."
            }
            Self::OverworldAnimationRuntimeRequired => {
                "The four feature switches require Lunar Magic's overworld animation runtime; original lightning is independently editable."
            }
            Self::OverworldAnimationInstallRuntime => "Install overworld animation runtime",
            Self::OverworldAnimationInstallRuntimeNotice => {
                "Install Lunar Magic's authenticated vanilla SMW-US runtime and seven-byte per-map option table as one undoable ROM transaction."
            }
            Self::OverworldAnimationInstallBlocked => {
                "Commit or discard staged changes before installing the runtime."
            }
            Self::OverworldAnimationPreviewHeading => "Live overworld ExAnimation preview",
            Self::OverworldAnimationPlay => "Play",
            Self::OverworldAnimationPause => "Pause",
            Self::OverworldAnimationReset => "Reset",
            Self::OverworldAnimationStepTimer => "Step timer",
            Self::OverworldAnimationPhaseTick => "phase {phase}, tick {tick}",
            Self::OverworldAnimationTimerNotice => {
                "The selected native timer advances {count} animation {unit} per callback."
            }
            Self::OverworldAnimationCustom => "Custom",
            Self::OverworldAnimationOneShot => "One Shot",
            Self::OverworldAnimationManualFrame => "Manual Frame",
            Self::OverworldAnimationActive => "Active",
            Self::OverworldAnimationEventPrefix => "Event $",
            Self::OverworldAnimationPassed => "Passed",
            Self::OverworldAnimationEventManualNotice => {
                "Event Manual 8-F uses the event numbers stored by Trigger Init and these passed-event states."
            }
            Self::OverworldAnimationNoRecordsNotice => {
                "No custom overworld ExAnimation records are installed for this submap."
            }
            Self::TitleRecordingTitle => "ROM Title-Screen Recording",
            Self::TitleRecordingDescription => {
                "Exact Lunar Magic movement payload. Enter two hexadecimal digits per byte; whitespace separates bytes and the final byte must be FF."
            }
            Self::TitleRecordingNoPlayback => "No playback patch is installed in this ROM.",
            Self::TitleRecordingStaleNotice => {
                "The ROM changed after this recording was opened. Reopen before committing."
            }
            Self::TitleRecordingBytesPresent => "{count} bytes, terminator present",
            Self::TitleRecordingEnterPayload => "Enter a recording payload to install playback.",
            Self::TitleRecordingMinimalPayload => "Minimal payload",
            Self::TitleRecordingNormalizeHex => "Normalize hex",
            Self::TitleRecordingCommit => "Commit recording to ROM",
            Self::TitleRecordingModified => "Modified",
            Self::TitleRecordingUnchanged => "Unchanged",
            Self::TitleRecordingRecorderHeading => {
                "Temporary joypad recorder for creating title movements"
            }
            Self::TitleRecordingRecorderAbsentNotice => {
                "The recorder temporarily repurposes overworld RAM. Install it only while recording a level, then uninstall it before loading or creating overworld save states."
            }
            Self::TitleRecordingInstallRecorder => "Install temporary joypad recorder",
            Self::TitleRecordingRecorderInstalledNotice => {
                "Recorder installed: create the emulator save state now, then uninstall immediately."
            }
            Self::TitleRecordingUninstallRecorder => "Uninstall temporary joypad recorder",
            Self::TitleRecordingFilesHeading => "Recording files and emulator states",
            Self::TitleRecordingImportNative => "Import .lmtitle…",
            Self::TitleRecordingImportZsnes => "Import ZSNES state…",
            Self::TitleRecordingImportSnes9x => "Import Snes9x state…",
            Self::TitleRecordingExportNative => "Export .lmtitle…",
            Self::TitleRecordingExportZsnes => "Export ZSNES state…",
            Self::TitleRecordingTransferNotice => {
                "Imports stage the exact movement payload for review; Commit recording to ROM applies it. Exports never modify the ROM."
            }
            Self::TitleRecordingDiscardTitle => "Discard title-recording changes?",
            Self::TitleRecordingUnsavedNotice => {
                "The edited recording has not been committed to the ROM."
            }
            Self::TitleRecordingCancel => "Cancel",
            Self::TitleRecordingDiscard => "Discard",
            Self::TitleRecordingErrorTitle => "Title-recording editor error",
            Self::TitleRecordingOk => "OK",
            Self::OverworldMessageTitle => "ROM Overworld Messages",
            Self::OverworldMessageDescription => {
                "Complete variable 8×18 message table. All numeric fields are hexadecimal."
            }
            Self::OverworldMessageStorageStatus => {
                "Loaded storage: {storage}; staged messages: {count}"
            }
            Self::OverworldMessageStaleNotice => {
                "The ROM changed after these messages were opened. Reopen before committing."
            }
            Self::OverworldMessageTableCount => "Table count (0C2–200, even)",
            Self::OverworldMessageResize => "Resize table",
            Self::OverworldMessageIndex => "Message",
            Self::OverworldMessageColumn => "Column (00–11)",
            Self::OverworldMessageTileValue => "Tile value (FE is reserved)",
            Self::OverworldMessageDiscardTitle => "Discard overworld-message changes?",
            Self::OverworldMessageUnsavedNotice => {
                "The staged message table has not been committed to the ROM."
            }
            Self::OverworldMessageErrorTitle => "Overworld-message editor error",
            Self::BossMessageTitle => "ROM Boss-Sequence Messages",
            Self::BossMessageDescription => {
                "Seven lossless 24×8 tile-index messages. All fields are hexadecimal."
            }
            Self::BossMessageStaleNotice => {
                "The ROM changed after these messages were opened. Reopen before committing."
            }
            Self::BossMessageIndex => "Message (00–06)",
            Self::BossMessageColumn => "Column (00–17)",
            Self::BossMessageTileValue => "Tile value",
            Self::BossMessageDiscardTitle => "Discard boss-message changes?",
            Self::BossMessageUnsavedNotice => {
                "The staged boss-sequence messages have not been committed."
            }
            Self::BossMessageErrorTitle => "Boss-sequence editor error",
            Self::MessageEditorRow => "Row (00–07)",
            Self::MessageEditorLoadTile => "Load tile",
            Self::MessageEditorApplyTile => "Apply tile",
            Self::MessageEditorCommit => "Commit messages to ROM",
            Self::MessageEditorStaged => "Staged",
            Self::MessageEditorUnchanged => "Unchanged",
            Self::MessageEditorCancel => "Cancel",
            Self::MessageEditorDiscard => "Discard",
            Self::MessageEditorOk => "OK",
            Self::RomMetadataTitle => "Lunar Magic ROM Metadata",
            Self::RomMetadataDescription => {
                "Lossless fixed LM metadata. Unknown bytes remain deliberately opaque."
            }
            Self::RomMetadataSummary => {
                "features={features}  compression={compression}  mapping={mapping}  checksum-status={checksum}"
            }
            Self::RomMetadataStaleNotice => {
                "The ROM changed after this metadata was opened. Reopen before committing."
            }
            Self::RomMetadataRegion => "Region",
            Self::RomMetadataAttribution => "Attribution",
            Self::RomMetadataAttributionRange => "Attribution (00–9F)",
            Self::RomMetadataVramVersion => "VRAM version",
            Self::RomMetadataVramVersionRange => "VRAM version (00)",
            Self::RomMetadataFeatureRecord => "Feature record",
            Self::RomMetadataFeatureRecordRange => "Feature record (00–18)",
            Self::RomMetadataByteIndex => "Byte index",
            Self::RomMetadataByteValue => "Byte value",
            Self::RomMetadataLoadByte => "Load byte",
            Self::RomMetadataApplyByte => "Apply byte",
            Self::RomMetadataCommit => "Commit metadata to ROM",
            Self::RomMetadataStaged => "Staged",
            Self::RomMetadataUnchanged => "Unchanged",
            Self::RomMetadataDiscardTitle => "Discard Lunar Magic metadata changes?",
            Self::RomMetadataUnsavedNotice => "The staged fixed metadata has not been committed.",
            Self::RomMetadataCancel => "Cancel",
            Self::RomMetadataDiscard => "Discard",
            Self::RomMetadataErrorTitle => "Lunar Magic metadata editor error",
            Self::RomMetadataOk => "OK",
            Self::LegacyBypassFgBgTitle => "Standard FG/BG GFX Bypass",
            Self::LegacyBypassSpriteTitle => "Standard Sprite GFX Bypass",
            Self::LegacyBypassDescription => {
                "Recovered Lunar Magic standard-GFX list: 255 selectable rows."
            }
            Self::LegacyBypassEnable => "Enable bypass for this level",
            Self::LegacyBypassListRow => "GFX bypass list row",
            Self::LegacyBypassRegularRow => "List row",
            Self::LegacyBypassRegularNotice => {
                "Alternate regular edit-field dialog enabled by the historical Options preference."
            }
            Self::LegacyBypassZeroFallback => {
                "A zero-filled selected row falls back to the level's normal tileset assignment."
            }
            Self::LegacyBypassStaleNotice => {
                "The ROM changed after this editor opened. Close and reopen before committing."
            }
            Self::LegacyBypassStage => "Stage row and level selection",
            Self::LegacyBypassCommit => "Commit to ROM",
            Self::LegacyBypassStaged => "Staged",
            Self::LegacyBypassUnchanged => "Unchanged",
            Self::LegacyBypassDiscardTitle => "Discard staged GFX bypass changes?",
            Self::LegacyBypassUnsavedNotice => {
                "These list or level-selection changes have not been committed."
            }
            Self::LegacyBypassCancel => "Cancel",
            Self::LegacyBypassDiscard => "Discard",
            Self::LegacyBypassErrorTitle => "Standard GFX bypass error",
            Self::LegacyBypassOk => "OK",
            Self::CopierHeaderTitle => "Convert Copier Header",
            Self::CopierHeaderLogicalRomFormat => "Logical ROM: {length} bytes (unchanged)",
            Self::CopierHeaderCurrentStateFormat => "Current physical state: {state}",
            Self::CopierHeaderTarget => "Target",
            Self::CopierHeaderAbsent => "Headerless",
            Self::CopierHeaderPresent => "512-byte copier header",
            Self::CopierHeaderFillByte => "New-header fill byte",
            Self::CopierHeaderPreservationNotice => {
                "Only the physical file prefix changes; mapper addresses and logical ROM contents remain identical."
            }
            Self::CopierHeaderUseCanonical => "Use Lunar Magic synthesized header",
            Self::CopierHeaderCancel => "Cancel",
            Self::CopierHeaderConvert => "Convert transactionally",
            Self::CopierHeaderErrorTitle => "Copier-header conversion error",
            Self::CopierHeaderOk => "OK",
            Self::IpsApplyTitle => "Apply IPS Patch",
            Self::IpsApplyHeaderNotice => {
                "The patch applies to logical ROM offsets; the copier header remains unchanged."
            }
            Self::IpsApplySummaryFormat => {
                "logical bytes: {source} → {target}    changed/added/removed: {changed}"
            }
            Self::IpsApplyIdentityNotice => {
                "The resulting image must retain the open game's stable identity and occupy complete mapper-addressable banks. A successful patch is one undoable project operation."
            }
            Self::IpsApplyStaleNotice => "The ROM changed after this patch was loaded.",
            Self::IpsApplyCancel => "Cancel",
            Self::IpsApplyAction => "Apply transactionally",
            Self::IpsApplyErrorTitle => "IPS patch error",
            Self::IpsApplyOk => "OK",
            Self::IpsCreateOriginalPrompt => "Select Original ROM",
            Self::IpsCreateModifiedPrompt => "Select Modified ROM",
            Self::IpsCreateTitle => "Create IPS Patch",
            Self::IpsCreateOriginalFormat => "Original: {path}",
            Self::IpsCreateModifiedFormat => "Modified: {path}",
            Self::IpsCreateOutputFormat => "Output: {path}",
            Self::IpsCreateProgress => "Comparing logical ROM bytes and creating the IPS patch…",
            Self::IpsCreateCompletedTitle => "IPS patch created",
            Self::IpsCreateCompletedFormat => "Created {path} ({bytes} bytes).",
            Self::IpsCreateErrorTitle => "IPS creation error",
            Self::IpsCreateOk => "OK",
            Self::RatsReclaimTitle => "Reclaim Owned RATS Blocks",
            Self::RatsReclaimOwnershipNotice => {
                "Only blocks explicitly owned and not retained by the manifest will be erased."
            }
            Self::RatsReclaimSummaryFormat => {
                "reclaim={blocks} blocks / {bytes} bytes    retain={retained} blocks"
            }
            Self::RatsReclaimFillByte => "Erase fill byte",
            Self::RatsReclaimTransactionNotice => {
                "The manifest is revalidated against the current ROM. Erasure and checksum repair commit as one undoable project operation."
            }
            Self::RatsReclaimStaleNotice => "The ROM changed after this manifest was loaded.",
            Self::RatsReclaimCancel => "Cancel",
            Self::RatsReclaimAction => "Reclaim transactionally",
            Self::RatsReclaimErrorTitle => "RATS reclamation error",
            Self::RatsReclaimOk => "OK",
            Self::RevisionPatchTitle => "Install Revision Patch",
            Self::RevisionPatchIdentityFormat => {
                "Identity: {game} / {region} / revision {revision} / {mapper}"
            }
            Self::RevisionPatchPayloadSummaryFormat => {
                "Payloads: {payloads}    Guarded writes: {writes}"
            }
            Self::RevisionPatchRangeNotice => {
                "End-exclusive logical-PC allocation range (hexadecimal)."
            }
            Self::RevisionPatchSearchStart => "Search start",
            Self::RevisionPatchSearchEnd => "Search end",
            Self::RevisionPatchExpansionFill => "Expansion fill",
            Self::RevisionPatchAtomicNotice => {
                "The audited profile supplies protected metadata ranges. Allocation, guarded writes, fixups, expansion, checksum repair, and undo history commit atomically."
            }
            Self::RevisionPatchStaleNotice => {
                "The ROM or profile changed after this template was loaded."
            }
            Self::RevisionPatchCancel => "Cancel",
            Self::RevisionPatchInstall => "Install transactionally",
            Self::RevisionPatchErrorTitle => "Revision patch installation error",
            Self::RevisionPatchOk => "OK",
            Self::BuiltInRuntimeTitle => "Install Built-in Runtime",
            Self::BuiltInRuntimeTarget => "Target: Super Mario World (USA), revision 0, LoROM",
            Self::BuiltInRuntimeFamily => "Recovered runtime family",
            Self::BuiltInRuntimeExpandedSettings => "Expanded level settings",
            Self::BuiltInRuntimeCompleteLayer3 => {
                "Complete Layer 3 family (includes expanded settings)"
            }
            Self::BuiltInRuntimeLfix3 => "Lfix3 core runtime and shared tables",
            Self::BuiltInRuntimeMap16 => "Complete Map16 runtime and auxiliary table",
            Self::BuiltInRuntimeExAnimation => "Expanded ExAnimation runtime",
            Self::BuiltInRuntimeLayer2 => "Layer 2 object-data runtime format $103",
            Self::BuiltInRuntimeSprite19 => "Sprite 19 ASM fix",
            Self::BuiltInRuntimeSupportPatchB => "Level support patch B (custom time / scroll)",
            Self::BuiltInRuntimeLz2Speed => "LZ2 Speed graphics decompressor",
            Self::BuiltInRuntimeSharedPalettes => "Expanded shared/custom palettes",
            Self::BuiltInRuntimeExpandedSettingsDescription => {
                "Install the recovered 512-record settings table and its exact runtime hooks."
            }
            Self::BuiltInRuntimeCompleteLayer3Description => {
                "Install all recovered Layer 3 runtime allocations, hooks, compatibility code, and expanded settings as one transaction."
            }
            Self::BuiltInRuntimeLfix3Description => {
                "Install the recovered Lfix3 runtime, three initialized 512-entry tables, and all fixed entry hooks."
            }
            Self::BuiltInRuntimeMap16Description => {
                "Install the recovered fixed Map16 hooks and the relocated 32-KiB auxiliary table."
            }
            Self::BuiltInRuntimeExAnimationDescription => {
                "Install the recovered expanded ExAnimation core, pointer table, graphics helpers, shared-palette helpers, and fixed hooks as one transaction."
            }
            Self::BuiltInRuntimeLayer2Description => {
                "Migrate an authenticated legacy Layer 2 pointer/descriptor table and runtime hook to format $103."
            }
            Self::BuiltInRuntimeSprite19Description => {
                "Install the recovered shared helper and branch patch that make sprite $19 safe on any level."
            }
            Self::BuiltInRuntimeSupportPatchBDescription => {
                "Install the recovered fixed runtime used by custom level time and separate scroll settings."
            }
            Self::BuiltInRuntimeLz2SpeedDescription => {
                "Install Lunar Magic's fast LZ2 decompressor. LZ2 Orig and LZ2 Speed share the same payload format, so graphics data is not recompressed."
            }
            Self::BuiltInRuntimeSharedPalettesDescription => {
                "Install the recovered shared-palette hooks, helpers, expanded table, and the 512-entry per-level custom-palette pointer table."
            }
            Self::BuiltInRuntimeAlreadyInstalled => {
                "The selected current runtime is already installed and authenticated."
            }
            Self::BuiltInRuntimeAtomicNotice => {
                "Installation may expand the ROM. All allocations, hooks, checksum repair, and history changes commit atomically."
            }
            Self::BuiltInRuntimeStaleNotice => {
                "The ROM changed after this installer opened. Reopen before installing."
            }
            Self::BuiltInRuntimeCancel => "Cancel",
            Self::BuiltInRuntimeMigrate => "Migrate transactionally",
            Self::BuiltInRuntimeInstall => "Install transactionally",
            Self::BuiltInRuntimeErrorTitle => "Built-in runtime installation error",
            Self::BuiltInRuntimeOk => "OK",
            Self::BuiltInRuntimeMigrateLfix3Gen1 => {
                "The authenticated legacy Lfix3 generation 1 will be migrated to generation 3 while converting its live packed table into the current three-plane form."
            }
            Self::BuiltInRuntimeMigrateLfix3Gen2 => {
                "The authenticated legacy Lfix3 generation 2 will be migrated to generation 3 while preserving all three live per-level tables."
            }
            Self::BuiltInRuntimeMigrateMap16Stage1 => {
                "The authenticated legacy Map16 stage $0100 runtime will be migrated to stage $0112 while leaving existing Map16 data and allocations untouched."
            }
            Self::BuiltInRuntimeMigrateMap16Stage2 => {
                "The authenticated legacy Map16 stage $0101 runtime will be migrated to stage $0112 while leaving existing Map16 data and allocations untouched."
            }
            Self::BuiltInRuntimeMigrateMap16Stage3 => {
                "The authenticated legacy Map16 stage $0111 runtime will be migrated to stage $0112 while leaving existing Map16 data and allocations untouched."
            }
            Self::BuiltInRuntimeMigrateExAnimationPointers => {
                "The authenticated legacy ExAnimation pointer fragments will be migrated to the current bank and marker contract while preserving the existing runtime allocation."
            }
            Self::BuiltInRuntimeMigrateExAnimationTable => {
                "The authenticated legacy 512-entry ExAnimation table will be converted into current compact per-level allocations together with the complete current runtime as one undoable transaction."
            }
            Self::BuiltInRuntimeMigrateLayer2Format100 => {
                "The authenticated legacy Layer 2 format $100 pointer table and descriptors will be converted to format $103 together with the exact current runtime hook."
            }
            Self::BuiltInRuntimeMigrateLayer2Format101 => {
                "The authenticated legacy Layer 2 format $101 pointer table and descriptors will be converted to format $103 together with the exact current runtime hook."
            }
            Self::BuiltInRuntimeMigrateLayer2Format102 => {
                "The authenticated legacy Layer 2 format $102 pointer table and descriptors will be converted to format $103 together with the exact current runtime hook."
            }
            Self::RomLoaderMissingHeaderTitle => "Missing Copier Header",
            Self::RomLoaderMissingHeaderQuestion => {
                "This ROM has no 0x200-byte copier header. Add the header now?"
            }
            Self::RomLoaderAddHeader => "Add Header",
            Self::RomLoaderCancel => "Cancel",
            Self::RomLoaderOpeningTitle => "Opening ROM",
            Self::RomLoaderOpeningProgress => "Reading and validating the selected ROM…",
            Self::MwlImportTitle => "Insert Level From File",
            Self::MwlImportReadingFormat => "Reading {path}",
            Self::MwlImportReadingSidecarsFormat => "Reading legacy sidecars for {path}",
            Self::MwlImportMissingPalette => {
                "Couldn't locate the palette file! Switching to non-custom shared palette."
            }
            Self::MwlImportCommittingFormat => "Committing level {level} from {path}",
            Self::MwlImportCommittingNotesFormat => {
                "Committing level {level} from {path} ({notes})"
            }
            Self::MwlImportClose => "Close",
            Self::MwlImportInsertedFormat => "Inserted level {level} from {path}",
            Self::MwlImportFailedFormat => "Failed to commit level {level} from {path}",
            Self::MwlBatchImportTitle => "Insert Multiple MWL Levels",
            Self::MwlBatchImportDirectoryFormat => "Directory: {path}",
            Self::MwlBatchImportSummaryFormat => {
                "Inserted: {inserted}   Failed: {failed}   Hidden skipped: {hidden}   Remaining: {remaining}"
            }
            Self::MwlBatchImportAllocationSearch => "Allocation search (logical PC hex)",
            Self::MwlBatchImportRangeSeparator => "..",
            Self::MwlBatchImportStart => "Start import",
            Self::MwlBatchImportCancelNotice => {
                "Press Escape or choose Cancel to stop after the current level."
            }
            Self::MwlBatchImportCancel => "Cancel",
            Self::MwlBatchImportClose => "Close",
            Self::MwlBatchImportCancelled => "Batch import cancelled.",
            Self::MwlBatchImportCompleteFormat => {
                "{inserted} levels inserted; {failed} failed; {hidden} hidden files skipped."
            }
            Self::MwlBatchImportReadingFormat => "Reading {path}",
            Self::MwlBatchImportCommittingFormat => "Committing level {level} from {path}",
            Self::MwlBatchImportPreparedFormat => "Prepared level {level} from {path}",
            Self::MwlBatchImportInsertedFormat => "Inserted level {level} from {path}",
            Self::MwlBatchImportReadFailedFormat => "Failed to start reading {path}: {error}",
            Self::MwlBatchImportInsertFailedFormat => "Failed to insert {path}: {error}",
            Self::MwlBatchImportCommitFailedFormat => "Failed to commit level {level} from {path}",
            Self::MwlBatchImportDiscardedRead => "Discarded the completed read after cancellation.",
            Self::MwlBatchExportProgressTitle => "Exporting Multiple MWL Levels",
            Self::MwlBatchExportTemplateFormat => "Template: {path}",
            Self::MwlBatchExportAtomicNotice => {
                "Levels are prepared in the background and published as one group."
            }
            Self::MwlBatchExportCancellationRequested => "Cancellation requested…",
            Self::MwlBatchExportCancel => "Cancel",
            Self::MwlBatchExportResultTitle => "MWL Batch Export",
            Self::MwlBatchExportCompletedFormat => "Exported {count} levels.",
            Self::MwlBatchExportCancelled => "Batch MWL export cancelled.",
            Self::MwlBatchExportClose => "Close",
            Self::VramPatchTitle => "Change VRAM Patch Options",
            Self::VramPatchDescription => {
                "The VRAM patch by smkdan allows using an extra 2 GFX slots for more graphics (BG2 and BG3). It's also required for horizontal levels to be resized vertically."
            }
            Self::VramPatchDeferredNotice => "Any changes will be applied on the next level save.",
            Self::VramPatchType => "VRAM Patch Type",
            Self::VramPatchNone => "None - Do not install patch",
            Self::VramPatchNoneHelp => {
                "This will not install the VRAM patch. It can make some features unavailable. This option is only available if the patch has not yet been installed."
            }
            Self::VramPatchNormal => "Normal Version",
            Self::VramPatchNormalHelp => {
                "Installs the regular version of the VRAM patch. This is the default setting."
            }
            Self::VramPatchHd16x9 => "HD Version 16:9 (352 width)",
            Self::VramPatchHd21x9 => "HD Version 21:9 (448 width)",
            Self::VramPatchUnknownNotice => {
                "The installed VRAM patch version is not recognized. Lunar Magic disables every choice to avoid overwriting an unknown patch."
            }
            Self::VramPatchCancel => "Cancel",
            Self::VramPatchOk => "OK",
            Self::VramPatchErrorTitle => "VRAM patch options error",
            Self::VramPatchStatusNone => {
                "VRAM patch will remain uninstalled on the next level save."
            }
            Self::VramPatchStatusNormal => {
                "Normal VRAM patch will be applied on the next level save."
            }
            Self::VramPatchStatusHd => "The installed HD VRAM patch selection is retained.",
            Self::LegacyBypassTransferCompleteTitle => "Bypass List Extraction Complete",
            Self::LegacyBypassTransferCompleteFormat => {
                "Old ExGFX bypass list extracted to:\n{path}"
            }
            Self::LegacyBypassTransferDestinationFallback => "selected destination",
            Self::LegacyBypassTransferErrorTitle => "Old ExGFX Bypass List Error",
            Self::LegacyBypassTransferOk => "OK",
            Self::VanillaLevelZoomTitle => "Zoom",
            Self::VanillaLevelZoomIn => "Zoom in",
            Self::VanillaLevelZoomOut => "Zoom out",
            Self::VanillaLevelZoomFilter => "Zoom Filter",
            Self::VanillaLevelConditionalMap16Title => "Conditional Direct Map16 Access",
            Self::VanillaLevelConditionalMap16RuntimeFlag => {
                "Runtime flag ($7FC060–$7FC06F bit index)"
            }
            Self::VanillaLevelConditionalMap16AlwaysShow => {
                "Always show objects (flag selects the +$100 tile bank)"
            }
            Self::VanillaLevelConditionalMap16RemoveFlag => "Remove flag check",
            Self::VanillaLevelApply => "Apply",
            Self::VanillaLevelCancel => "Cancel",
            Self::VanillaLevelDirectMap16RemapTitle => "Remap Direct Map16 Access",
            Self::VanillaLevelHexSourceDestinationPairs => "Hexadecimal source/destination pairs",
            Self::VanillaLevelDirectMap16RemapHelp => {
                "Use M for a moving destination, +/− for offsets, and R for rectangles."
            }
            Self::VanillaLevelBackgroundMap16BankTitle => "Change Background Map16 Bank",
            Self::VanillaLevelBackgroundMap16BankHelp => {
                "Select the 4-KiB Map16 bank used by this level's background."
            }
            Self::VanillaLevelBank => "Bank",
            Self::VanillaLevelOk => "OK",
            Self::VanillaLevelBackgroundTileRemapTitle => "Remap Background Tiles",
            Self::VanillaLevelBackgroundTileOffset => "Offset to add to every background tile",
            Self::VanillaLevelBackgroundTileRemapHelp => {
                "Sources always refer to the original tilemap. Ranges, relative +/− values, moving M destinations, and rectangular R ranges follow Lunar Magic syntax."
            }
            Self::VanillaLevelPropertiesTitle => "Object/Sprite Properties",
            Self::VanillaLevelManualEditTitle => "Edit Manual",
            Self::VanillaLevelLayer1ObjectFormat => "Layer 1 object {index}",
            Self::VanillaLevelLayer2ObjectFormat => "Layer 2 object {index}",
            Self::VanillaLevelSpriteRecordFormat => "Sprite record {index}",
            Self::VanillaLevelApplyProperties => "Apply properties",
            Self::VanillaLevelSelectEntityForProperties => {
                "Select one object or sprite to inspect its properties."
            }
            Self::VanillaLevelManualSingleSelection => {
                "Edit Manual requires exactly one selected object or sprite."
            }
            Self::VanillaLevelSpriteTokenFormat => "Sprite token {index}",
            Self::VanillaLevelApplyCompleteRecord => "Apply complete record",
            Self::VanillaLevelSelectEntityForManualEdit => {
                "Select one object or sprite to edit it manually."
            }
            Self::VanillaLevelAddStructures => "Add structures and platforms",
            Self::VanillaLevelHexFilter => "Hex filter",
            Self::VanillaLevelHexNameFilter => "Hex/name filter",
            Self::VanillaLevelClear => "Clear",
            Self::VanillaLevelChooseStandardObject => {
                "Choose a tileset-resolved object, then click its destination tile."
            }
            Self::VanillaLevelHandlerMapUnavailable => {
                "The active standard-object handler map is unavailable."
            }
            Self::VanillaLevelStandardDefinitionsUnavailable => {
                "The recovered standard-object definitions are unavailable."
            }
            Self::VanillaLevelSwitchPreviewsUnavailable => {
                "The switch-state object previews are unavailable."
            }
            Self::VanillaLevelStandardObject => "Standard object",
            Self::VanillaLevelAddCustomOscObject => "Add custom OSC object visually",
            Self::VanillaLevelCustomObject => "custom object",
            Self::VanillaLevelAddExtendedObjects => "Add blocks, coins, doors, and small objects",
            Self::VanillaLevelChooseExtendedObject => {
                "Choose a tileset-resolved extended object, then click its destination tile."
            }
            Self::VanillaLevelExtendedDefinitionsUnavailable => {
                "The recovered extended-object definitions are unavailable."
            }
            Self::VanillaLevelExtendedObject => "Extended object",
            Self::VanillaLevelInsertAfterSelection => "Insert after selection",
            Self::VanillaLevelApplyScreenJump => "Apply screen jump",
            Self::VanillaLevelApplyScreenExit => "Apply screen exit",
            Self::VanillaLevelApplyObjectFields => "Apply object fields",
            Self::VanillaLevelApplyRawRecord => "Apply raw record",
            Self::VanillaLevelRemoveObject => "Remove object",
            Self::VanillaLevelMoveUp => "Move up",
            Self::VanillaLevelMoveDown => "Move down",
            Self::VanillaLevelCopy => "Copy",
            Self::VanillaLevelPasteAfterSelection => "Paste after selection",
            Self::VanillaLevelPasteMap16Rectangle => "Paste Map16 rectangle for placement",
            Self::VanillaLevelExistingSpritesFormat => {
                "Edit existing enemies and sprites ({count})"
            }
            Self::VanillaLevelChooseExistingSprite => {
                "Choose a picture, then click the canvas to place a copy in this level."
            }
            Self::VanillaLevelChooseExistingSpritePlaceholder => "Choose an existing sprite…",
            Self::VanillaLevelPlacementActive => {
                "Placement active: click a destination tile on the canvas."
            }
            Self::VanillaLevelRawSpriteStream => "Raw stream records and control commands",
            Self::VanillaLevelSpritesStored => "Enemies and sprites stored in this level",
            Self::VanillaLevelAddStandardSprites => "Add new enemies and sprites",
            Self::VanillaLevelChooseStandardSprite => {
                "Choose a recovered standard-sprite preview, then click its destination tile."
            }
            Self::VanillaLevelStandardSprite => "Standard sprite",
            Self::VanillaLevelAddCustomSprites => "Add custom enemies and sprites",
            Self::VanillaLevelStageSpriteHeader => "Stage sprite header",
            Self::VanillaLevelReplaceRecord => "Replace record",
            Self::VanillaLevelApplySpriteFields => "Apply sprite fields",
            Self::VanillaLevelRemoveSprite => "Remove sprite",
            Self::VanillaLevelCopyRecord => "Copy record",
            Self::VanillaLevelPasteRecordAfterSelection => "Paste record after selection",
            Self::VanillaLevelPlaceOnCanvas => "Place on canvas",
            Self::VanillaLevelApplyFields => "Apply fields",
            Self::VanillaLevelHeaderCountsFormat => "{objects} objects, {sprites} sprite records",
            Self::VanillaLevelMode => "Level mode",
            Self::VanillaLevelBackgroundPalette => "Background palette",
            Self::VanillaLevelLastScreen => "Last screen",
            Self::VanillaLevelBackgroundColor => "Background color",
            Self::VanillaLevelSpriteTileset => "Sprite tileset",
            Self::VanillaLevelDefaultMusic => "Default music selector",
            Self::VanillaLevelCustomMusicBypass => "Custom music bypass",
            Self::VanillaLevelEnabled => "Enabled",
            Self::VanillaLevelCustomMusicTrack => "Custom music track (hex)",
            Self::VanillaLevelTimeLimit => "Time limit selector",
            Self::VanillaLevelCustomTimeBypass => "Custom time bypass",
            Self::VanillaLevelCustomTime => "Custom time (hex)",
            Self::VanillaLevelForceTimeReset => "Force time reset",
            Self::VanillaLevelForegroundPalette => "Foreground palette",
            Self::VanillaLevelSpritePalette => "Sprite palette",
            Self::VanillaLevelObjectTileset => "Object tileset",
            Self::VanillaLevelLayer1VerticalScroll => "Layer 1 vertical scroll",
            Self::VanillaLevelStageHeader => "Stage header changes",
            Self::VanillaLevelResetStagedValues => "Reset staged values",
            Self::VanillaLevelResetLayer2Title => "Reset Layer 2 for level mode change?",
            Self::VanillaLevelResetLayer2Format => {
                "Changing level mode ${source} to ${target} switches Layer 2 storage formats."
            }
            Self::VanillaLevelResetLayer2Help => {
                "Lunar Magic clears the tilemap workspace when entering a tilemap-backed mode. Object-backed data remains available if you switch back before saving."
            }
            Self::VanillaLevelResetLayer2Apply => "Reset Layer 2 and stage changes",
            Self::VanillaLevelMainEntrance => "Main entrance",
            Self::VanillaLevelEntranceExactRecord => {
                "Exact four-plane vanilla SMW entrance record."
            }
            Self::VanillaLevelPosition => "Position",
            Self::VanillaLevelLayer2ScrollPreset => "Layer 2 original scroll preset",
            Self::VanillaLevelVerticalSettings => "Vertical settings",
            Self::VanillaLevelScreenMethod => "Screen / method",
            Self::VanillaLevelModeScreen => "Level mode / screen",
            Self::VanillaLevelMidwayInstalled => "Installed separate midway entrance",
            Self::VanillaLevelFlags => "Flags",
            Self::VanillaLevelAdditionalFlags => "Additional flags",
            Self::VanillaLevelHighPosition => "High position",
            Self::VanillaLevelMidwayNotInstalled => {
                "Separate-midway runtime is not installed. Initial values:"
            }
            Self::VanillaLevelInstallMidway => "Install separate midway runtime",
            Self::VanillaLevelStageEntrance => "Stage entrance fields",
            Self::VanillaLevelResetEntrance => "Reset entrance",
            Self::VanillaLevelCommitEntrances => "Commit entrances to ROM",
            Self::VanillaLevelCurrentUnavailable => "The current level is unavailable.",
            Self::VanillaLevelExitTableHelp => {
                "Stage all 32 source screens together. Apply creates one level-editor Undo step; Reset discards this form only."
            }
            Self::VanillaLevelScreen => "Screen",
            Self::VanillaLevelPresent => "Present",
            Self::VanillaLevelDestinationFlags => "Destination / flags",
            Self::VanillaLevelApplyAllExits => "Apply all screen exits",
            Self::VanillaLevelResetExits => "Reset screen exits",
            Self::VanillaLevelInvalidExitScreens => {
                "The following screens have exit-enabled objects that lead to level 0 or 0x100:"
            }
            Self::VanillaLevelInvalidExitSaveHelp => {
                "If you do not set an exit destination or remove the exit-enabled objects on these screens, the player could become trapped in an endless bonus game."
            }
            Self::VanillaLevelDisableWarningFormat => {
                "To disable this warning, turn off “{option}” in Tools."
            }
            Self::VanillaLevelSaveAnywayQuestion => "Save the level anyway?",
            Self::VanillaLevelSaveAnyway => "Save Anyway",
            Self::VanillaLevelScanExitsTitle => "Scan for Undefined Exits",
            Self::VanillaLevelNoInvalidExits => "No undefined exit destinations were found.",
            Self::VanillaLevelInvalidExitFixHelp => {
                "Set an exit destination or remove the exit-enabled pipe or door objects on those screens; otherwise the player can become trapped in an endless bonus game."
            }
            Self::VanillaLevelLayer2 => "Layer 2",
            Self::VanillaLevelLayer2TilemapStatusFormat => {
                "Compressed 32×32 background tilemap · selected storage word {index}"
            }
            Self::VanillaLevelMap16Word => "Map16 word",
            Self::VanillaLevelStageSelectedTile => "Stage selected tile",
            Self::VanillaLevelLayer2PaintHelp => {
                "Choose “Paint Layer 2 tile” and click the canvas to write this word. Selection follows Lunar Magic's column-major two-plane storage."
            }
            Self::VanillaLevelSharedBackgroundReadOnly => {
                "This is a shared pristine SMW background. It remains read-only until the format-$103 Layer 2 runtime can be installed copy-on-write; editing the shared bank-$0C payload directly would change every level that uses it."
            }
            Self::VanillaLevelLayer2ObjectCountFormat => {
                "{count} native Layer 2 object records are decoded and rendered."
            }
            Self::VanillaLevelBackgroundCanvas => "32×32 background canvas",
            Self::VanillaLevelCanvasPlaceHelp => {
                "Click a canvas tile to place the values from the matching editor below."
            }
            Self::VanillaLevelCanvasSelectHelp => {
                "Select or drag an object/enemy; Insert places the active template at the pointer, right-click duplicates there, and Delete removes the selection."
            }
            Self::VanillaLevelDuplicateSelected => "Duplicate selected",
            Self::VanillaLevelDeleteSelected => "Delete selected",
            Self::VanillaLevelGamePixels => "Game pixels",
            Self::VanillaLevelViewport => "256×224 viewport",
            Self::VanillaLevelSelectionOverGame => "Selection over game",
            Self::VanillaLevelSelectionOverGameHelp => {
                "Draw selected object and sprite tiles over the live emulator frame"
            }
            Self::VanillaLevelCanvasTool => "Canvas tool:",
            Self::VanillaLevelSelectMove => "Select / move",
            Self::VanillaLevelPlaceObject => "Place object",
            Self::VanillaLevelPlaceSprite => "Place sprite",
            Self::VanillaLevelPaintLayer2Tile => "Paint Layer 2 tile",
            Self::VanillaLevelPlaceLayer2Object => "Place Layer 2 object",
            Self::VanillaLevelZoom => "Zoom:",
            Self::VanillaLevelReset => "Reset",
            Self::VanillaLevelCamera => "Camera:",
            Self::VanillaLevelScreenMinus => "Screen −",
            Self::VanillaLevelScreenPlus => "Screen +",
            Self::VanillaLevelEntrance => "Entrance",
            Self::VanillaLevelObjectPlacementWarningFormat => {
                "There is at least 1 object that is placing tiles beyond the {edge} screen in the level. This can corrupt SNES RAM during gameplay."
            }
            Self::VanillaLevelSpriteCountFormat => {
                "You currently have {count} sprites in the level (limit is usually around {limit})."
            }
            Self::VanillaLevelSpriteCountWarning => {
                "Exceeding the maximum limit may cause extra sprites to not appear, or the game could freeze or display random sprites when the player reaches the affected screen."
            }
            Self::VanillaLevelVerticalFireballWarning => {
                "Sprite 33 (vertical fireball) is used in this level, but sprite buoyancy is not enabled. This will usually cause the game to freeze."
            }
            Self::VanillaLevelSaveTitle => "Save level to ROM?",
            Self::VanillaLevelSaveBeforeContinuing => {
                "The current level has staged changes. Save before continuing?"
            }
            Self::VanillaLevelSave => "Save",
            Self::VanillaLevelDiscard => "Discard",
            Self::VanillaLevelSaveBeforeExitFormat => {
                "The current level has staged changes. Save before following this exit to level {destination}?"
            }
            Self::VanillaLevelObjectFormat => "Object {index}",
            Self::VanillaLevelNoSelectedObject => "No selected object.",
            Self::VanillaLevelNativeScreenExit => "Native screen-exit object",
            Self::VanillaLevelSourceScreen => "Source screen",
            Self::VanillaLevelScreenExitEncodingHelp => {
                "Lunar Magic always sets flag 0400. Resulting values below 1000 use the compact four-byte form; higher flag values use the five-byte extended form."
            }
            Self::VanillaLevelScreenJumpFormat => "Screen-jump control ({order})",
            Self::VanillaLevelLowByteFirst => "low byte first",
            Self::VanillaLevelHighByteFirst => "high byte first",
            Self::VanillaLevelFirstEncodedComponent => "First encoded component",
            Self::VanillaLevelSecondEncodedComponent => "Second encoded component",
            Self::VanillaLevelAdvanceScreen => "Advance screen",
            Self::VanillaLevelPreviewZoomOut => "Zoom out",
            Self::VanillaLevelPreviewZoomIn => "Zoom in",
            Self::VanillaLevelPreviewZoomDefault => "Default 100%",
            Self::VanillaLevelSpriteMemory => "Sprite memory",
            Self::VanillaLevelSpriteBuoyancy1 => "Sprite buoyancy 1",
            Self::VanillaLevelWaterLavaInteraction => "Water/lava interaction",
            Self::VanillaLevelSpriteBuoyancy2 => "Sprite buoyancy 2",
            Self::VanillaLevelWaterLavaDisableLayerInteraction => {
                "Water/lava; disable Layer 2/3 interaction"
            }
            Self::VanillaLevelRecordBytes => "Record bytes",
            Self::VanillaLevelSpriteNumber => "Sprite number",
            Self::VanillaLevelX => "X",
            Self::VanillaLevelYLowBits => "Y (low 5 bits)",
            Self::VanillaLevelExtraBits => "Extra bits",
        }
    }

    const fn storage_key(self) -> OriginalDialogTextKey {
        OriginalDialogTextKey {
            dialog_id: RUST_UI_DIALOG_ID,
            item_index: RUST_UI_ITEM_INDEX,
            control_id: self as u32,
        }
    }

    fn from_storage_key(key: OriginalDialogTextKey) -> Option<Self> {
        if key.dialog_id != RUST_UI_DIALOG_ID || key.item_index != RUST_UI_ITEM_INDEX {
            return None;
        }
        Self::ALL
            .get(usize::try_from(key.control_id).ok()?)
            .copied()
    }
}

/// Stable identity for one literal item in an original Win32 dialog template.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OriginalDialogTextKey {
    pub dialog_id: u16,
    /// Zero-based item position, or `u16::MAX` for the dialog title.
    pub item_index: u16,
    /// Win32 control ID, or `u32::MAX` for the dialog title.
    pub control_id: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalizationError {
    WrongMagic,
    Truncated,
    TrailingBytes,
    InvalidUtf8,
    InvalidLocale,
    TextTooLong {
        key: UiTextKey,
        bytes: usize,
    },
    InvalidText(UiTextKey),
    WrongEntryCount(usize),
    UnknownKey(u8),
    DuplicateKey(UiTextKey),
    MissingKey(UiTextKey),
    TooManyDialogTexts(usize),
    DuplicateDialogText(OriginalDialogTextKey),
    DialogTextTooLong {
        key: OriginalDialogTextKey,
        bytes: usize,
    },
    InvalidDialogText(OriginalDialogTextKey),
    InvalidDialogTextKey(OriginalDialogTextKey),
    WrongDialogTextMagic,
    Overflow,
}

impl std::fmt::Display for LocalizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "localization catalog error: {self:?}")
    }
}

impl std::error::Error for LocalizationError {}

impl LocalizationCatalog {
    /// Maximum canonical `LMLOC001` size accepted by native bounded loaders.
    pub const MAX_ENCODED_LEN: usize = MAX_ENCODED_BYTES;

    #[must_use]
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// Creates a complete catalog. Partial translations are rejected so frontends never need to
    /// guess whether an absent value is intentional.
    ///
    /// # Errors
    ///
    /// Returns [`LocalizationError`] for invalid locale/text data or an incomplete/duplicate key
    /// set.
    pub fn new(
        locale: impl Into<String>,
        entries: impl IntoIterator<Item = (UiTextKey, String)>,
    ) -> Result<Self, LocalizationError> {
        let mut catalog = Self {
            locale: locale.into(),
            entries: BTreeMap::new(),
            dialog_entries: BTreeMap::new(),
        };
        for (key, value) in entries {
            if catalog.entries.insert(key, value).is_some() {
                return Err(LocalizationError::DuplicateKey(key));
            }
        }
        catalog.validate()?;
        Ok(catalog)
    }

    /// Adds an optional, lossless original-dialog text inventory to a complete typed catalog.
    /// Repeated Win32 control IDs remain distinct through their template item positions.
    ///
    /// # Errors
    ///
    /// Returns [`LocalizationError`] for duplicate/noncanonical keys or invalid aggregate text.
    pub fn with_original_dialog_texts(
        mut self,
        entries: impl IntoIterator<Item = (OriginalDialogTextKey, String)>,
    ) -> Result<Self, LocalizationError> {
        for (key, value) in entries {
            if ExtendedUiTextKey::from_storage_key(key).is_some()
                || (key.dialog_id == RUST_UI_DIALOG_ID && key.item_index == RUST_UI_ITEM_INDEX)
            {
                return Err(LocalizationError::InvalidDialogTextKey(key));
            }
            if self.dialog_entries.insert(key, value).is_some() {
                return Err(LocalizationError::DuplicateDialogText(key));
            }
        }
        self.validate()?;
        Ok(self)
    }

    /// Adds typed Rust-native translations in the versioned `LMDLG001` extension.
    /// Missing values intentionally fall back to each key's built-in English text.
    pub fn with_extended_ui_texts(
        mut self,
        entries: impl IntoIterator<Item = (ExtendedUiTextKey, String)>,
    ) -> Result<Self, LocalizationError> {
        for (key, value) in entries {
            let storage = key.storage_key();
            if self.dialog_entries.insert(storage, value).is_some() {
                return Err(LocalizationError::DuplicateDialogText(storage));
            }
        }
        self.validate()?;
        Ok(self)
    }

    #[must_use]
    /// Returns the translated value for a typed key.
    ///
    /// # Panics
    ///
    /// Panics only if an internal invariant is violated. Public constructors and decoding require
    /// every key and the entry map cannot be mutated externally.
    pub fn text(&self, key: UiTextKey) -> &str {
        self.entries
            .get(&key)
            .expect("validated catalogs contain every key")
    }

    /// Returns a Rust-native extension translation or its stable English fallback.
    #[must_use]
    pub fn extended_text(&self, key: ExtendedUiTextKey) -> &str {
        self.dialog_entries
            .get(&key.storage_key())
            .map_or_else(|| key.english(), String::as_str)
    }

    /// Returns the localized title for one original dialog resource when present.
    #[must_use]
    pub fn original_dialog_title(&self, dialog_id: u16) -> Option<&str> {
        self.dialog_entries
            .get(&OriginalDialogTextKey {
                dialog_id,
                item_index: DIALOG_TITLE_ITEM_INDEX,
                control_id: DIALOG_TITLE_CONTROL_ID,
            })
            .map(String::as_str)
    }

    /// Returns the first literal caption with the requested original dialog/control ID.
    /// Exact item lookup remains available for templates that repeat a control ID.
    #[must_use]
    pub fn original_dialog_control_text(&self, dialog_id: u16, control_id: u32) -> Option<&str> {
        self.dialog_entries
            .iter()
            .find(|(key, _)| {
                key.dialog_id == dialog_id
                    && key.item_index != DIALOG_TITLE_ITEM_INDEX
                    && key.control_id == control_id
            })
            .map(|(_, text)| text.as_str())
    }

    /// Returns a literal caption by its exact zero-based template item position.
    #[must_use]
    pub fn original_dialog_item_text(&self, dialog_id: u16, item_index: u16) -> Option<&str> {
        self.dialog_entries
            .iter()
            .find(|(key, _)| key.dialog_id == dialog_id && key.item_index == item_index)
            .map(|(_, text)| text.as_str())
    }

    #[must_use]
    pub fn original_dialog_text_count(&self) -> usize {
        self.dialog_entries.len()
    }

    fn insert_original_dialog_template(
        &mut self,
        dialog_id: u16,
        template: &OriginalLanguageDialogTemplate,
    ) {
        if let Some(title) = template.title.as_deref() {
            self.insert_original_dialog_text(
                OriginalDialogTextKey {
                    dialog_id,
                    item_index: DIALOG_TITLE_ITEM_INDEX,
                    control_id: DIALOG_TITLE_CONTROL_ID,
                },
                title,
            );
        }
        for (item_index, control) in template.controls.iter().enumerate() {
            let Some(text) = control.text.as_deref() else {
                continue;
            };
            let Ok(item_index) = u16::try_from(item_index) else {
                break;
            };
            self.insert_original_dialog_text(
                OriginalDialogTextKey {
                    dialog_id,
                    item_index,
                    control_id: control.id,
                },
                text,
            );
        }
    }

    fn insert_original_dialog_text(&mut self, key: OriginalDialogTextKey, text: &str) {
        if self.dialog_entries.len() >= MAX_DIALOG_TEXT_ENTRIES {
            return;
        }
        let text = normalize_original_dialog_text(text);
        if !text.is_empty() && text.len() <= MAX_TEXT_BYTES && !text.contains('\0') {
            self.dialog_entries.insert(key, text);
        }
    }

    /// Validates locale syntax, resource limits, and completeness.
    ///
    /// # Errors
    ///
    /// Returns [`LocalizationError`] for invalid or incomplete catalog data.
    pub fn validate(&self) -> Result<(), LocalizationError> {
        if self.locale.is_empty()
            || self.locale.len() > MAX_LOCALE_BYTES
            || self
                .locale
                .bytes()
                .any(|byte| byte == 0 || byte.is_ascii_control())
        {
            return Err(LocalizationError::InvalidLocale);
        }
        for key in UiTextKey::ALL {
            let value = self
                .entries
                .get(&key)
                .ok_or(LocalizationError::MissingKey(key))?;
            if value.len() > MAX_TEXT_BYTES {
                return Err(LocalizationError::TextTooLong {
                    key,
                    bytes: value.len(),
                });
            }
            if value.is_empty() || value.contains('\0') {
                return Err(LocalizationError::InvalidText(key));
            }
        }
        if self.entries.len() != UiTextKey::ALL.len() {
            return Err(LocalizationError::WrongEntryCount(self.entries.len()));
        }
        if self.dialog_entries.len() > MAX_DIALOG_TEXT_ENTRIES {
            return Err(LocalizationError::TooManyDialogTexts(
                self.dialog_entries.len(),
            ));
        }
        for (key, value) in &self.dialog_entries {
            if key.dialog_id == RUST_UI_DIALOG_ID && key.item_index == RUST_UI_ITEM_INDEX {
                if ExtendedUiTextKey::from_storage_key(*key).is_none() {
                    return Err(LocalizationError::InvalidDialogTextKey(*key));
                }
            }
            if key.item_index == DIALOG_TITLE_ITEM_INDEX
                && key.control_id != DIALOG_TITLE_CONTROL_ID
            {
                return Err(LocalizationError::InvalidDialogTextKey(*key));
            }
            if value.len() > MAX_TEXT_BYTES {
                return Err(LocalizationError::DialogTextTooLong {
                    key: *key,
                    bytes: value.len(),
                });
            }
            if value.is_empty() || value.contains('\0') {
                return Err(LocalizationError::InvalidDialogText(*key));
            }
        }
        Ok(())
    }

    /// Encodes the catalog canonically as `LMLOC001`.
    ///
    /// # Errors
    ///
    /// Returns [`LocalizationError`] if the in-memory catalog is invalid or cannot be represented.
    pub fn encode(&self) -> Result<Vec<u8>, LocalizationError> {
        self.validate()?;
        let mut output = MAGIC.to_vec();
        write_string(&mut output, &self.locale, MAX_LOCALE_BYTES)?;
        let count = u16::try_from(UiTextKey::ALL.len()).map_err(|_| LocalizationError::Overflow)?;
        output.extend_from_slice(&count.to_le_bytes());
        for key in UiTextKey::ALL {
            output.push(key as u8);
            write_string(&mut output, self.text(key), MAX_TEXT_BYTES)?;
        }
        if !self.dialog_entries.is_empty() {
            output.extend_from_slice(DIALOG_TEXT_MAGIC);
            let count = u16::try_from(self.dialog_entries.len())
                .map_err(|_| LocalizationError::Overflow)?;
            output.extend_from_slice(&count.to_le_bytes());
            for (key, text) in &self.dialog_entries {
                output.extend_from_slice(&key.dialog_id.to_le_bytes());
                output.extend_from_slice(&key.item_index.to_le_bytes());
                output.extend_from_slice(&key.control_id.to_le_bytes());
                write_string(&mut output, text, MAX_TEXT_BYTES)?;
            }
        }
        if output.len() > Self::MAX_ENCODED_LEN {
            return Err(LocalizationError::Overflow);
        }
        Ok(output)
    }

    /// Decodes one complete bounded `LMLOC001` catalog.
    ///
    /// # Errors
    ///
    /// Returns [`LocalizationError`] for malformed framing, invalid Unicode, unknown/duplicate
    /// keys, incomplete catalogs, or exceeded resource limits.
    pub fn decode(bytes: &[u8]) -> Result<Self, LocalizationError> {
        let mut reader = Reader::new(bytes);
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(LocalizationError::WrongMagic);
        }
        let locale = reader.string(MAX_LOCALE_BYTES)?;
        let count = usize::from(reader.u16()?);
        if count != UiTextKey::ALL.len()
            && count != PREVIOUS_COMPLETE_KEY_COUNT
            && !EARLIER_COMPLETE_KEY_COUNTS.contains(&count)
            && count != LEGACY_CHROME_KEY_COUNT
        {
            return Err(LocalizationError::WrongEntryCount(count));
        }
        let mut entries = BTreeMap::new();
        for _ in 0..count {
            let raw = reader.byte()?;
            let key = UiTextKey::from_byte(raw).ok_or(LocalizationError::UnknownKey(raw))?;
            let value = reader.string(MAX_TEXT_BYTES)?;
            if entries.insert(key, value).is_some() {
                return Err(LocalizationError::DuplicateKey(key));
            }
        }
        if count != UiTextKey::ALL.len() {
            for key in UiTextKey::ALL[..count].iter().copied() {
                if !entries.contains_key(&key) {
                    return Err(LocalizationError::MissingKey(key));
                }
            }
            for key in UiTextKey::ALL[count..].iter().copied() {
                entries.insert(key, key.english().into());
            }
        }
        let mut dialog_entries = BTreeMap::new();
        if !reader.is_empty() {
            if reader.remaining() < DIALOG_TEXT_MAGIC.len() + 2 {
                return Err(LocalizationError::TrailingBytes);
            }
            if reader.take(DIALOG_TEXT_MAGIC.len())? != DIALOG_TEXT_MAGIC {
                return Err(LocalizationError::WrongDialogTextMagic);
            }
            let count = usize::from(reader.u16()?);
            if count > MAX_DIALOG_TEXT_ENTRIES {
                return Err(LocalizationError::TooManyDialogTexts(count));
            }
            for _ in 0..count {
                let key = OriginalDialogTextKey {
                    dialog_id: reader.u16()?,
                    item_index: reader.u16()?,
                    control_id: reader.u32()?,
                };
                let value = reader.string(MAX_TEXT_BYTES)?;
                if dialog_entries.insert(key, value).is_some() {
                    return Err(LocalizationError::DuplicateDialogText(key));
                }
            }
            if !reader.is_empty() {
                return Err(LocalizationError::TrailingBytes);
            }
        }
        let catalog = Self {
            locale,
            entries,
            dialog_entries,
        };
        catalog.validate()?;
        Ok(catalog)
    }
}

fn write_string(output: &mut Vec<u8>, value: &str, limit: usize) -> Result<(), LocalizationError> {
    if value.len() > limit {
        return Err(LocalizationError::Overflow);
    }
    let len = u16::try_from(value.len()).map_err(|_| LocalizationError::Overflow)?;
    output.extend_from_slice(&len.to_le_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], LocalizationError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(LocalizationError::Overflow)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(LocalizationError::Truncated)?;
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, LocalizationError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, LocalizationError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("length checked"),
        ))
    }

    fn u32(&mut self) -> Result<u32, LocalizationError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("length checked"),
        ))
    }

    fn string(&mut self, limit: usize) -> Result<String, LocalizationError> {
        let len = usize::from(self.u16()?);
        if len > limit {
            return Err(LocalizationError::Overflow);
        }
        std::str::from_utf8(self.take(len)?)
            .map(str::to_owned)
            .map_err(|_| LocalizationError::InvalidUtf8)
    }

    const fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }

    const fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::DeflateEncoder};
    use std::io::Write;

    fn catalog() -> LocalizationCatalog {
        LocalizationCatalog::new(
            "fr-CA",
            UiTextKey::ALL.map(|key| (key, format!("texte-{key:?}"))),
        )
        .unwrap()
    }

    fn checksummed_original_module(payload: &[u8]) -> Vec<u8> {
        let mut bytes = payload.to_vec();
        bytes.resize(payload.len() + ORIGINAL_LANGUAGE_TRAILER_BYTES, 0);
        sign_original_module(&mut bytes);
        bytes
    }

    fn sign_original_module(bytes: &mut [u8]) {
        let payload_end = bytes.len() - ORIGINAL_LANGUAGE_TRAILER_BYTES;
        let checksum =
            bytes[..payload_end]
                .iter()
                .copied()
                .enumerate()
                .fold(0_u32, |sum, (offset, byte)| {
                    let transformed = if offset & 2 == 0 {
                        if offset & 1 == 0 {
                            u32::from(byte.rotate_left(2) ^ 0x46)
                        } else {
                            0_u32.wrapping_sub(u32::from(byte.rotate_left(4) ^ 0x77))
                        }
                    } else {
                        u32::from(
                            byte.wrapping_mul(0x80)
                                .wrapping_add(byte >> 1)
                                .wrapping_sub(0x17)
                                ^ 0x71,
                        )
                    };
                    sum.wrapping_add(transformed)
                });
        let checksum_offset = bytes.len() - ORIGINAL_LANGUAGE_CHECKSUM_FROM_END;
        bytes[checksum_offset..checksum_offset + 4].copy_from_slice(&checksum.to_le_bytes());
    }

    fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn original_language_pe(metadata: &[u8], pe32_plus: bool) -> Vec<u8> {
        let mut payload = vec![0_u8; 0x600];
        payload[..2].copy_from_slice(b"MZ");
        write_u32(&mut payload, 0x3c, 0x80);
        payload[0x80..0x84].copy_from_slice(b"PE\0\0");
        write_u16(&mut payload, 0x84, if pe32_plus { 0x8664 } else { 0x014c });
        write_u16(&mut payload, 0x86, 1);
        let optional = 0x98;
        let optional_size = if pe32_plus { 0xf0 } else { 0xe0 };
        write_u16(&mut payload, 0x94, optional_size);
        write_u16(
            &mut payload,
            optional,
            if pe32_plus { 0x020b } else { 0x010b },
        );
        write_u32(&mut payload, optional + 60, 0x200);
        let directory_base = optional + if pe32_plus { 112 } else { 96 };
        write_u32(
            &mut payload,
            optional + if pe32_plus { 108 } else { 92 },
            16,
        );
        write_u32(&mut payload, directory_base + 16, 0x1000);
        write_u32(&mut payload, directory_base + 20, 0x200);
        let section = optional + usize::from(optional_size);
        payload[section..section + 8].copy_from_slice(b".rsrc\0\0\0");
        write_u32(&mut payload, section + 8, 0x200);
        write_u32(&mut payload, section + 12, 0x1000);
        write_u32(&mut payload, section + 16, 0x400);
        write_u32(&mut payload, section + 20, 0x200);

        let root = 0x200;
        write_u16(&mut payload, root + 14, 1);
        write_u32(&mut payload, root + 16, u32::from(PE_RESOURCE_TYPE));
        write_u32(&mut payload, root + 20, 0x8000_0020);
        write_u16(&mut payload, root + 0x20 + 14, 2);
        write_u32(&mut payload, root + 0x30, 0x0db6);
        write_u32(&mut payload, root + 0x34, 0x8000_0048);
        write_u32(&mut payload, root + 0x38, 0x0db7);
        write_u32(&mut payload, root + 0x3c, 0x8000_0060);
        write_u16(&mut payload, root + 0x48 + 14, 1);
        write_u32(&mut payload, root + 0x58, 0x0409);
        write_u32(&mut payload, root + 0x5c, 0x80);
        write_u16(&mut payload, root + 0x60 + 14, 1);
        write_u32(&mut payload, root + 0x70, 0x0409);
        write_u32(&mut payload, root + 0x74, 0x90);
        write_u32(&mut payload, root + 0x80, 0x1100);
        write_u32(
            &mut payload,
            root + 0x84,
            u32::try_from(metadata.len()).unwrap(),
        );
        write_u32(&mut payload, root + 0x90, 0x1180);
        write_u32(&mut payload, root + 0x94, 4);
        payload[root + 0x100..root + 0x100 + metadata.len()].copy_from_slice(metadata);
        payload[root + 0x180..root + 0x184]
            .copy_from_slice(&ORIGINAL_LANGUAGE_MARKER.to_le_bytes());
        checksummed_original_module(&payload)
    }

    fn original_language_catalog_pe(
        metadata: &[u8],
        encoded_pool: &[u8],
        offsets: &[u8],
        lengths: &[u8],
    ) -> Vec<u8> {
        let mut payload = vec![0_u8; 0x1200];
        payload[..2].copy_from_slice(b"MZ");
        write_u32(&mut payload, 0x3c, 0x80);
        payload[0x80..0x84].copy_from_slice(b"PE\0\0");
        write_u16(&mut payload, 0x84, 0x014c);
        write_u16(&mut payload, 0x86, 1);
        let optional = 0x98;
        write_u16(&mut payload, 0x94, 0xe0);
        write_u16(&mut payload, optional, 0x010b);
        write_u32(&mut payload, optional + 60, 0x200);
        write_u32(&mut payload, optional + 92, 16);
        write_u32(&mut payload, optional + 96 + 16, 0x1000);
        write_u32(&mut payload, optional + 96 + 20, 0x1000);
        let section = optional + 0xe0;
        payload[section..section + 8].copy_from_slice(b".rsrc\0\0\0");
        write_u32(&mut payload, section + 8, 0x1000);
        write_u32(&mut payload, section + 12, 0x1000);
        write_u32(&mut payload, section + 16, 0x1000);
        write_u32(&mut payload, section + 20, 0x200);

        let marker = ORIGINAL_LANGUAGE_MARKER.to_le_bytes();
        let resources: [(u16, &[u8]); 5] = [
            (0x0db6, metadata),
            (0x0db7, &marker),
            (0x0dac, encoded_pool),
            (0x0dad, offsets),
            (0x0dae, lengths),
        ];
        let root = 0x200;
        write_u16(&mut payload, root + 14, 1);
        write_u32(&mut payload, root + 16, u32::from(PE_RESOURCE_TYPE));
        write_u32(&mut payload, root + 20, 0x8000_0020);
        write_u16(&mut payload, root + 0x20 + 14, resources.len() as u16);
        let language_directories = 0x60;
        let data_entries = 0xe0;
        let mut blob = 0x140;
        for (index, (id, bytes)) in resources.into_iter().enumerate() {
            let entry = 0x30 + index * 8;
            let language = language_directories + index * 0x18;
            let data = data_entries + index * 0x10;
            write_u32(&mut payload, root + entry, u32::from(id));
            write_u32(
                &mut payload,
                root + entry + 4,
                0x8000_0000 | u32::try_from(language).unwrap(),
            );
            write_u16(&mut payload, root + language + 14, 1);
            write_u32(&mut payload, root + language + 16, 0x0409);
            write_u32(
                &mut payload,
                root + language + 20,
                u32::try_from(data).unwrap(),
            );
            write_u32(
                &mut payload,
                root + data,
                0x1000 + u32::try_from(blob).unwrap(),
            );
            write_u32(
                &mut payload,
                root + data + 4,
                u32::try_from(bytes.len()).unwrap(),
            );
            payload[root + blob..root + blob + bytes.len()].copy_from_slice(bytes);
            blob = (blob + bytes.len() + 3) & !3;
        }
        assert!(blob <= 0x1000);
        checksummed_original_module(&payload)
    }

    fn original_language_dialog_pe(metadata: &[u8], dialogs: &[(u16, &[u8])]) -> Vec<u8> {
        assert!(dialogs.len() <= 2);
        let mut payload = vec![0_u8; 0x1200];
        payload[..2].copy_from_slice(b"MZ");
        write_u32(&mut payload, 0x3c, 0x80);
        payload[0x80..0x84].copy_from_slice(b"PE\0\0");
        write_u16(&mut payload, 0x84, 0x014c);
        write_u16(&mut payload, 0x86, 1);
        let optional = 0x98;
        write_u16(&mut payload, 0x94, 0xe0);
        write_u16(&mut payload, optional, 0x010b);
        write_u32(&mut payload, optional + 60, 0x200);
        write_u32(&mut payload, optional + 92, 16);
        write_u32(&mut payload, optional + 96 + 16, 0x1000);
        write_u32(&mut payload, optional + 96 + 20, 0x1000);
        let section = optional + 0xe0;
        payload[section..section + 8].copy_from_slice(b".rsrc\0\0\0");
        write_u32(&mut payload, section + 8, 0x1000);
        write_u32(&mut payload, section + 12, 0x1000);
        write_u32(&mut payload, section + 16, 0x1000);
        write_u32(&mut payload, section + 20, 0x200);

        let root = 0x200;
        write_u16(&mut payload, root + 14, 2);
        write_u32(&mut payload, root + 0x10, 5);
        write_u32(&mut payload, root + 0x14, 0x8000_0060);
        write_u32(&mut payload, root + 0x18, u32::from(PE_RESOURCE_TYPE));
        write_u32(&mut payload, root + 0x1c, 0x8000_0030);

        let marker = ORIGINAL_LANGUAGE_MARKER.to_le_bytes();
        let metadata_resources: [(u16, &[u8]); 2] = [(0x0db6, metadata), (0x0db7, &marker)];
        write_u16(&mut payload, root + 0x30 + 14, 2);
        for (index, (id, _)) in metadata_resources.iter().enumerate() {
            write_u32(&mut payload, root + 0x40 + index * 8, u32::from(*id));
            write_u32(
                &mut payload,
                root + 0x44 + index * 8,
                0x8000_0000 | (0x90 + u32::try_from(index).unwrap() * 0x20),
            );
        }

        write_u16(
            &mut payload,
            root + 0x60 + 14,
            u16::try_from(dialogs.len()).unwrap(),
        );
        for (index, (id, _)) in dialogs.iter().enumerate() {
            write_u32(&mut payload, root + 0x70 + index * 8, u32::from(*id));
            write_u32(
                &mut payload,
                root + 0x74 + index * 8,
                0x8000_0000 | (0xd0 + u32::try_from(index).unwrap() * 0x20),
            );
        }

        let resources = metadata_resources
            .iter()
            .map(|(_, bytes)| *bytes)
            .chain(dialogs.iter().map(|(_, bytes)| *bytes));
        let mut blob = 0x180;
        for (index, bytes) in resources.enumerate() {
            let language = 0x90 + index * 0x20;
            let data = 0x120 + index * 0x10;
            write_u16(&mut payload, root + language + 14, 1);
            write_u32(&mut payload, root + language + 16, 0x0409);
            write_u32(
                &mut payload,
                root + language + 20,
                u32::try_from(data).unwrap(),
            );
            write_u32(
                &mut payload,
                root + data,
                0x1000 + u32::try_from(blob).unwrap(),
            );
            write_u32(
                &mut payload,
                root + data + 4,
                u32::try_from(bytes.len()).unwrap(),
            );
            payload[root + blob..root + blob + bytes.len()].copy_from_slice(bytes);
            blob = (blob + bytes.len() + 3) & !3;
        }
        assert!(blob <= 0x1000);
        checksummed_original_module(&payload)
    }

    fn original_language_dialog_template(title: &str) -> Vec<u8> {
        let mut bytes = vec![0; 18];
        bytes.extend_from_slice(&0_u16.to_le_bytes()); // no menu
        bytes.extend_from_slice(&0_u16.to_le_bytes()); // default dialog class
        for unit in title.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes
    }

    fn encoded_original_string_pool(decoded: &[u8]) -> Vec<u8> {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(decoded).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut encoded = compressed.clone();
        for index in 1..encoded.len() {
            encoded[index] = compressed[index]
                .wrapping_sub(0x34)
                .wrapping_add(compressed[index - 1])
                ^ 0x92;
        }
        encoded
    }

    fn string_table(values: &[u32], include_count: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        if include_count {
            bytes.extend_from_slice(&u32::try_from(values.len()).unwrap().to_le_bytes());
        }
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn original_language_checksum_matches_recovered_three_branch_transform() {
        let mut payload = vec![0; 64];
        payload[..4].copy_from_slice(&[0x01, 0x12, 0x34, 0x56]);
        let mut module = checksummed_original_module(&payload);
        let stored_offset = module.len() - ORIGINAL_LANGUAGE_CHECKSUM_FROM_END;
        assert_eq!(
            u32::from_le_bytes(module[stored_offset..stored_offset + 4].try_into().unwrap()),
            4_020
        );
        assert_eq!(validate_original_language_module_checksum(&module), Ok(()));

        module[0] ^= 1;
        assert!(matches!(
            validate_original_language_module_checksum(&module),
            Err(OriginalLanguageModuleError::ChecksumMismatch { .. })
        ));
        assert_eq!(
            validate_original_language_module_checksum(&module[..63]),
            Err(OriginalLanguageModuleError::ModuleTooShort(63))
        );
    }

    #[test]
    fn original_language_metadata_decodes_bom_crlf_and_four_fields() {
        let marker = ORIGINAL_LANGUAGE_MARKER.to_le_bytes();
        let decoded = OriginalLanguageModuleMetadata::decode(
            &marker,
            b"\xef\xbb\xbfFran\xc3\xa7ais - Test\r\n3.63\r\nfr-CA\r\n1252\r\nignored",
        )
        .unwrap();
        assert_eq!(
            decoded,
            OriginalLanguageModuleMetadata {
                display_name: "Français - Test".into(),
                version: "3.63".into(),
                locale: "fr-CA".into(),
                code_page: "1252".into(),
            }
        );
    }

    #[test]
    fn original_language_metadata_rejects_marker_bounds_utf8_and_missing_fields() {
        let marker = ORIGINAL_LANGUAGE_MARKER.to_le_bytes();
        assert_eq!(
            OriginalLanguageModuleMetadata::decode(&[0; 4], b"a\nb\nc\nd\n"),
            Err(OriginalLanguageModuleError::WrongMarker)
        );
        assert_eq!(
            OriginalLanguageModuleMetadata::decode(
                &marker,
                &vec![b'x'; ORIGINAL_LANGUAGE_METADATA_MAX_BYTES + 1]
            ),
            Err(OriginalLanguageModuleError::MetadataTooLong(
                ORIGINAL_LANGUAGE_METADATA_MAX_BYTES + 1
            ))
        );
        assert_eq!(
            OriginalLanguageModuleMetadata::decode(&marker, &[0xff]),
            Err(OriginalLanguageModuleError::InvalidUtf8)
        );
        assert_eq!(
            OriginalLanguageModuleMetadata::decode(&marker, b"a\nb\nc"),
            Err(OriginalLanguageModuleError::MissingMetadataFields)
        );
    }

    #[test]
    fn original_language_dll_extracts_integer_resources_from_pe32_and_pe32_plus() {
        let metadata = b"Deutsch - Test\n3.63\nde-DE\n1252\n";
        let expected = OriginalLanguageModuleMetadata {
            display_name: "Deutsch - Test".into(),
            version: "3.63".into(),
            locale: "de-DE".into(),
            code_page: "1252".into(),
        };
        assert_eq!(
            decode_original_language_module(&original_language_pe(metadata, false)).unwrap(),
            expected
        );
        assert_eq!(
            decode_original_language_module(&original_language_pe(metadata, true)).unwrap(),
            expected
        );
    }

    #[test]
    fn original_language_dll_rejects_missing_and_out_of_bounds_resources() {
        let metadata = b"English - Test\n3.63\nen\n1252\n";
        let mut missing = original_language_pe(metadata, false);
        write_u32(&mut missing, 0x200 + 0x38, 0x0db8);
        sign_original_module(&mut missing);
        assert_eq!(
            decode_original_language_module(&missing),
            Err(OriginalLanguageModuleError::MissingResource(0x0db7))
        );

        let mut out_of_bounds = original_language_pe(metadata, false);
        write_u32(&mut out_of_bounds, 0x200 + 0x90, 0xffff_f000);
        sign_original_module(&mut out_of_bounds);
        assert_eq!(
            decode_original_language_module(&out_of_bounds),
            Err(OriginalLanguageModuleError::ResourceBounds)
        );
    }

    #[test]
    fn original_language_pe_reader_rejects_every_truncated_prefix_without_panicking() {
        let module = original_language_pe(b"English\n3.63\nen\n1252\n", false);
        for end in 0..0x384 {
            let result = PeResources::parse(&module[..end]).and_then(|resources| {
                resources.resource(0x0db6)?;
                resources.resource(0x0db7)?;
                Ok(())
            });
            assert!(result.is_err(), "prefix {end}");
        }
        let resources = PeResources::parse(&module).unwrap();
        assert!(resources.resource(0x0db6).is_ok());
        assert!(resources.resource(0x0db7).is_ok());
    }

    #[test]
    fn original_language_string_resources_decode_obfuscation_deflate_and_tables() {
        let encoded = encoded_original_string_pool(b"hello\0world\0");
        let offsets = string_table(&[0, 6], true);
        let lengths = string_table(&[5, 5], false);
        let pool = decode_original_language_string_resources(&encoded, &offsets, &lengths).unwrap();
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.get(0), Some("hello"));
        assert_eq!(pool.get(1), Some("world"));
        assert_eq!(pool.get(2), None);
    }

    #[test]
    fn original_language_string_resources_bound_count_and_clear_invalid_entries() {
        let encoded = encoded_original_string_pool(b"\0");
        let offsets = string_table(&vec![0; ORIGINAL_LANGUAGE_MAX_STRINGS + 1], true);
        let lengths = string_table(&vec![0; ORIGINAL_LANGUAGE_MAX_STRINGS + 1], false);
        let pool = decode_original_language_string_resources(&encoded, &offsets, &lengths).unwrap();
        assert_eq!(pool.len(), ORIGINAL_LANGUAGE_MAX_STRINGS);
        assert_eq!(pool.get(ORIGINAL_LANGUAGE_MAX_STRINGS - 1), Some(""));

        let encoded = encoded_original_string_pool(b"hello\0world\0");
        let pool = decode_original_language_string_resources(
            &encoded,
            &string_table(&[0, 6], true),
            &string_table(&[6, 5], false),
        )
        .unwrap();
        assert_eq!(pool.get(0), None);
        assert_eq!(pool.get(1), Some("world"));
    }

    #[test]
    fn original_language_fixed_buffer_ceilings_match_recovered_boundaries() {
        assert!(original_language_string_length_is_allowed(0x0119, 0x007f));
        assert!(!original_language_string_length_is_allowed(0x0119, 0x0080));

        assert!(original_language_string_length_is_allowed(0x0212, 0x05ff));
        assert!(!original_language_string_length_is_allowed(0x0212, 0x0600));
        assert!(!original_language_string_length_is_allowed(0x071b, 0x0600));
        assert!(original_language_string_length_is_allowed(0x071c, 0x00ff));
        assert!(!original_language_string_length_is_allowed(0x071c, 0x0100));

        assert!(original_language_string_length_is_allowed(0x071b, 0x05ff));
        assert!(original_language_string_length_is_allowed(
            0x16c6,
            usize::MAX
        ));
        assert!(original_language_string_length_is_allowed(0, usize::MAX));
    }

    #[test]
    fn original_language_decoder_clears_oversized_guarded_entry_only() {
        let mut inflated = vec![b'a'; 0x80];
        inflated.push(0);
        inflated.extend_from_slice(b"kept\0");
        let encoded = encoded_original_string_pool(&inflated);
        let count = 0x011a;
        let mut offsets = vec![0x81_u32; count];
        let mut lengths = vec![4_u32; count];
        offsets[0x0119] = 0;
        lengths[0x0119] = 0x80;
        let pool = decode_original_language_string_resources(
            &encoded,
            &string_table(&offsets, true),
            &string_table(&lengths, false),
        )
        .unwrap();
        assert_eq!(pool.get(0x0118), Some("kept"));
        assert_eq!(pool.get(0x0119), None);
    }

    #[test]
    fn original_language_pool_maps_only_evidence_backed_typed_ui_strings() {
        let mut strings = vec![None; ORIGINAL_LANGUAGE_MAX_STRINGS];
        strings[0x000a] = Some("&Fichier".into());
        strings[0x000b] = Some("&Édition".into());
        strings[0x0011] = Some("&Ouvrir ROM...\tCtrl+O".into());
        strings[0x0055] = Some("&Annuler\tCtrl+Z".into());
        strings[0x0109] = Some("Musique && &Temps...".into());
        strings[0x0119] = Some("À &propos de %s...".into());
        let catalog = OriginalLanguageStringPool { strings }
            .to_catalog("fr-FR")
            .unwrap();

        assert_eq!(catalog.text(UiTextKey::MenuFile), "Fichier");
        assert_eq!(catalog.text(UiTextKey::MenuEdit), "Édition");
        assert_eq!(catalog.text(UiTextKey::FileOpen), "Ouvrir ROM…");
        assert_eq!(catalog.text(UiTextKey::EditUndo), "Annuler");
        assert_eq!(
            catalog.text(UiTextKey::HelpAbout),
            "À propos de Lunar Magic Rust…"
        );
        assert_eq!(
            catalog.text(UiTextKey::MenuTools),
            UiTextKey::MenuTools.english()
        );
        assert_eq!(
            catalog.text(UiTextKey::EditorNativeGraphics),
            UiTextKey::EditorNativeGraphics.english()
        );
    }

    #[test]
    fn original_language_pool_falls_back_for_missing_empty_and_invalid_locale() {
        let mut strings = vec![None; 0x0012];
        strings[0x000a] = Some(String::new());
        let pool = OriginalLanguageStringPool { strings };
        let catalog = pool.to_catalog("de").unwrap();
        assert_eq!(
            catalog.text(UiTextKey::MenuFile),
            UiTextKey::MenuFile.english()
        );
        assert_eq!(
            catalog.text(UiTextKey::FileOpen),
            UiTextKey::FileOpen.english()
        );
        assert_eq!(pool.to_catalog(""), Err(LocalizationError::InvalidLocale));
    }

    #[test]
    fn original_dialog_templates_override_only_evidence_backed_typed_actions() {
        let mut catalog = OriginalLanguageStringPool {
            strings: Vec::new(),
        }
        .to_catalog("fr-FR")
        .unwrap();
        let dialogs = vec![
            (
                0x042b,
                OriginalLanguageDialogTemplate {
                    extended: true,
                    title: Some("Langue".into()),
                    controls: vec![
                        OriginalLanguageDialogControl {
                            id: 1,
                            class_ordinal: Some(0x80),
                            text: Some("&Valider".into()),
                        },
                        OriginalLanguageDialogControl {
                            id: 2,
                            class_ordinal: Some(0x80),
                            text: Some("A&nnuler".into()),
                        },
                    ],
                },
            ),
            (
                0x03f8,
                OriginalLanguageDialogTemplate {
                    extended: false,
                    title: Some("À propos".into()),
                    controls: vec![
                        OriginalLanguageDialogControl {
                            id: 1,
                            class_ordinal: Some(0x80),
                            text: Some("&Fermer".into()),
                        },
                        OriginalLanguageDialogControl {
                            id: 0x66,
                            class_ordinal: Some(0x80),
                            text: Some("Extensions &tierces".into()),
                        },
                        OriginalLanguageDialogControl {
                            id: 0x67,
                            class_ordinal: Some(0x80),
                            text: Some("Avis &juridique".into()),
                        },
                    ],
                },
            ),
        ];
        for (dialog_id, template) in &dialogs {
            catalog.insert_original_dialog_template(*dialog_id, template);
        }
        apply_original_dialog_catalog_overrides(&mut catalog, dialogs);
        assert_eq!(catalog.text(UiTextKey::CommonOk), "Valider");
        assert_eq!(catalog.text(UiTextKey::CommonCancel), "Annuler");
        assert_eq!(catalog.text(UiTextKey::AboutOk), "Fermer");
        assert_eq!(
            catalog.text(UiTextKey::AboutThirdPartyEnhancements),
            "Extensions tierces"
        );
        assert_eq!(catalog.text(UiTextKey::AboutLegalNotice), "Avis juridique");
        assert_eq!(
            catalog.text(UiTextKey::AboutWindowTitleFormat),
            UiTextKey::AboutWindowTitleFormat.english()
        );
        assert_eq!(catalog.original_dialog_title(0x042b), Some("Langue"));
        assert_eq!(
            catalog.original_dialog_control_text(0x03f8, 0x66),
            Some("Extensions tierces")
        );
        let reopened = LocalizationCatalog::decode(&catalog.encode().unwrap()).unwrap();
        assert_eq!(reopened, catalog);
    }

    #[test]
    fn original_language_module_catalog_decodes_all_five_resources_end_to_end() {
        let count = 0x011a;
        let mut decoded = Vec::new();
        let mut offsets = Vec::with_capacity(count);
        let mut lengths = Vec::with_capacity(count);
        for index in 0..count {
            let text: &[u8] = match index {
                0x000a => b"&Datei",
                0x0011 => b"ROM &oeffnen...\tCtrl+O",
                0x0119 => b"&Ueber %s...",
                _ => b"",
            };
            offsets.push(u32::try_from(decoded.len()).unwrap());
            lengths.push(u32::try_from(text.len()).unwrap());
            decoded.extend_from_slice(text);
            decoded.push(0);
        }
        let module = original_language_catalog_pe(
            b"Deutsch - Test\n3.63\nde-DE\n1252\n",
            &encoded_original_string_pool(&decoded),
            &string_table(&offsets, true),
            &string_table(&lengths, false),
        );
        let (metadata, catalog) = decode_original_language_module_catalog(&module).unwrap();
        assert_eq!(metadata.display_name, "Deutsch - Test");
        assert_eq!(catalog.locale(), "de-DE");
        assert_eq!(catalog.text(UiTextKey::MenuFile), "Datei");
        assert_eq!(catalog.text(UiTextKey::FileOpen), "ROM oeffnen…");
        assert_eq!(
            catalog.text(UiTextKey::HelpAbout),
            "Ueber Lunar Magic Rust…"
        );
        assert_eq!(
            catalog.text(UiTextKey::MenuTools),
            UiTextKey::MenuTools.english()
        );
    }

    #[test]
    fn original_language_dialog_map_matches_recovered_contract() {
        assert_eq!(ORIGINAL_LANGUAGE_DIALOG_RESOURCE_IDS.len(), 107);
        assert_eq!(
            ORIGINAL_LANGUAGE_DIALOG_RESOURCE_IDS.first(),
            Some(&(0x03e8, 0x07d0))
        );
        assert_eq!(
            ORIGINAL_LANGUAGE_DIALOG_RESOURCE_IDS.last(),
            Some(&(0x04d7, 0x08bf))
        );
        assert!(
            ORIGINAL_LANGUAGE_DIALOG_RESOURCE_IDS
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0)
        );
    }

    #[test]
    fn original_language_dialogs_decode_type_five_resources_and_omit_missing_mappings() {
        let first = original_language_dialog_template("First dialog");
        let last = original_language_dialog_template("Last dialog");
        let module = original_language_dialog_pe(
            b"Deutsch - Test\n3.63\nde-DE\n1252\n",
            &[(0x07d0, &first), (0x08bf, &last)],
        );
        let dialogs = decode_original_language_module_dialogs(&module).unwrap();
        assert_eq!(dialogs.len(), 2);
        assert_eq!(dialogs[0].original_id, 0x03e8);
        assert_eq!(dialogs[0].localized_id, 0x07d0);
        assert_eq!(dialogs[0].bytes(), first);
        assert_eq!(
            dialogs[0].decode().unwrap().title.as_deref(),
            Some("First dialog")
        );
        assert_eq!(dialogs[1].original_id, 0x04d7);
        assert_eq!(dialogs[1].localized_id, 0x08bf);
        assert_eq!(dialogs[1].bytes(), last);
        assert_eq!(
            dialogs[1].decode().unwrap().title.as_deref(),
            Some("Last dialog")
        );
    }

    #[test]
    fn original_language_dialogs_require_valid_module_metadata_and_resource_bounds() {
        let mut wrong_marker = original_language_dialog_pe(
            b"English - Test\n3.63\nen\n1252\n",
            &[(0x07d0, b"dialog")],
        );
        let marker_rva = u32::from_le_bytes(wrong_marker[0x330..0x334].try_into().unwrap());
        let marker_offset = 0x200 + usize::try_from(marker_rva - 0x1000).unwrap();
        wrong_marker[marker_offset] ^= 1;
        sign_original_module(&mut wrong_marker);
        assert_eq!(
            decode_original_language_module_dialogs(&wrong_marker),
            Err(OriginalLanguageModuleError::WrongMarker)
        );

        let mut out_of_bounds = original_language_dialog_pe(
            b"English - Test\n3.63\nen\n1252\n",
            &[(0x07d0, b"dialog")],
        );
        write_u32(&mut out_of_bounds, 0x200 + 0x140, 0xffff_f000);
        sign_original_module(&mut out_of_bounds);
        assert_eq!(
            decode_original_language_module_dialogs(&out_of_bounds),
            Err(OriginalLanguageModuleError::ResourceBounds)
        );
    }

    #[test]
    #[ignore = "requires a locally supplied Lunar Magic 3.63 executable"]
    fn every_original_363_dialog_resource_decodes_with_the_portable_template_parser() {
        let path = std::env::var_os("LM_ORIGINAL_EXE")
            .expect("LM_ORIGINAL_EXE must name the locally supplied 3.63 executable");
        let bytes = std::fs::read(path).unwrap();
        let resources = PeResources::parse(&bytes).unwrap();
        for &(original_id, _) in ORIGINAL_LANGUAGE_DIALOG_RESOURCE_IDS {
            let bytes = resources.resource_of_type(5, original_id).unwrap();
            let template = decode_original_language_dialog_template(bytes)
                .unwrap_or_else(|error| panic!("dialog {original_id:#06x}: {error}"));
            if std::env::var_os("LM_PRINT_ORIGINAL_DIALOG_TEXT").is_some() {
                eprintln!("{original_id:#06x}\tdialog\t{:?}", template.title);
                for control in template.controls {
                    if control.text.is_some() {
                        eprintln!(
                            "{original_id:#06x}\t{:#010x}\t{:?}",
                            control.id, control.text
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn original_language_string_resources_reject_bad_tables_deflate_and_utf8() {
        assert_eq!(
            decode_original_language_string_resources(&[], &[], &[]),
            Err(OriginalLanguageModuleError::MalformedStringTables)
        );
        assert!(matches!(
            decode_original_language_string_resources(&[1, 2, 3], &0_u32.to_le_bytes(), &[]),
            Err(OriginalLanguageModuleError::Inflate(_))
        ));
        assert_eq!(
            decode_original_language_string_resources(
                &encoded_original_string_pool(&[0xff, 0]),
                &string_table(&[0], true),
                &string_table(&[1], false),
            ),
            Err(OriginalLanguageModuleError::InvalidStringUtf8(0))
        );

        let encoded = encoded_original_string_pool(&vec![0; 129]);
        let mut compressed = encoded.clone();
        for index in 1..compressed.len() {
            compressed[index] = (compressed[index] ^ 0x92)
                .wrapping_sub(compressed[index - 1])
                .wrapping_add(0x34);
        }
        assert!(matches!(
            inflate_original_language_pool_with_limit(&compressed, 128),
            Err(OriginalLanguageModuleError::InflatedPoolTooLong(_))
        ));
    }

    #[test]
    fn complete_unicode_catalog_round_trips_canonically() {
        let mut expected = catalog();
        expected
            .entries
            .insert(UiTextKey::AppTitle, "Éditeur 🌙".into());
        let bytes = expected.encode().unwrap();
        assert_eq!(LocalizationCatalog::decode(&bytes).unwrap(), expected);
        assert_eq!(
            LocalizationCatalog::decode(&bytes)
                .unwrap()
                .encode()
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn legacy_nineteen_key_catalogs_upgrade_with_english_menu_fallbacks() {
        let mut bytes = MAGIC.to_vec();
        write_string(&mut bytes, "de-DE", MAX_LOCALE_BYTES).unwrap();
        bytes.extend_from_slice(&(LEGACY_CHROME_KEY_COUNT as u16).to_le_bytes());
        for key in UiTextKey::ALL[..LEGACY_CHROME_KEY_COUNT].iter().copied() {
            bytes.push(key as u8);
            write_string(&mut bytes, &format!("alt-{key:?}"), MAX_TEXT_BYTES).unwrap();
        }
        let upgraded = LocalizationCatalog::decode(&bytes).unwrap();
        assert_eq!(upgraded.text(UiTextKey::AppTitle), "alt-AppTitle");
        assert_eq!(upgraded.text(UiTextKey::MenuFile), "File");
        assert_eq!(
            upgraded.text(UiTextKey::HelpCompatibilityDiagnostics),
            "Compatibility diagnostics…"
        );
        assert_eq!(
            LocalizationCatalog::decode(&upgraded.encode().unwrap()).unwrap(),
            upgraded
        );
    }

    #[test]
    fn previous_complete_catalogs_append_new_keys_with_english_fallback() {
        for count in EARLIER_COMPLETE_KEY_COUNTS
            .into_iter()
            .chain([PREVIOUS_COMPLETE_KEY_COUNT])
        {
            let mut bytes = MAGIC.to_vec();
            write_string(&mut bytes, "es-MX", MAX_LOCALE_BYTES).unwrap();
            bytes.extend_from_slice(&(count as u16).to_le_bytes());
            for key in UiTextKey::ALL[..count].iter().copied() {
                bytes.push(key as u8);
                write_string(&mut bytes, &format!("viejo-{key:?}"), MAX_TEXT_BYTES).unwrap();
            }

            let upgraded = LocalizationCatalog::decode(&bytes).unwrap();
            for (index, key) in UiTextKey::ALL.into_iter().enumerate() {
                assert_eq!(
                    upgraded.text(key),
                    if index < count {
                        format!("viejo-{key:?}")
                    } else {
                        key.english().to_owned()
                    },
                    "wrong migration at key {key:?} for historical count {count}"
                );
            }
            assert_eq!(
                LocalizationCatalog::decode(&upgraded.encode().unwrap()).unwrap(),
                upgraded
            );
        }
    }

    #[test]
    fn published_encoded_limit_accepts_the_largest_valid_catalog() {
        let mut catalog = LocalizationCatalog::new(
            "l".repeat(MAX_LOCALE_BYTES),
            UiTextKey::ALL.map(|key| (key, "x".repeat(MAX_TEXT_BYTES))),
        )
        .unwrap();
        for index in 0..MAX_DIALOG_TEXT_ENTRIES {
            catalog.dialog_entries.insert(
                OriginalDialogTextKey {
                    dialog_id: 1,
                    item_index: u16::try_from(index).unwrap(),
                    control_id: u32::try_from(index).unwrap(),
                },
                "d".repeat(MAX_TEXT_BYTES),
            );
        }
        let bytes = catalog.encode().unwrap();
        assert_eq!(bytes.len(), LocalizationCatalog::MAX_ENCODED_LEN);
        assert_eq!(LocalizationCatalog::decode(&bytes).unwrap(), catalog);
    }

    #[test]
    fn dialog_text_extension_round_trips_duplicate_control_ids_by_item_position() {
        let mut catalog = catalog();
        for (item_index, text) in [(3, "Premier"), (7, "Deuxième")] {
            catalog.dialog_entries.insert(
                OriginalDialogTextKey {
                    dialog_id: 0x03f0,
                    item_index,
                    control_id: u32::MAX,
                },
                text.into(),
            );
        }
        catalog.dialog_entries.insert(
            OriginalDialogTextKey {
                dialog_id: 0x03f0,
                item_index: DIALOG_TITLE_ITEM_INDEX,
                control_id: DIALOG_TITLE_CONTROL_ID,
            },
            "Entrées".into(),
        );
        let decoded = LocalizationCatalog::decode(&catalog.encode().unwrap()).unwrap();
        assert_eq!(decoded, catalog);
        assert_eq!(decoded.original_dialog_title(0x03f0), Some("Entrées"));
        assert_eq!(
            decoded.original_dialog_control_text(0x03f0, u32::MAX),
            Some("Premier")
        );
        assert_eq!(
            decoded.original_dialog_item_text(0x03f0, 7),
            Some("Deuxième")
        );
    }

    #[test]
    fn typed_rust_extension_round_trips_without_colliding_with_original_dialogs() {
        let original = OriginalDialogTextKey {
            dialog_id: 0x019d,
            item_index: 2,
            control_id: 1001,
        };
        let catalog = catalog()
            .with_original_dialog_texts([(original, "Original caption".into())])
            .unwrap()
            .with_extended_ui_texts([
                (ExtendedUiTextKey::TilemapRow, "Fila".into()),
                (ExtendedUiTextKey::TilemapCommit, "Guardar mapa".into()),
            ])
            .unwrap();
        assert_eq!(catalog.extended_text(ExtendedUiTextKey::TilemapRow), "Fila");
        assert_eq!(
            catalog.extended_text(ExtendedUiTextKey::TilemapColumn),
            "Column"
        );
        assert_eq!(
            catalog.original_dialog_item_text(original.dialog_id, original.item_index),
            Some("Original caption")
        );
        let decoded = LocalizationCatalog::decode(&catalog.encode().unwrap()).unwrap();
        assert_eq!(decoded, catalog);
        assert_eq!(
            decoded.extended_text(ExtendedUiTextKey::TilemapCommit),
            "Guardar mapa"
        );
    }

    #[test]
    fn typed_rust_extension_rejects_duplicates_unknown_ids_and_original_injection() {
        let duplicate = catalog().with_extended_ui_texts([
            (ExtendedUiTextKey::TilemapRow, "A".into()),
            (ExtendedUiTextKey::TilemapRow, "B".into()),
        ]);
        assert!(matches!(
            duplicate,
            Err(LocalizationError::DuplicateDialogText(_))
        ));

        let reserved = OriginalDialogTextKey {
            dialog_id: RUST_UI_DIALOG_ID,
            item_index: RUST_UI_ITEM_INDEX,
            control_id: ExtendedUiTextKey::ALL.len() as u32,
        };
        assert_eq!(
            catalog().with_original_dialog_texts([(reserved, "bad".into())]),
            Err(LocalizationError::InvalidDialogTextKey(reserved))
        );
    }

    #[test]
    fn dialog_text_extension_rejects_every_truncation_bad_magic_count_duplicate_and_title_key() {
        let keys = [
            OriginalDialogTextKey {
                dialog_id: 0x03f0,
                item_index: 1,
                control_id: 1,
            },
            OriginalDialogTextKey {
                dialog_id: 0x03f0,
                item_index: 2,
                control_id: 2,
            },
        ];
        let extended_catalog = catalog()
            .with_original_dialog_texts([(keys[0], "A".into()), (keys[1], "B".into())])
            .unwrap();
        let bytes = extended_catalog.encode().unwrap();
        let extension = bytes
            .windows(DIALOG_TEXT_MAGIC.len())
            .position(|window| window == DIALOG_TEXT_MAGIC)
            .unwrap();
        for end in extension + 1..bytes.len() {
            assert!(
                LocalizationCatalog::decode(&bytes[..end]).is_err(),
                "end {end}"
            );
        }

        let mut wrong_magic = bytes.clone();
        wrong_magic[extension] ^= 1;
        assert_eq!(
            LocalizationCatalog::decode(&wrong_magic),
            Err(LocalizationError::WrongDialogTextMagic)
        );

        let mut too_many = bytes.clone();
        too_many[extension + 8..extension + 10].copy_from_slice(
            &u16::try_from(MAX_DIALOG_TEXT_ENTRIES + 1)
                .unwrap()
                .to_le_bytes(),
        );
        assert_eq!(
            LocalizationCatalog::decode(&too_many),
            Err(LocalizationError::TooManyDialogTexts(
                MAX_DIALOG_TEXT_ENTRIES + 1
            ))
        );

        let mut duplicate = bytes;
        let first_key = extension + 10;
        let second_key = first_key + 8 + 2 + 1;
        let key_bytes: [u8; 8] = duplicate[first_key..first_key + 8].try_into().unwrap();
        duplicate[second_key..second_key + 8].copy_from_slice(&key_bytes);
        assert_eq!(
            LocalizationCatalog::decode(&duplicate),
            Err(LocalizationError::DuplicateDialogText(keys[0]))
        );

        let invalid_title = OriginalDialogTextKey {
            dialog_id: 1,
            item_index: DIALOG_TITLE_ITEM_INDEX,
            control_id: 7,
        };
        assert_eq!(
            catalog().with_original_dialog_texts([(invalid_title, "bad".into())]),
            Err(LocalizationError::InvalidDialogTextKey(invalid_title))
        );
    }

    #[test]
    fn every_truncation_trailing_byte_and_duplicate_key_is_rejected_at_full_capacity() {
        let bytes = catalog().encode().unwrap();
        for end in 0..bytes.len() {
            assert!(LocalizationCatalog::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            LocalizationCatalog::decode(&trailing),
            Err(LocalizationError::TrailingBytes)
        );
        let mut duplicate = bytes;
        let first_key = MAGIC.len() + 2 + "fr-CA".len() + 2;
        duplicate[first_key] = 0xff;
        assert_eq!(
            LocalizationCatalog::decode(&duplicate),
            Err(LocalizationError::DuplicateKey(
                UiTextKey::Map16SetErrorTitle
            ))
        );
    }

    #[test]
    fn missing_duplicate_and_invalid_values_fail_validation() {
        let mut entries = UiTextKey::ALL.map(|key| (key, format!("{key:?}"))).to_vec();
        entries.pop();
        assert!(matches!(
            LocalizationCatalog::new("en", entries),
            Err(LocalizationError::MissingKey(_))
        ));
        let mut duplicate = UiTextKey::ALL.map(|key| (key, format!("{key:?}"))).to_vec();
        duplicate.push((UiTextKey::AppTitle, "again".into()));
        assert_eq!(
            LocalizationCatalog::new("en", duplicate),
            Err(LocalizationError::DuplicateKey(UiTextKey::AppTitle))
        );
        assert!(matches!(
            LocalizationCatalog::new("", UiTextKey::ALL.map(|key| (key, "x".into()))),
            Err(LocalizationError::InvalidLocale)
        ));
    }
}
