//! Narrow safe wrappers around Windows APIs needed by the native frontend.

use windows_sys::Win32::Foundation::GlobalFree;
use windows_sys::Win32::Globalization::{
    GetThreadPreferredUILanguages, GetUserDefaultUILanguage, LCIDToLocaleName,
};
use windows_sys::Win32::System::{
    DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    },
    Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
    Ole::CF_UNICODETEXT,
};

const MERGED_USER_AND_SYSTEM_FALLBACKS: u32 = 0x30;
const MAX_LANGUAGE_LIST_UTF16_UNITS: u32 = 0x600;
const LUNAR_MAGIC_GRAPHICS_TILE_FORMAT: &str = "Lunar Magic 8x8 Tile";
const LUNAR_MAGIC_GRAPHICS_TILE_BYTES: usize = 64;

/// Publishes Lunar Magic's native 64-byte graphics-tile record and a Unicode text fallback in one
/// clipboard transaction. The custom allocation is transferred to Windows only after
/// `SetClipboardData` succeeds.
///
/// # Errors
///
/// Returns an error when the tile is not exactly 64 bytes or a required Win32 clipboard operation
/// fails.
pub fn write_graphics_tile_clipboard(tile: &[u8], fallback_text: &str) -> Result<(), String> {
    if tile.len() != LUNAR_MAGIC_GRAPHICS_TILE_BYTES {
        return Err("Lunar Magic graphics clipboard tile must contain exactly 64 bytes".into());
    }
    let format = register_graphics_tile_format()?;
    let fallback = fallback_text
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_ne_bytes)
        .collect::<Vec<_>>();
    let custom = allocate_global_copy(tile)?;
    let unicode = match allocate_global_copy(&fallback) {
        Ok(unicode) => unicode,
        Err(error) => {
            free_global(custom);
            return Err(error);
        }
    };
    // SAFETY: A null owner is valid for a short synchronous clipboard transaction.
    if unsafe { OpenClipboard(std::ptr::null_mut()) } == 0 {
        free_global(custom);
        free_global(unicode);
        return Err("could not open the Windows clipboard".into());
    }
    // SAFETY: The current thread owns the open clipboard.
    if unsafe { EmptyClipboard() } == 0 {
        // SAFETY: This thread opened the clipboard above.
        unsafe { CloseClipboard() };
        free_global(custom);
        free_global(unicode);
        return Err("could not empty the Windows clipboard".into());
    }
    // SAFETY: `custom` is a movable global-memory block; ownership transfers on success.
    if unsafe { SetClipboardData(format, custom) }.is_null() {
        // SAFETY: This thread opened the clipboard above.
        unsafe { CloseClipboard() };
        free_global(custom);
        free_global(unicode);
        return Err("could not publish Lunar Magic graphics clipboard data".into());
    }
    // SAFETY: `unicode` is a movable, NUL-terminated UTF-16 global-memory block; ownership
    // transfers on success.
    if unsafe { SetClipboardData(u32::from(CF_UNICODETEXT), unicode) }.is_null() {
        // The custom block already belongs to Windows. Only the unpublished Unicode block remains
        // ours to release.
        free_global(unicode);
        // SAFETY: This thread opened the clipboard above.
        unsafe { CloseClipboard() };
        return Err("could not publish graphics clipboard text fallback".into());
    }
    // SAFETY: This thread opened the clipboard above.
    if unsafe { CloseClipboard() } == 0 {
        return Err("could not close the Windows clipboard".into());
    }
    Ok(())
}

/// Reads Lunar Magic's registered single-tile clipboard payload. Allocations larger than 64 bytes
/// are accepted exactly like Lunar Magic 3.63; only the first 64 bytes are returned.
///
/// # Errors
///
/// Returns an error for Win32 failures or a present custom payload shorter than 64 bytes.
pub fn read_graphics_tile_clipboard() -> Result<Option<[u8; 64]>, String> {
    let format = register_graphics_tile_format()?;
    // SAFETY: A null owner is valid for a short synchronous clipboard transaction.
    if unsafe { OpenClipboard(std::ptr::null_mut()) } == 0 {
        return Err("could not open the Windows clipboard".into());
    }
    // SAFETY: The clipboard is open on this thread and `format` is a registered identifier.
    if unsafe { IsClipboardFormatAvailable(format) } == 0 {
        // SAFETY: This thread opened the clipboard above.
        unsafe { CloseClipboard() };
        return Ok(None);
    }
    // SAFETY: The clipboard is open and reports this format available.
    let memory = unsafe { GetClipboardData(format) };
    if memory.is_null() {
        // SAFETY: This thread opened the clipboard above.
        unsafe { CloseClipboard() };
        return Err("could not obtain Lunar Magic graphics clipboard data".into());
    }
    // SAFETY: `memory` is the global-memory handle returned by `GetClipboardData`.
    let size = unsafe { GlobalSize(memory) };
    if size < LUNAR_MAGIC_GRAPHICS_TILE_BYTES {
        // SAFETY: This thread opened the clipboard above.
        unsafe { CloseClipboard() };
        return Err("Lunar Magic graphics clipboard data is shorter than 64 bytes".into());
    }
    // SAFETY: `memory` remains owned by the clipboard and valid while it is open.
    let source = unsafe { GlobalLock(memory) }.cast::<u8>();
    if source.is_null() {
        // SAFETY: This thread opened the clipboard above.
        unsafe { CloseClipboard() };
        return Err("could not lock Lunar Magic graphics clipboard data".into());
    }
    let mut tile = [0; LUNAR_MAGIC_GRAPHICS_TILE_BYTES];
    // SAFETY: `source` is readable for at least 64 bytes by the `GlobalSize` check, and `tile` is
    // writable for exactly 64 non-overlapping bytes.
    unsafe { std::ptr::copy_nonoverlapping(source, tile.as_mut_ptr(), tile.len()) };
    // SAFETY: The handle was successfully locked above.
    unsafe { GlobalUnlock(memory) };
    // SAFETY: This thread opened the clipboard above.
    if unsafe { CloseClipboard() } == 0 {
        return Err("could not close the Windows clipboard".into());
    }
    Ok(Some(tile))
}

fn register_graphics_tile_format() -> Result<u32, String> {
    let name = LUNAR_MAGIC_GRAPHICS_TILE_FORMAT
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: `name` is NUL-terminated and remains alive for the duration of the call.
    let format = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
    (format != 0)
        .then_some(format)
        .ok_or_else(|| "could not register Lunar Magic graphics clipboard format".into())
}

fn allocate_global_copy(bytes: &[u8]) -> Result<*mut core::ffi::c_void, String> {
    // SAFETY: Allocation size comes from a live slice and is nonzero for every caller.
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
    if memory.is_null() {
        return Err("could not allocate Windows clipboard memory".into());
    }
    // SAFETY: `memory` is a newly allocated movable global-memory block.
    let destination = unsafe { GlobalLock(memory) }.cast::<u8>();
    if destination.is_null() {
        free_global(memory);
        return Err("could not lock Windows clipboard memory".into());
    }
    // SAFETY: The allocation is exactly `bytes.len()` bytes and does not overlap the input slice.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len()) };
    // SAFETY: The handle was successfully locked above.
    unsafe { GlobalUnlock(memory) };
    Ok(memory)
}

fn free_global(memory: *mut core::ffi::c_void) {
    if !memory.is_null() {
        // SAFETY: Callers invoke this only while they still own an unpublished allocation.
        unsafe { GlobalFree(memory) };
    }
}

/// Returns the current thread's preferred UI languages in Windows preference order.
///
/// Lunar Magic 3.63 dynamically invokes the same API with flags `0x30` and a
/// 0x600-unit bounded buffer. An empty vector represents an unavailable API or
/// malformed response so the caller can use its portable fallback.
#[must_use]
pub fn preferred_ui_languages() -> Vec<String> {
    let preferred = thread_preferred_ui_languages();
    if preferred.is_empty() {
        user_default_ui_language().into_iter().collect()
    } else {
        preferred
    }
}

fn thread_preferred_ui_languages() -> Vec<String> {
    let mut language_count = 0_u32;
    let mut required_units = 0_u32;
    // SAFETY: This is the documented size-query form: the buffer is null and its
    // capacity is zero, while both output pointers refer to initialized `u32`s.
    let queried = unsafe {
        GetThreadPreferredUILanguages(
            MERGED_USER_AND_SYSTEM_FALLBACKS,
            &raw mut language_count,
            std::ptr::null_mut(),
            &raw mut required_units,
        )
    };
    if queried == 0
        || language_count == 0
        || !(2..=MAX_LANGUAGE_LIST_UTF16_UNITS).contains(&required_units)
    {
        return Vec::new();
    }

    let mut buffer = vec![0_u16; required_units as usize];
    let mut written_units = required_units;
    // SAFETY: `buffer` is writable for `required_units` UTF-16 units, and all
    // output pointers remain valid for the duration of the call.
    let loaded = unsafe {
        GetThreadPreferredUILanguages(
            MERGED_USER_AND_SYSTEM_FALLBACKS,
            &raw mut language_count,
            buffer.as_mut_ptr(),
            &raw mut written_units,
        )
    };
    if loaded == 0 || written_units < 2 || written_units > required_units {
        return Vec::new();
    }
    parse_utf16_multi_string(&buffer[..written_units as usize], language_count)
}

fn user_default_ui_language() -> Option<String> {
    // SAFETY: This parameterless query has no pointer preconditions.
    let language_id = unsafe { GetUserDefaultUILanguage() };
    if language_id == 0 {
        return None;
    }
    let mut buffer = [0_u16; 85];
    // A LANGID is the low word of its corresponding default LCID. This mirrors
    // Lunar Magic's fallback mapping while allowing Windows to supply modern tags.
    // SAFETY: `buffer` is writable for the exact capacity passed to the API.
    let written = unsafe {
        LCIDToLocaleName(
            u32::from(language_id),
            buffer.as_mut_ptr(),
            i32::try_from(buffer.len()).expect("locale-name buffer fits in i32"),
            0,
        )
    };
    let content_units = usize::try_from(written).ok()?.checked_sub(1)?;
    String::from_utf16(buffer.get(..content_units)?).ok()
}

fn parse_utf16_multi_string(buffer: &[u16], expected_count: u32) -> Vec<String> {
    if !buffer.ends_with(&[0, 0]) {
        return Vec::new();
    }
    let languages = buffer[..buffer.len() - 1]
        .split(|unit| *unit == 0)
        .filter(|language| !language.is_empty())
        .map(String::from_utf16)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();
    if languages.len() == expected_count as usize {
        languages
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_utf16_multi_string;

    #[test]
    fn parses_bounded_double_null_terminated_language_list() {
        let buffer = "fr-CA\0en-US\0\0".encode_utf16().collect::<Vec<_>>();
        assert_eq!(parse_utf16_multi_string(&buffer, 2), ["fr-CA", "en-US"]);
    }

    #[test]
    fn rejects_bad_termination_count_and_utf16() {
        assert!(parse_utf16_multi_string(&[u16::from(b'e'), 0], 1).is_empty());
        assert!(parse_utf16_multi_string(&[u16::from(b'e'), 0, 0], 2).is_empty());
        assert!(parse_utf16_multi_string(&[0xd800, 0, 0], 1).is_empty());
    }
}
