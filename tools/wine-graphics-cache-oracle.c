#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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

static int right_paste_and_changed(
    HWND window,
    HANDLE process,
    uintptr_t planar_address,
    unsigned x,
    unsigned y,
    int *matches_source,
    const uint8_t source[0x20]
) {
    uint8_t before[0x20] = {0};
    uint8_t after[0x20] = {0};
    if (!read_exact(process, planar_address, before, sizeof(before))) {
        return -1;
    }
    SendMessageA(window, WM_RBUTTONDOWN, MK_RBUTTON, MAKELPARAM(x, y));
    SendMessageA(window, WM_RBUTTONUP, 0, MAKELPARAM(x, y));
    Sleep(25);
    if (!read_exact(process, planar_address, after, sizeof(after))) {
        return -1;
    }
    if (matches_source != NULL) {
        *matches_source = memcmp(after, source, sizeof(after)) == 0;
    }
    return memcmp(before, after, sizeof(after)) != 0;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: wine-graphics-cache-oracle.exe WINDOWS_PROCESS_ID\n");
        return 2;
    }
    char *end = NULL;
    unsigned long parsed = strtoul(argv[1], &end, 0);
    if (end == argv[1] || *end != '\0' || parsed == 0 || parsed > UINT32_MAX) {
        fprintf(stderr, "invalid process id: %s\n", argv[1]);
        return 2;
    }
    DWORD process_id = (DWORD)parsed;
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

    uint32_t original_page = 0;
    uint32_t maximum_page = 0;
    uint8_t bypass = 0;
    uint8_t vanilla_animation = 0;
    uint8_t special_world = 0;
    uint8_t source[0x20] = {0};
    if (!read_exact(process, 0x00e27b80, &original_page, sizeof(original_page)) ||
        !read_exact(process, 0x005e54f0, &maximum_page, sizeof(maximum_page)) ||
        !read_exact(process, 0x00e27888, &bypass, sizeof(bypass)) ||
        !read_exact(process, 0x00600b86, &vanilla_animation, sizeof(vanilla_animation)) ||
        !read_exact(process, 0x00e278df, &special_world, sizeof(special_world)) ||
        !read_exact(process, 0x0086b7e8, source, sizeof(source))) {
        fprintf(stderr, "cannot read graphics editor state\n");
        CloseHandle(process);
        return 1;
    }

    if (!write_u32(process, 0x00e27b80, 0)) {
        fprintf(stderr, "cannot select graphics page zero\n");
        CloseHandle(process);
        return 1;
    }
    SendMessageA(search.window, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(8, 8));
    SendMessageA(search.window, WM_LBUTTONUP, 0, MAKELPARAM(8, 8));
    Sleep(25);

    int ordinary_matches = 0;
    int ordinary_changed = right_paste_and_changed(
        search.window, process, 0x0086b7e8 + 0x02 * 0x20, 40, 8, &ordinary_matches, source
    );
    int fixed_changed = right_paste_and_changed(
        search.window, process, 0x0086b7e8 + 0x41 * 0x20, 24, 72, NULL, source
    );

    write_u32(process, 0x00e27b80, 3);
    int unused_changed = right_paste_and_changed(
        search.window, process, 0x0086b7e8 + 0x300 * 0x20, 8, 8, NULL, source
    );

    write_u32(process, 0x00e27b80, 5);
    int last_matches = 0;
    int last_changed = right_paste_and_changed(
        search.window, process, 0x0086b7e8 + 0x5ff * 0x20, 248, 248, &last_matches, source
    );

    write_u32(process, 0x00e27b80, 6);
    int beyond_changed = right_paste_and_changed(
        search.window, process, 0x0086b7e8 + 0x600 * 0x20, 8, 8, NULL, source
    );
    write_u32(process, 0x00e27b80, original_page);

    CloseHandle(process);
    if (ordinary_changed < 0 || fixed_changed < 0 || unused_changed < 0 ||
        last_changed < 0 || beyond_changed < 0) {
        fprintf(stderr, "cannot capture graphics paste transition\n");
        return 1;
    }

    printf("field\tvalue\n");
    printf("maximum_page\t%02lX\n", (unsigned long)maximum_page);
    printf("super_gfx_bypass\t%u\n", bypass);
    printf("vanilla_animation_enabled\t%u\n", vanilla_animation);
    printf("special_world_passed\t%u\n", special_world);
    printf("ordinary_target_changed\t%d\n", ordinary_changed);
    printf("ordinary_target_matches_source\t%d\n", ordinary_matches);
    printf("fixed_animation_target_changed\t%d\n", fixed_changed);
    printf("unused_fg_target_changed\t%d\n", unused_changed);
    printf("last_editable_target_changed\t%d\n", last_changed);
    printf("last_editable_target_matches_source\t%d\n", last_matches);
    printf("beyond_limit_target_changed\t%d\n", beyond_changed);
    return 0;
}
