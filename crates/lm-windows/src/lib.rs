//! Narrow safe wrappers around Windows APIs needed by the native frontend.

use std::os::windows::{
    ffi::{OsStrExt, OsStringExt},
    io::AsRawHandle,
};
use windows_sys::Win32::Foundation::GlobalFree;
use windows_sys::Win32::Globalization::{
    GetThreadPreferredUILanguages, GetUserDefaultUILanguage, LCIDToLocaleName,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle, GetShortPathNameW,
};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, EnumWindows, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, PostMessageW, SW_RESTORE, SetForegroundWindow, SetWindowTextW, ShowWindow,
};

const MAX_WINDOWS_PATH_UTF16_UNITS: usize = 32_768;
const LUNAR_MAGIC_SETTINGS_KEY: &str = "Software\\LunarianConcepts\\LunarMagic\\Settings";
const MAX_LUNAR_MAGIC_TOOL_UTF16_UNITS: usize = 0x410;
const MAX_LUNAR_MAGIC_TOOL_UTF8_BYTES: usize = 0x40f;

/// Original Lunar Magic 3.63 external-tool values read from its per-user settings key.
///
/// Missing values remain distinct from present empty strings so migration can reproduce the
/// original profile loader's defaults without creating tools for unconfigured profiles.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LunarMagicExternalToolRegistry {
    pub emulator: Option<String>,
    pub emulator_arguments: Option<String>,
    pub gba_emulator: Option<String>,
    pub gba_emulator_arguments: Option<String>,
    pub tile_editor: Option<String>,
    pub tile_editor_arguments: Option<String>,
    pub options: u32,
    pub options2: u32,
}

/// Reads Lunar Magic's three configured external-tool profiles without modifying the registry.
///
/// # Errors
///
/// Rejects wrong registry types, invalid UTF-16, values beyond the original-compatible bounded
/// profile size, and all registry failures other than an absent key/value.
pub fn lunar_magic_external_tool_registry()
-> std::io::Result<Option<LunarMagicExternalToolRegistry>> {
    let fields = [
        read_registry_string("Emulator")?,
        read_registry_string("EmulatorArg")?,
        read_registry_string("Emulator2")?,
        read_registry_string("Emulator2Arg")?,
        read_registry_string("TileEditor")?,
        read_registry_string("TileEditorArg")?,
    ];
    let options = read_registry_dword("Options")?;
    let options2 = read_registry_dword("Options2")?;
    if fields.iter().all(Option::is_none) && options.is_none() && options2.is_none() {
        return Ok(None);
    }
    let [
        emulator,
        emulator_arguments,
        gba_emulator,
        gba_emulator_arguments,
        tile_editor,
        tile_editor_arguments,
    ] = fields;
    Ok(Some(LunarMagicExternalToolRegistry {
        emulator,
        emulator_arguments,
        gba_emulator,
        gba_emulator_arguments,
        tile_editor,
        tile_editor_arguments,
        options: options.unwrap_or_default(),
        options2: options2.unwrap_or_default(),
    }))
}

fn read_registry_string(name: &str) -> std::io::Result<Option<String>> {
    let subkey = wide_nul(LUNAR_MAGIC_SETTINGS_KEY);
    let name = wide_nul(name);
    let mut byte_count = 0_u32;
    let status = unsafe {
        // SAFETY: Both names are retained NUL-terminated UTF-16 strings; the size probe supplies
        // no output allocation and `byte_count` is a live writable scalar.
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut byte_count,
        )
    };
    if status == windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    registry_status(status)?;
    if byte_count % 2 != 0
        || usize::try_from(byte_count).unwrap_or(usize::MAX) > MAX_LUNAR_MAGIC_TOOL_UTF16_UNITS * 2
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Lunar Magic registry string exceeds its bounded UTF-16 shape",
        ));
    }
    let units = usize::try_from(byte_count / 2)
        .map_err(|_| std::io::Error::other("registry string size overflow"))?;
    let mut value = vec![0_u16; units.max(1)];
    let mut second_count = byte_count;
    let status = unsafe {
        // SAFETY: The allocation exposes at least the byte count returned by the size probe and
        // remains live and writable for this non-mutating registry read.
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            value.as_mut_ptr().cast(),
            &mut second_count,
        )
    };
    registry_status(status)?;
    if second_count > byte_count || second_count % 2 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Lunar Magic registry string changed during bounded read",
        ));
    }
    value.truncate(usize::try_from(second_count / 2).unwrap_or_default());
    if value.last() == Some(&0) {
        value.pop();
    }
    if value.contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Lunar Magic registry string contains an interior NUL",
        ));
    }
    let value = String::from_utf16(&value).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid registry UTF-16")
    })?;
    if value.len() > MAX_LUNAR_MAGIC_TOOL_UTF8_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Lunar Magic registry string exceeds its original UTF-8 buffer",
        ));
    }
    Ok(Some(value))
}

fn read_registry_dword(name: &str) -> std::io::Result<Option<u32>> {
    let subkey = wide_nul(LUNAR_MAGIC_SETTINGS_KEY);
    let name = wide_nul(name);
    let mut value = 0_u32;
    let mut byte_count = 4_u32;
    let status = unsafe {
        // SAFETY: Names are retained and NUL-terminated; `value` and `byte_count` are live writable
        // scalars exactly matching the requested REG_DWORD representation.
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut byte_count,
        )
    };
    if status == windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    registry_status(status)?;
    if byte_count != 4 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Lunar Magic registry DWORD has the wrong size",
        ));
    }
    Ok(Some(value))
}

fn registry_status(status: u32) -> std::io::Result<()> {
    if status == windows_sys::Win32::Foundation::ERROR_SUCCESS {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(
            i32::try_from(status).unwrap_or(i32::MAX),
        ))
    }
}

fn wide_nul(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Resolves an existing Windows path to its filesystem-provided 8.3 short form.
///
/// # Errors
///
/// Returns the last Windows error when the path does not exist, the volume has no short-name
/// mapping, or the API rejects it; oversized results are rejected without an unbounded retry.
pub fn short_path(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let source = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut output = vec![0_u16; MAX_WINDOWS_PATH_UTF16_UNITS];
    let written = unsafe {
        // SAFETY: Both pointers reference initialized allocations for the duration of the call;
        // `source` is NUL-terminated and `output` exposes its complete writable capacity.
        GetShortPathNameW(
            source.as_ptr(),
            output.as_mut_ptr(),
            u32::try_from(output.len()).expect("Windows path bound fits u32"),
        )
    };
    if written == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let written =
        usize::try_from(written).map_err(|_| std::io::Error::other("short path overflow"))?;
    if written >= output.len() {
        return Err(std::io::Error::other(
            "short path exceeds the Windows path bound",
        ));
    }
    output.truncate(written);
    Ok(std::path::PathBuf::from(std::ffi::OsString::from_wide(
        &output,
    )))
}

/// Opens a file, directory, URL, or other shell-associated target without retaining a process.
///
/// This matches the ownership boundary of `ShellExecuteW`: success does not imply that a child
/// process was created, and no process handle is returned to the caller.
pub fn shell_open(
    target: &std::ffi::OsStr,
    parameters: Option<&std::ffi::OsStr>,
    directory: Option<&std::path::Path>,
) -> std::io::Result<()> {
    fn wide_nul(value: &std::ffi::OsStr) -> std::io::Result<Vec<u16>> {
        let mut value = value.encode_wide().collect::<Vec<_>>();
        if value.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "ShellExecute text contains an interior NUL",
            ));
        }
        value.push(0);
        Ok(value)
    }

    let target = wide_nul(target)?;
    let parameters = parameters.map(wide_nul).transpose()?;
    let directory = directory
        .map(|path| wide_nul(path.as_os_str()))
        .transpose()?;
    let result = unsafe {
        // SAFETY: Every supplied pointer is either null or references a NUL-terminated UTF-16
        // allocation retained across the call. A null owner and show mode 1 are valid inputs.
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            target.as_ptr(),
            parameters
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            directory
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            1,
        )
    };
    let code = result as isize;
    if code <= 32 {
        Err(std::io::Error::from_raw_os_error(code as i32))
    } else {
        Ok(())
    }
}

/// Hidden top-level window whose caption can carry the current ROM path to notified tools.
pub struct MessageTextWindow {
    handle: windows_sys::Win32::Foundation::HWND,
}

impl MessageTextWindow {
    /// Creates the hidden top-level window with `text` as its cross-process-readable caption.
    pub fn new(text: &std::ffi::OsStr) -> std::io::Result<Self> {
        let class = "STATIC\0".encode_utf16().collect::<Vec<_>>();
        let text = nul_terminated_wide(text)?;
        let handle = unsafe {
            // SAFETY: Class and caption are valid NUL-terminated strings. All optional handles and
            // creation data are null. A null parent creates the hidden top-level caption window
            // required for cross-process GetWindowText rather than a child/message-only control.
            CreateWindowExW(
                0,
                class.as_ptr(),
                text.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if handle.is_null() {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(Self { handle })
        }
    }

    /// Replaces the caption while retaining the stable HWND supplied in notification `wParam`.
    pub fn set_text(&mut self, text: &std::ffi::OsStr) -> std::io::Result<()> {
        let text = nul_terminated_wide(text)?;
        let succeeded = unsafe {
            // SAFETY: `self.handle` remains owned and live; `text` is NUL-terminated for the call.
            SetWindowTextW(self.handle, text.as_ptr())
        };
        if succeeded == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    #[must_use]
    pub fn raw_handle(&self) -> isize {
        self.handle as isize
    }
}

impl Drop for MessageTextWindow {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: This object uniquely owns the hidden HWND and destroys it at most once.
            DestroyWindow(self.handle);
        }
    }
}

/// Posts one Lunar Magic-compatible message to every top-level window owned by `process_id`.
pub fn post_message_to_process_windows(
    process_id: u32,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> std::io::Result<usize> {
    struct Context {
        process_id: u32,
        message: u32,
        wparam: usize,
        lparam: isize,
        posted: usize,
    }

    unsafe extern "system" fn callback(
        window: windows_sys::Win32::Foundation::HWND,
        raw: isize,
    ) -> i32 {
        let context = unsafe { &mut *(raw as *mut Context) };
        let mut owner = 0_u32;
        unsafe { GetWindowThreadProcessId(window, &mut owner) };
        if owner == context.process_id
            && unsafe { PostMessageW(window, context.message, context.wparam, context.lparam) } != 0
        {
            context.posted += 1;
        }
        1
    }

    let mut context = Context {
        process_id,
        message,
        wparam,
        lparam,
        posted: 0,
    };
    let succeeded = unsafe {
        // SAFETY: `context` remains live and exclusively borrowed throughout synchronous
        // enumeration; the callback always returns TRUE and validates window ownership.
        EnumWindows(Some(callback), (&mut context as *mut Context) as isize)
    };
    if succeeded == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(context.posted)
    }
}

/// Restores and activates the first visible top-level window owned by `process_id`.
///
/// Lunar Magic uses this boundary when a user-toolbar button is configured for its default
/// single-instance policy and the button's process is already running. A process can temporarily
/// have no eligible window while starting; that is reported as `Ok(false)` rather than an error.
pub fn focus_process_window(process_id: u32) -> std::io::Result<bool> {
    struct Context {
        process_id: u32,
        found: bool,
    }

    unsafe extern "system" fn callback(
        window: windows_sys::Win32::Foundation::HWND,
        raw: isize,
    ) -> i32 {
        let context = unsafe { &mut *(raw as *mut Context) };
        let mut owner = 0_u32;
        unsafe { GetWindowThreadProcessId(window, &mut owner) };
        if owner != context.process_id || unsafe { IsWindowVisible(window) } == 0 {
            return 1;
        }
        if unsafe { IsIconic(window) } != 0 {
            unsafe { ShowWindow(window, SW_RESTORE) };
        }
        // Windows may reject foreground activation under its user-input policy. Finding and
        // attempting the eligible process window still completes Lunar Magic's best-effort action.
        let _activated = unsafe { SetForegroundWindow(window) };
        context.found = true;
        0
    }

    let mut context = Context {
        process_id,
        found: false,
    };
    let succeeded = unsafe {
        // SAFETY: `context` remains live and exclusively borrowed throughout synchronous
        // enumeration. Returning FALSE intentionally stops once the first eligible window is found.
        EnumWindows(Some(callback), (&mut context as *mut Context) as isize)
    };
    if succeeded == 0 && !context.found {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(context.found)
    }
}

fn nul_terminated_wide(value: &std::ffi::OsStr) -> std::io::Result<Vec<u16>> {
    let mut value = value.encode_wide().collect::<Vec<_>>();
    if value.contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "window text contains an interior NUL",
        ));
    }
    value.push(0);
    Ok(value)
}
use windows_sys::Win32::System::{
    DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    },
    Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
    Ole::CF_UNICODETEXT,
};

/// Stable identity of one Windows filesystem object while it exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileIdentity {
    volume_serial_number: u32,
    file_index: u64,
}

/// Reads the volume serial and 64-bit file index from an open file handle.
///
/// This is the stable Win32 equivalent of Rust's currently unstable Windows
/// `MetadataExt::{volume_serial_number, file_index}` methods.
///
/// # Errors
///
/// Returns the last Windows error if the handle information cannot be queried.
pub fn file_identity(file: &std::fs::File) -> std::io::Result<FileIdentity> {
    let mut information = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    // SAFETY: `file` owns a live kernel handle for the duration of the call and `information`
    // points to writable storage of the exact structure requested by the API.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), information.as_mut_ptr()) }
        == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: A nonzero return initializes the complete output structure.
    let information = unsafe { information.assume_init() };
    Ok(FileIdentity {
        volume_serial_number: information.dwVolumeSerialNumber,
        file_index: u64::from(information.nFileIndexHigh) << 32
            | u64::from(information.nFileIndexLow),
    })
}

const MERGED_USER_AND_SYSTEM_FALLBACKS: u32 = 0x30;
const MAX_LANGUAGE_LIST_UTF16_UNITS: u32 = 0x600;
const LUNAR_MAGIC_GRAPHICS_TILE_FORMAT: &str = "Lunar Magic 8x8 Tile";
const LUNAR_MAGIC_GRAPHICS_TILE_BYTES: usize = 64;
const LUNAR_MAGIC_MAP16_TILE_FORMAT: &str = "Lunar Magic 16x16 Tile";
const LUNAR_MAGIC_MAP16_TILE_BYTES: usize = 10;
const LUNAR_MAGIC_COLOR_V2_FORMAT: &str = "Lunar Magic Color V2";
const LUNAR_MAGIC_COLOR_V2_BYTES: usize = 12;
const LUNAR_MAGIC_COLOR_ROW_V2_FORMAT: &str = "Lunar Magic Color Row V2";
const LUNAR_MAGIC_COLOR_ROW_V2_BYTES: usize = 132;

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
    write_registered_clipboard(LUNAR_MAGIC_GRAPHICS_TILE_FORMAT, tile, fallback_text)
}

/// Publishes Lunar Magic's native ten-byte single-Map16-tile record and a Unicode text fallback.
///
/// # Errors
///
/// Returns an error when the tile is not exactly ten bytes or a Win32 clipboard operation fails.
pub fn write_map16_tile_clipboard(tile: &[u8], fallback_text: &str) -> Result<(), String> {
    if tile.len() != LUNAR_MAGIC_MAP16_TILE_BYTES {
        return Err("Lunar Magic Map16 clipboard tile must contain exactly 10 bytes".into());
    }
    write_registered_clipboard(LUNAR_MAGIC_MAP16_TILE_FORMAT, tile, fallback_text)
}

/// Publishes Lunar Magic's exact 12-byte Color V2 record and a Unicode typed-text fallback.
pub fn write_palette_color_clipboard(color: &[u8], fallback_text: &str) -> Result<(), String> {
    if color.len() != LUNAR_MAGIC_COLOR_V2_BYTES {
        return Err("Lunar Magic Color V2 data must contain exactly 12 bytes".into());
    }
    write_registered_clipboard(LUNAR_MAGIC_COLOR_V2_FORMAT, color, fallback_text)
}

/// Publishes Lunar Magic's exact 132-byte Color Row V2 record and a Unicode typed-text fallback.
pub fn write_palette_row_clipboard(row: &[u8], fallback_text: &str) -> Result<(), String> {
    if row.len() != LUNAR_MAGIC_COLOR_ROW_V2_BYTES {
        return Err("Lunar Magic Color Row V2 data must contain exactly 132 bytes".into());
    }
    write_registered_clipboard(LUNAR_MAGIC_COLOR_ROW_V2_FORMAT, row, fallback_text)
}

fn write_registered_clipboard(
    format_name: &str,
    bytes: &[u8],
    fallback_text: &str,
) -> Result<(), String> {
    let format = register_clipboard_format(format_name)?;
    let fallback = fallback_text
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_ne_bytes)
        .collect::<Vec<_>>();
    let custom = allocate_global_copy(bytes)?;
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
    read_registered_clipboard(
        LUNAR_MAGIC_GRAPHICS_TILE_FORMAT,
        LUNAR_MAGIC_GRAPHICS_TILE_BYTES,
    )
    .map(|bytes| bytes.map(|bytes| bytes.try_into().expect("requested exactly 64 bytes")))
}

/// Reads Lunar Magic's registered single-Map16-tile clipboard payload, accepting larger
/// allocations and consuming only the first ten bytes.
///
/// # Errors
///
/// Returns an error for Win32 failures or a present custom payload shorter than ten bytes.
pub fn read_map16_tile_clipboard() -> Result<Option<[u8; 10]>, String> {
    read_registered_clipboard(LUNAR_MAGIC_MAP16_TILE_FORMAT, LUNAR_MAGIC_MAP16_TILE_BYTES)
        .map(|bytes| bytes.map(|bytes| bytes.try_into().expect("requested exactly 10 bytes")))
}

/// Reads the preferred Color V2 payload, accepting larger allocations like Lunar Magic 3.63.
pub fn read_palette_color_clipboard() -> Result<Option<[u8; 12]>, String> {
    read_registered_clipboard(LUNAR_MAGIC_COLOR_V2_FORMAT, LUNAR_MAGIC_COLOR_V2_BYTES)
        .map(|bytes| bytes.map(|bytes| bytes.try_into().expect("requested exactly 12 bytes")))
}

/// Reads the preferred Color Row V2 payload, accepting larger allocations like Lunar Magic 3.63.
pub fn read_palette_row_clipboard() -> Result<Option<[u8; 132]>, String> {
    read_registered_clipboard(
        LUNAR_MAGIC_COLOR_ROW_V2_FORMAT,
        LUNAR_MAGIC_COLOR_ROW_V2_BYTES,
    )
    .map(|bytes| bytes.map(|bytes| bytes.try_into().expect("requested exactly 132 bytes")))
}

fn read_registered_clipboard(
    format_name: &str,
    minimum_bytes: usize,
) -> Result<Option<Vec<u8>>, String> {
    let format = register_clipboard_format(format_name)?;
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
    if size < minimum_bytes {
        // SAFETY: This thread opened the clipboard above.
        unsafe { CloseClipboard() };
        return Err(format!(
            "Lunar Magic clipboard data is shorter than {minimum_bytes} bytes"
        ));
    }
    // SAFETY: `memory` remains owned by the clipboard and valid while it is open.
    let source = unsafe { GlobalLock(memory) }.cast::<u8>();
    if source.is_null() {
        // SAFETY: This thread opened the clipboard above.
        unsafe { CloseClipboard() };
        return Err("could not lock Lunar Magic graphics clipboard data".into());
    }
    let mut bytes = vec![0; minimum_bytes];
    // SAFETY: `source` is readable for at least `minimum_bytes` by the `GlobalSize` check, and the
    // destination is writable for exactly that many non-overlapping bytes.
    unsafe { std::ptr::copy_nonoverlapping(source, bytes.as_mut_ptr(), bytes.len()) };
    // SAFETY: The handle was successfully locked above.
    unsafe { GlobalUnlock(memory) };
    // SAFETY: This thread opened the clipboard above.
    if unsafe { CloseClipboard() } == 0 {
        return Err("could not close the Windows clipboard".into());
    }
    Ok(Some(bytes))
}

fn register_clipboard_format(format_name: &str) -> Result<u32, String> {
    let name = format_name
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
    use super::{LunarMagicExternalToolRegistry, parse_utf16_multi_string};

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

    /// Opt-in Wine/Windows registry oracle. The runner seeds the original Lunar Magic key in an
    /// isolated user hive before executing this exact test.
    #[test]
    #[ignore = "requires an isolated seeded Windows/Wine registry"]
    fn reads_seeded_lunar_magic_external_tool_registry_exactly() {
        assert_eq!(
            super::lunar_magic_external_tool_registry().unwrap(),
            Some(LunarMagicExternalToolRegistry {
                emulator: Some(r"C:\Emulators\Snes 日本語.exe".into()),
                emulator_arguments: Some(r#"--fullscreen "%1""#.into()),
                gba_emulator: Some(r"C:\Emulators\mGBA.exe".into()),
                gba_emulator_arguments: Some(r#"--gba "%1""#.into()),
                tile_editor: Some(r"C:\Tools\YY-CHR.exe".into()),
                tile_editor_arguments: Some(r#"--palette keep "%1""#.into()),
                options: 0x2000_0000,
                options2: 0x0107_0000,
            })
        );
    }
}
