#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <string.h>
#include <tlhelp32.h>

#define SAVE_LEVEL_COMMAND 0x23d2
#define DELETE_SELECTION_COMMAND 0x245b
#define DUPLICATE_SELECTION_COMMAND 0x245c
#define SELECT_ALL_COMMAND 0x245d

typedef struct {
    DWORD process_id;
    const char *class_name;
    HWND window;
} WindowSearch;

static BOOL CALLBACK find_yes_button(HWND window, LPARAM opaque) {
    HWND *result = (HWND *)opaque;
    char class_name[64] = {0};
    char title[64] = {0};
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    if (_stricmp(class_name, "Button") == 0 && strcmp(title, "&Yes") == 0) {
        *result = window;
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
            DWORD result = entry.th32ProcessID;
            CloseHandle(snapshot);
            return result;
        }
    } while (Process32Next(snapshot, &entry));
    CloseHandle(snapshot);
    return 0;
}

static BOOL CALLBACK find_window(HWND window, LPARAM opaque) {
    WindowSearch *search = (WindowSearch *)opaque;
    DWORD process_id = 0;
    char class_name[128] = {0};
    GetWindowThreadProcessId(window, &process_id);
    GetClassNameA(window, class_name, sizeof(class_name));
    if (process_id == search->process_id &&
        (!search->class_name || _stricmp(class_name, search->class_name) == 0)) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static BOOL CALLBACK find_child(HWND window, LPARAM opaque) {
    WindowSearch *search = (WindowSearch *)opaque;
    char class_name[128] = {0};
    GetClassNameA(window, class_name, sizeof(class_name));
    if (!search->class_name || _stricmp(class_name, search->class_name) == 0) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

int main(int argc, char **argv) {
    if (argc != 3 ||
        (strcmp(argv[2], "delete") != 0 && strcmp(argv[2], "insert") != 0)) {
        fputs("usage: wine-object-selection-oracle.exe EXECUTABLE delete|insert\n", stderr);
        return 2;
    }
    DWORD process_id = 0;
    for (unsigned attempt = 0; attempt < 400 && !process_id; attempt++) {
        process_id = find_process(argv[1]);
        if (!process_id) Sleep(25);
    }
    if (!process_id) return 3;
    WindowSearch frame = {process_id, "LMFrame", NULL};
    for (unsigned attempt = 0; attempt < 400 && !frame.window; attempt++) {
        EnumWindows(find_window, (LPARAM)&frame);
        if (!frame.window) Sleep(25);
    }
    if (!frame.window) return 4;
    WindowSearch canvas = {process_id, "SMWLevelEditor", NULL};
    for (unsigned attempt = 0; attempt < 400 && !canvas.window; attempt++) {
        EnumChildWindows(frame.window, find_child, (LPARAM)&canvas);
        if (!canvas.window) Sleep(25);
    }
    if (!canvas.window) return 5;

    ShowWindow(frame.window, SW_RESTORE);
    SetForegroundWindow(frame.window);
    SetFocus(canvas.window);
    Sleep(100);
    SendMessageA(frame.window, WM_COMMAND, MAKEWPARAM(SELECT_ALL_COMMAND, 0), 0);
    Sleep(100);
    SendMessageA(
        frame.window,
        WM_COMMAND,
        MAKEWPARAM(
            strcmp(argv[2], "delete") == 0
                ? DELETE_SELECTION_COMMAND
                : DUPLICATE_SELECTION_COMMAND,
            0
        ),
        0
    );
    Sleep(250);
    PostMessageA(frame.window, WM_COMMAND, MAKEWPARAM(SAVE_LEVEL_COMMAND, 0), 0);
    for (unsigned attempt = 0; attempt < 80; attempt++) {
        WindowSearch warning = {process_id, "#32770", NULL};
        EnumWindows(find_window, (LPARAM)&warning);
        if (warning.window) {
            HWND yes = GetDlgItem(warning.window, IDYES);
            if (!yes) EnumChildWindows(warning.window, find_yes_button, (LPARAM)&yes);
            if (yes) SendMessageA(yes, BM_CLICK, 0, 0);
            break;
        }
        Sleep(25);
    }
    Sleep(500);
    printf("gesture=ctrl-a,%s\n", argv[2]);
    SendMessageA(frame.window, WM_CLOSE, 0, 0);
    return 0;
}
