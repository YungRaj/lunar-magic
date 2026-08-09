#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tlhelp32.h>

struct window_search {
    DWORD process_id;
    HWND window;
};

static BOOL CALLBACK find_graphics_window(HWND window, LPARAM opaque) {
    struct window_search *search = (struct window_search *)opaque;
    DWORD process_id = 0;
    char class_name[64] = {0};
    GetWindowThreadProcessId(window, &process_id);
    GetClassNameA(window, class_name, sizeof(class_name));
    if (process_id == search->process_id && _stricmp(class_name, "Window8x8") == 0) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static int read_exact(HANDLE process, uintptr_t address, void *bytes, size_t length) {
    SIZE_T read = 0;
    return ReadProcessMemory(process, (void *)address, bytes, length, &read) && read == length;
}

static int write_u32(HANDLE process, uintptr_t address, uint32_t value) {
    SIZE_T written = 0;
    return WriteProcessMemory(process, (void *)address, &value, sizeof(value), &written) &&
           written == sizeof(value);
}

static DWORD find_process(const char *name) {
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    PROCESSENTRY32 entry = {.dwSize = sizeof(entry)};
    DWORD process_id = 0;
    if (snapshot != INVALID_HANDLE_VALUE && Process32First(snapshot, &entry)) {
        do {
            if (_stricmp(entry.szExeFile, name) == 0) {
                process_id = entry.th32ProcessID;
                break;
            }
        } while (Process32Next(snapshot, &entry));
    }
    if (snapshot != INVALID_HANDLE_VALUE) {
        CloseHandle(snapshot);
    }
    return process_id;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: wine-graphics-pixel-oracle.exe WINDOWS_PROCESS_ID|PROCESS_NAME\n");
        return 2;
    }
    char *end = NULL;
    unsigned long parsed = strtoul(argv[1], &end, 0);
    DWORD process_id = 0;
    if (end != argv[1] && *end == '\0' && parsed > 0 && parsed <= UINT32_MAX) {
        process_id = (DWORD)parsed;
    } else {
        process_id = find_process(argv[1]);
    }
    if (process_id == 0) {
        fprintf(stderr, "process not found: %s\n", argv[1]);
        return 2;
    }
    struct window_search search = {.process_id = process_id, .window = NULL};
    EnumWindows(find_graphics_window, (LPARAM)&search);
    if (search.window == NULL) {
        fprintf(stderr, "Window8x8 not found for process %lu\n", parsed);
        return 1;
    }
    HANDLE process = OpenProcess(
        PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE,
        FALSE,
        process_id
    );
    if (process == NULL) {
        fprintf(stderr, "cannot open process %lu\n", parsed);
        return 1;
    }

    const uint32_t tile = 0x600;
    const uintptr_t decoded_address = 0x006204b0 + tile * 0x40;
    const uintptr_t planar_address = 0x0086b7e8 + tile * 0x20;
    const uintptr_t edit_buffer_address = 0x00acf908;
    uint32_t original_page = 0;
    uint32_t maximum_page = 0;
    uint32_t editor_x = 0;
    uint32_t editor_y = 0;
    uint32_t editor_width = 0;
    uint32_t editor_height = 0;
    uint32_t foreground = 0;
    uint32_t background = 0;
    uint8_t decoded_before[0x40] = {0};
    uint8_t decoded_painted[0x40] = {0};
    uint8_t decoded_restored[0x40] = {0};
    uint8_t edit_before[0x40] = {0};
    uint8_t edit_flipped[0x40] = {0};
    uint8_t edit_unflipped[0x40] = {0};
    uint8_t edit_painted[0x40] = {0};
    uint8_t edit_restored[0x40] = {0};
    uint8_t planar_before[0x20] = {0};
    uint8_t planar_flipped[0x20] = {0};
    uint8_t planar_unflipped[0x20] = {0};
    uint8_t planar_painted[0x20] = {0};
    uint8_t planar_restored[0x20] = {0};
    int ok =
        read_exact(process, 0x00e27b80, &original_page, sizeof(original_page)) &&
        read_exact(process, 0x005e54f0, &maximum_page, sizeof(maximum_page)) &&
        read_exact(process, 0x005ec264, &editor_x, sizeof(editor_x)) &&
        read_exact(process, 0x005ec268, &editor_y, sizeof(editor_y)) &&
        read_exact(process, 0x005e54e4, &editor_width, sizeof(editor_width)) &&
        read_exact(process, 0x005e54e8, &editor_height, sizeof(editor_height)) &&
        read_exact(process, 0x005e54f4, &foreground, sizeof(foreground)) &&
        read_exact(process, 0x00e27b84, &background, sizeof(background)) &&
        read_exact(process, decoded_address, decoded_before, sizeof(decoded_before)) &&
        read_exact(process, planar_address, planar_before, sizeof(planar_before));
    if (!ok || maximum_page < 6 || editor_width == 0 || editor_height == 0) {
        fprintf(stderr, "cannot read unlocked graphics pixel-editor state\n");
        CloseHandle(process);
        return 1;
    }

    write_u32(process, 0x00e27b80, 6);
    SendMessageA(search.window, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(8, 8));
    SendMessageA(search.window, WM_LBUTTONUP, 0, MAKELPARAM(8, 8));
    Sleep(25);
    if (!read_exact(process, edit_buffer_address, edit_before, sizeof(edit_before))) {
        fprintf(stderr, "cannot read selected graphics edit buffer\n");
        write_u32(process, 0x00e27b80, original_page);
        CloseHandle(process);
        return 1;
    }

    SendMessageA(search.window, WM_CHAR, 'x', 0);
    Sleep(25);
    read_exact(process, edit_buffer_address, edit_flipped, sizeof(edit_flipped));
    read_exact(process, planar_address, planar_flipped, sizeof(planar_flipped));
    SendMessageA(search.window, WM_CHAR, 'x', 0);
    Sleep(25);
    read_exact(process, edit_buffer_address, edit_unflipped, sizeof(edit_unflipped));
    read_exact(process, planar_address, planar_unflipped, sizeof(planar_unflipped));

    unsigned pixel_x = editor_x + editor_width / 16;
    unsigned pixel_y = editor_y + editor_height / 16;
    SendMessageA(
        search.window,
        WM_LBUTTONDOWN,
        MK_LBUTTON,
        MAKELPARAM(pixel_x, pixel_y)
    );
    SendMessageA(search.window, WM_LBUTTONUP, 0, MAKELPARAM(pixel_x, pixel_y));
    Sleep(25);
    read_exact(process, edit_buffer_address, edit_painted, sizeof(edit_painted));
    read_exact(process, decoded_address, decoded_painted, sizeof(decoded_painted));
    read_exact(process, planar_address, planar_painted, sizeof(planar_painted));

    SendMessageA(
        search.window,
        WM_RBUTTONDOWN,
        MK_RBUTTON,
        MAKELPARAM(pixel_x, pixel_y)
    );
    SendMessageA(search.window, WM_RBUTTONUP, 0, MAKELPARAM(pixel_x, pixel_y));
    Sleep(25);
    read_exact(process, edit_buffer_address, edit_restored, sizeof(edit_restored));
    read_exact(process, decoded_address, decoded_restored, sizeof(decoded_restored));
    read_exact(process, planar_address, planar_restored, sizeof(planar_restored));
    write_u32(process, 0x00e27b80, original_page);
    CloseHandle(process);

    printf("field\tvalue\n");
    printf("tile\t%03X\n", tile);
    printf("maximum_page\t%02X\n", maximum_page);
    printf("foreground_color\t%X\n", foreground);
    printf("background_color\t%X\n", background);
    printf("initial_edit_pixel_zero\t%X\n", edit_before[0]);
    printf("flip_changed_edit_buffer\t%d\n", memcmp(edit_before, edit_flipped, 0x40) != 0);
    printf("flip_changed_planar_backing\t%d\n", memcmp(planar_before, planar_flipped, 0x20) != 0);
    printf("second_flip_restored_edit_buffer\t%d\n", memcmp(edit_before, edit_unflipped, 0x40) == 0);
    printf("second_flip_restored_planar_backing\t%d\n", memcmp(planar_before, planar_unflipped, 0x20) == 0);
    printf("foreground_paint_changed_edit_buffer\t%d\n", memcmp(edit_before, edit_painted, 0x40) != 0);
    printf("foreground_paint_changed_decoded_backing\t%d\n", memcmp(decoded_before, decoded_painted, 0x40) != 0);
    printf("foreground_paint_changed_planar_backing\t%d\n", memcmp(planar_before, planar_painted, 0x20) != 0);
    printf("painted_edit_pixel_zero\t%X\n", edit_painted[0]);
    printf("backing_pixel_zero\t%X\n", decoded_painted[0]);
    printf("background_paint_restored_edit_buffer\t%d\n", memcmp(edit_before, edit_restored, 0x40) == 0);
    printf("background_paint_restored_decoded\t%d\n", memcmp(decoded_before, decoded_restored, 0x40) == 0);
    printf("background_paint_restored_planar\t%d\n", memcmp(planar_before, planar_restored, 0x20) == 0);
    return 0;
}
