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
}

impl ExtendedUiTextKey {
    pub const ALL: [Self; 328] = [
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
    ];

    #[must_use]
    pub const fn english(self) -> &'static str {
        match self {
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
