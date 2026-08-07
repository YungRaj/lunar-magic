#include <windows.h>
#include <tlhelp32.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct window_search {
    DWORD process_id;
    const char *title;
    HWND window;
};

static DWORD find_process(const char *executable) {
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    PROCESSENTRY32 entry = {.dwSize = sizeof(entry)};
    DWORD found = 0;
    if (snapshot == INVALID_HANDLE_VALUE) {
        return 0;
    }
    if (Process32First(snapshot, &entry)) {
        do {
            if (_stricmp(entry.szExeFile, executable) == 0) {
                found = entry.th32ProcessID;
                break;
            }
        } while (Process32Next(snapshot, &entry));
    }
    CloseHandle(snapshot);
    return found;
}

static BOOL CALLBACK find_dialog(HWND window, LPARAM opaque) {
    struct window_search *search = (struct window_search *)opaque;
    DWORD process_id = 0;
    char class_name[64] = {0};
    char title[256] = {0};
    GetWindowThreadProcessId(window, &process_id);
    if (process_id != search->process_id) {
        return TRUE;
    }
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    if (_stricmp(class_name, "#32770") == 0 && strstr(title, search->title) != NULL) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static BOOL CALLBACK find_edit(HWND window, LPARAM opaque) {
    HWND *result = (HWND *)opaque;
    char class_name[64] = {0};
    GetClassNameA(window, class_name, sizeof(class_name));
    if (_stricmp(class_name, "Edit") == 0) {
        *result = window;
        return FALSE;
    }
    return TRUE;
}

int main(int argc, char **argv) {
    char *end = NULL;
    unsigned long combo_id;
    unsigned long button_id;
    DWORD process_id;
    struct window_search search;
    HWND combo;
    HWND edit = NULL;

    if (argc != 6) {
        fprintf(stderr,
                "usage: wine-editable-combo-command.exe EXECUTABLE DIALOG_TITLE "
                "COMBO_ID TEXT BUTTON_ID\n");
        return 2;
    }
    combo_id = strtoul(argv[3], &end, 0);
    if (end == argv[3] || *end != '\0' || combo_id > 0xffff) {
        fprintf(stderr, "invalid combo id: %s\n", argv[3]);
        return 2;
    }
    button_id = strtoul(argv[5], &end, 0);
    if (end == argv[5] || *end != '\0' || button_id > 0xffff) {
        fprintf(stderr, "invalid button id: %s\n", argv[5]);
        return 2;
    }
    process_id = find_process(argv[1]);
    if (process_id == 0) {
        fprintf(stderr, "process not found: %s\n", argv[1]);
        return 1;
    }
    search.process_id = process_id;
    search.title = argv[2];
    search.window = NULL;
    EnumWindows(find_dialog, (LPARAM)&search);
    if (search.window == NULL) {
        fprintf(stderr, "dialog not found: %s\n", argv[2]);
        return 1;
    }
    combo = GetDlgItem(search.window, (int)combo_id);
    if (combo == NULL) {
        fprintf(stderr, "combo not found: 0x%04lx\n", combo_id);
        return 1;
    }
    EnumChildWindows(combo, find_edit, (LPARAM)&edit);
    if (edit == NULL) {
        fprintf(stderr, "editable child not found for combo: 0x%04lx\n", combo_id);
        return 1;
    }
    if (!SendMessageA(edit, WM_SETTEXT, 0, (LPARAM)argv[4])) {
        fprintf(stderr, "cannot set combo text: 0x%04lx\n", combo_id);
        return 1;
    }
    SendMessageA(search.window, WM_COMMAND,
                 MAKEWPARAM((UINT)button_id, BN_CLICKED),
                 (LPARAM)GetDlgItem(search.window, (int)button_id));
    return 0;
}
