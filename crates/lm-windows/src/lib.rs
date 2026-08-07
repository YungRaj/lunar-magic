//! Narrow safe wrappers around Windows APIs needed by the native frontend.

use windows_sys::Win32::Globalization::{
    GetThreadPreferredUILanguages, GetUserDefaultUILanguage, LCIDToLocaleName,
};

const MERGED_USER_AND_SYSTEM_FALLBACKS: u32 = 0x30;
const MAX_LANGUAGE_LIST_UTF16_UNITS: u32 = 0x600;

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
