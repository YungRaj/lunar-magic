#include <windows.h>
#include <tlhelp32.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

struct window_search {
    DWORD process_id;
    const char *class_name;
    HWND window;
};

static BOOL CALLBACK find_window(HWND window, LPARAM parameter) {
    struct window_search *search = (struct window_search *)parameter;
    DWORD process_id = 0;
    char class_name[128] = {0};
    GetWindowThreadProcessId(window, &process_id);
    GetClassNameA(window, class_name, sizeof(class_name));
    if (process_id == search->process_id && strcmp(class_name, search->class_name) == 0) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static DWORD find_process(const char *executable) {
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    PROCESSENTRY32 entry = {.dwSize = sizeof(entry)};
    if (snapshot == INVALID_HANDLE_VALUE || !Process32First(snapshot, &entry)) {
        if (snapshot != INVALID_HANDLE_VALUE) CloseHandle(snapshot);
        return 0;
    }
    do {
        if (_stricmp(entry.szExeFile, executable) == 0) {
            DWORD process_id = entry.th32ProcessID;
            CloseHandle(snapshot);
            return process_id;
        }
    } while (Process32Next(snapshot, &entry));
    CloseHandle(snapshot);
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 4) {
        fprintf(stderr, "usage: wine-window-key.exe EXECUTABLE WINDOW_CLASS|@HWND_ADDRESS VIRTUAL_KEY\n");
        return 2;
    }
    char *end = NULL;
    unsigned long virtual_key = strtoul(argv[3], &end, 0);
    if (end == argv[3] || *end != '\0' || virtual_key > 0xff) {
        fprintf(stderr, "invalid virtual key: %s\n", argv[3]);
        return 2;
    }
    struct window_search search = {find_process(argv[1]), argv[2], NULL};
    if (search.process_id == 0) {
        fprintf(stderr, "process not found: %s\n", argv[1]);
        return 1;
    }
    if (argv[2][0] == '@') {
        char *address_end = NULL;
        unsigned long address = strtoul(argv[2] + 1, &address_end, 0);
        HANDLE process = OpenProcess(PROCESS_VM_READ, FALSE, search.process_id);
        SIZE_T bytes_read = 0;
        BOOL read_ok = address_end != argv[2] + 1 && *address_end == '\0' && process != NULL &&
            ReadProcessMemory(process, (void *)(uintptr_t)address, &search.window,
                              sizeof(search.window), &bytes_read);
        if (process != NULL) CloseHandle(process);
        if (!read_ok || bytes_read != sizeof(search.window) || !IsWindow(search.window)) {
            fprintf(stderr, "cannot resolve target HWND at %s\n", argv[2] + 1);
            return 1;
        }
    } else {
        EnumWindows(find_window, (LPARAM)&search);
    }
    if (search.window == NULL) {
        fprintf(stderr, "window class not found: %s\n", argv[2]);
        return 1;
    }
    if (!PostMessage(search.window, WM_KEYDOWN, virtual_key, 0) ||
        !PostMessage(search.window, WM_KEYUP, virtual_key, 0xc0000000)) {
        fprintf(stderr, "cannot post virtual key 0x%02lx\n", virtual_key);
        return 1;
    }
    return 0;
}
