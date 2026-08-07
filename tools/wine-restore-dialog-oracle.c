#define WIN32_LEAN_AND_MEAN
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

static BOOL CALLBACK find_window(HWND window, LPARAM parameter) {
    struct window_search *search = (struct window_search *)parameter;
    DWORD process_id = 0;
    char title[256] = {0};
    GetWindowThreadProcessId(window, &process_id);
    GetWindowTextA(window, title, sizeof(title));
    if (process_id == search->process_id && strcmp(title, search->title) == 0) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static HWND find_titled_window(DWORD process_id, const char *title) {
    struct window_search search = {process_id, title, NULL};
    EnumWindows(find_window, (LPARAM)&search);
    return search.window;
}

static int inspect(HWND dialog) {
    HWND list = GetDlgItem(dialog, 0x66);
    if (list == NULL) {
        fputs("restore list not found\n", stderr);
        return 1;
    }
    LRESULT count = SendMessageA(list, LB_GETCOUNT, 0, 0);
    LRESULT selected = SendMessageA(list, LB_GETCURSEL, 0, 0);
    printf("count=%ld selected=%ld associated=%u\n", (long)count, (long)selected,
           IsDlgButtonChecked(dialog, 0x65));
    for (LRESULT index = 0; index < count; index++) {
        char text[1024] = {0};
        LRESULT length = SendMessageA(list, LB_GETTEXT, index, (LPARAM)text);
        if (length == LB_ERR) {
            fprintf(stderr, "cannot read restore row %ld\n", (long)index);
            return 1;
        }
        printf("row=%ld text=%s\n", (long)index, text);
    }
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 3 || argc > 4) {
        fputs("usage: wine-restore-dialog-oracle.exe EXECUTABLE inspect|select|ok [INDEX]\n", stderr);
        return 2;
    }
    DWORD process_id = find_process(argv[1]);
    if (process_id == 0) {
        fprintf(stderr, "process not found: %s\n", argv[1]);
        return 1;
    }
    HWND dialog = find_titled_window(process_id, "Restore ROM to Previous State");
    if (dialog == NULL) {
        fputs("restore dialog not found\n", stderr);
        return 1;
    }
    if (_stricmp(argv[2], "inspect") == 0) return inspect(dialog);
    if (_stricmp(argv[2], "ok") == 0) {
        if (!PostMessageA(dialog, WM_COMMAND, MAKEWPARAM(IDOK, BN_CLICKED),
                          (LPARAM)GetDlgItem(dialog, IDOK))) {
            fputs("cannot submit restore dialog\n", stderr);
            return 1;
        }
        return 0;
    }
    if (_stricmp(argv[2], "select") == 0 && argc == 4) {
        char *end = NULL;
        unsigned long index = strtoul(argv[3], &end, 0);
        HWND list = GetDlgItem(dialog, 0x66);
        if (*argv[3] == '\0' || *end != '\0' || index > INT_MAX || list == NULL ||
            SendMessageA(list, LB_SETCURSEL, index, 0) == LB_ERR) {
            fprintf(stderr, "cannot select restore row: %s\n", argv[3]);
            return 1;
        }
        SendMessageA(dialog, WM_COMMAND, MAKEWPARAM(0x66, LBN_SELCHANGE), (LPARAM)list);
        return inspect(dialog);
    }
    fputs("invalid restore dialog action\n", stderr);
    return 2;
}
