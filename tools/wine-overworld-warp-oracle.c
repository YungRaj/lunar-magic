#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <commctrl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tlhelp32.h>

typedef struct {
    int32_t bitmap;
    int32_t command;
    uint8_t state;
    uint8_t style;
    uint8_t reserved[2];
    uint32_t data;
    uint32_t string;
} ToolbarButton32;

typedef struct {
    DWORD process_id;
    const char *class_name;
    HWND window;
} WindowSearch;

static BOOL CALLBACK find_process_window(HWND window, LPARAM opaque) {
    WindowSearch *search = (WindowSearch *)opaque;
    DWORD process_id = 0;
    char class_name[128] = {0};
    GetWindowThreadProcessId(window, &process_id);
    GetClassNameA(window, class_name, sizeof(class_name));
    if (process_id == search->process_id && _stricmp(class_name, search->class_name) == 0) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static BOOL CALLBACK find_child_toolbar(HWND window, LPARAM opaque) {
    HWND *toolbar = (HWND *)opaque;
    char class_name[128] = {0};
    GetClassNameA(window, class_name, sizeof(class_name));
    if (_stricmp(class_name, TOOLBARCLASSNAMEA) == 0) {
        *toolbar = window;
        return FALSE;
    }
    return TRUE;
}

static DWORD find_process_id(const char *executable) {
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    PROCESSENTRY32 entry = {.dwSize = sizeof(entry)};
    DWORD process_id = 0;
    if (snapshot != INVALID_HANDLE_VALUE && Process32First(snapshot, &entry)) {
        do {
            if (_stricmp(entry.szExeFile, executable) == 0) {
                process_id = entry.th32ProcessID;
                break;
            }
        } while (Process32Next(snapshot, &entry));
    }
    if (snapshot != INVALID_HANDLE_VALUE) CloseHandle(snapshot);
    return process_id;
}

static int list_toolbar(DWORD process_id, HWND frame) {
    HWND toolbar = NULL;
    EnumChildWindows(frame, find_child_toolbar, (LPARAM)&toolbar);
    if (toolbar == NULL) {
        fputs("toolbar not found\n", stderr);
        return 1;
    }
    HANDLE process = OpenProcess(
        PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE,
        FALSE,
        process_id
    );
    void *remote = process == NULL ? NULL : VirtualAllocEx(
        process, NULL, 512, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE
    );
    if (remote == NULL) {
        if (process != NULL) CloseHandle(process);
        fputs("cannot allocate toolbar exchange buffer\n", stderr);
        return 1;
    }
    DWORD_PTR count_result = 0;
    if (!SendMessageTimeoutA(
            toolbar, TB_BUTTONCOUNT, 0, 0, SMTO_ABORTIFHUNG, 2000, &count_result)) {
        fputs("toolbar count query timed out\n", stderr);
        VirtualFreeEx(process, remote, 0, MEM_RELEASE);
        CloseHandle(process);
        return 1;
    }
    LRESULT count = (LRESULT)count_result;
    for (LRESULT index = 0; index < count; index++) {
        ToolbarButton32 button = {0};
        SIZE_T read = 0;
        DWORD_PTR button_result = 0;
        if (SendMessageTimeoutA(
                toolbar,
                TB_GETBUTTON,
                index,
                (LPARAM)remote,
                SMTO_ABORTIFHUNG,
                2000,
                &button_result) &&
            button_result &&
            ReadProcessMemory(process, remote, &button, sizeof(button), &read) &&
            read == sizeof(button)) {
            printf(
                "button=%ld command=0x%04lx bitmap=%ld state=0x%02x style=0x%02x\n",
                (long)index,
                (unsigned long)(uint32_t)button.command,
                (long)button.bitmap,
                button.state,
                button.style
            );
        }
    }
    VirtualFreeEx(process, remote, 0, MEM_RELEASE);
    CloseHandle(process);
    return 0;
}

int main(int argc, char **argv) {
    BOOL toolbar = (argc == 3 || argc == 4) && strcmp(argv[2], "toolbar") == 0;
    BOOL post = argc == 5 && strcmp(argv[2], "post") == 0;
    if (!toolbar && !post) {
        fprintf(
            stderr,
            "usage: wine-overworld-warp-oracle.exe EXECUTABLE toolbar [WINDOW_CLASS]\n"
            "       wine-overworld-warp-oracle.exe EXECUTABLE post COMMAND WINDOW_CLASS\n"
        );
        return 2;
    }
    DWORD process_id = find_process_id(argv[1]);
    if (process_id == 0) {
        fprintf(stderr, "process not found: %s\n", argv[1]);
        return 1;
    }
    WindowSearch search = {
        process_id,
        post ? argv[4] : (argc == 4 ? argv[3] : "LMFrame"),
        NULL
    };
    EnumWindows(find_process_window, (LPARAM)&search);
    if (search.window == NULL) {
        WindowSearch frame = {process_id, "OVFrame", NULL};
        EnumWindows(find_process_window, (LPARAM)&frame);
        if (frame.window != NULL) {
            EnumChildWindows(frame.window, find_process_window, (LPARAM)&search);
        }
    }
    if (search.window == NULL) {
        fputs("requested Lunar Magic frame not found\n", stderr);
        return 1;
    }
    if (post) {
        char *end = NULL;
        unsigned long command = strtoul(argv[3], &end, 0);
        if (end == argv[3] || *end != '\0' || command > 0xffff) {
            fputs("invalid command\n", stderr);
            return 2;
        }
        if (!PostMessageA(search.window, WM_COMMAND, MAKEWPARAM(command, 0), 0)) {
            fputs("cannot post command\n", stderr);
            return 1;
        }
        return 0;
    }
    return list_toolbar(process_id, search.window);
}
