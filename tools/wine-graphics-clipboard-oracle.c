#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <tlhelp32.h>

struct window_search {
    DWORD process_id;
    HWND window;
};

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

static BOOL CALLBACK find_graphics_window(HWND window, LPARAM opaque) {
    struct window_search *search = (struct window_search *)opaque;
    DWORD process_id = 0;
    char class_name[128] = {0};
    GetWindowThreadProcessId(window, &process_id);
    GetClassNameA(window, class_name, sizeof(class_name));
    if (
        process_id == search->process_id &&
        _stricmp(class_name, "Window8x8") == 0
    ) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static HWND graphics_window(DWORD process_id) {
    struct window_search search = {.process_id = process_id, .window = NULL};
    EnumWindows(find_graphics_window, (LPARAM)&search);
    return search.window;
}

static int invoke_original(DWORD process_id, uintptr_t address) {
    HANDLE process = OpenProcess(
        PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION |
            PROCESS_VM_READ | PROCESS_VM_WRITE,
        FALSE,
        process_id
    );
    if (process == NULL) {
        return 1;
    }
    HANDLE thread = CreateRemoteThread(
        process,
        NULL,
        0,
        (LPTHREAD_START_ROUTINE)address,
        NULL,
        0,
        NULL
    );
    if (thread == NULL) {
        CloseHandle(process);
        return 1;
    }
    DWORD waited = WaitForSingleObject(thread, 5000);
    CloseHandle(thread);
    CloseHandle(process);
    return waited == WAIT_OBJECT_0 ? 0 : 1;
}

static UINT graphics_format(void) {
    return RegisterClipboardFormatA("Lunar Magic 8x8 Tile");
}

static int dump_clipboard(void) {
    UINT format = graphics_format();
    if (format == 0 || !IsClipboardFormatAvailable(format) || !OpenClipboard(NULL)) {
        return 1;
    }
    HANDLE memory = GetClipboardData(format);
    SIZE_T size = memory == NULL ? 0 : GlobalSize(memory);
    const unsigned char *bytes = memory == NULL ? NULL : GlobalLock(memory);
    if (bytes == NULL) {
        CloseClipboard();
        return 1;
    }
    printf("format=Lunar Magic 8x8 Tile\nsize=%lu\nbytes=", (unsigned long)size);
    for (SIZE_T index = 0; index < size; index++) {
        printf("%02X", bytes[index]);
    }
    putchar('\n');
    GlobalUnlock(memory);
    CloseClipboard();
    return 0;
}

static int publish_clipboard(const unsigned char bytes[64]) {
    UINT format = graphics_format();
    HGLOBAL memory = GlobalAlloc(GMEM_MOVEABLE, 64);
    void *destination = memory == NULL ? NULL : GlobalLock(memory);
    if (format == 0 || destination == NULL) {
        if (memory != NULL) {
            GlobalFree(memory);
        }
        return 1;
    }
    memcpy(destination, bytes, 64);
    GlobalUnlock(memory);
    if (!OpenClipboard(NULL)) {
        GlobalFree(memory);
        return 1;
    }
    BOOL published = EmptyClipboard() && SetClipboardData(format, memory) != NULL;
    CloseClipboard();
    if (!published) {
        GlobalFree(memory);
    }
    return published ? 0 : 1;
}

int main(int argc, char **argv) {
    if (
        argc != 3 ||
        (strcmp(argv[2], "copy") != 0 && strcmp(argv[2], "roundtrip") != 0)
    ) {
        fprintf(stderr, "usage: wine-graphics-clipboard-oracle.exe PROCESS copy|roundtrip\n");
        return 2;
    }
    DWORD process_id = find_process(argv[1]);
    if (graphics_window(process_id) == NULL) {
        fprintf(stderr, "Window8x8 not found\n");
        return 1;
    }
    if (strcmp(argv[2], "roundtrip") == 0) {
        unsigned char expected[64];
        for (size_t index = 0; index < sizeof(expected); index++) {
            expected[index] = (unsigned char)(index & 0x0f);
        }
        if (
            publish_clipboard(expected) != 0 ||
            invoke_original(process_id, 0x005052b0) != 0
        ) {
            return 1;
        }
    }
    if (invoke_original(process_id, 0x005051e0) != 0) {
        return 1;
    }
    return dump_clipboard();
}
