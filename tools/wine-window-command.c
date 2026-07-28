#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tlhelp32.h>

struct search {
    DWORD process_id;
    HWND window;
    BOOL list;
    const char *window_class;
};

static BOOL CALLBACK list_child_window(HWND window, LPARAM opaque) {
    (void)opaque;
    char class_name[128] = {0};
    char title[512] = {0};
    GetClassName(window, class_name, sizeof(class_name));
    GetWindowText(window, title, sizeof(title));
    printf(
        "  child=0x%p id=0x%04lx class=%s title=%s\n",
        window,
        (unsigned long)GetDlgCtrlID(window),
        class_name,
        title
    );
    return TRUE;
}

static BOOL CALLBACK find_top_level_window(HWND window, LPARAM opaque) {
    struct search *search = (struct search *)opaque;
    DWORD process_id = 0;
    GetWindowThreadProcessId(window, &process_id);
    if (process_id == search->process_id && search->list) {
        char class_name[128] = {0};
        char title[512] = {0};
        GetClassName(window, class_name, sizeof(class_name));
        GetWindowText(window, title, sizeof(title));
        printf(
            "hwnd=0x%p owner=0x%p class=%s title=%s\n",
            window,
            GetWindow(window, GW_OWNER),
            class_name,
            title
        );
        if (
            search->window_class != NULL &&
            _stricmp(class_name, search->window_class) == 0
        ) {
            EnumChildWindows(window, list_child_window, 0);
        }
    } else if (process_id == search->process_id) {
        char class_name[128] = {0};
        GetClassName(window, class_name, sizeof(class_name));
        if (
            (search->window_class != NULL &&
             _stricmp(class_name, search->window_class) == 0) ||
            (search->window_class == NULL && GetWindow(window, GW_OWNER) == NULL)
        ) {
            search->window = window;
            return FALSE;
        }
    }
    return TRUE;
}

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

int main(int argc, char **argv) {
    if (argc < 3 || argc > 4) {
        fprintf(
            stderr,
            "usage: wine-window-command.exe EXECUTABLE COMMAND_ID [WINDOW_CLASS]\n"
            "       wine-window-command.exe EXECUTABLE save WINDOWS_PATH\n"
            "       wine-window-command.exe EXECUTABLE level HEX_LEVEL\n"
        );
        return 2;
    }
    DWORD process_id = find_process(argv[1]);
    if (process_id == 0) {
        fprintf(stderr, "process not found: %s\n", argv[1]);
        return 1;
    }
    BOOL save = _stricmp(argv[2], "save") == 0;
    BOOL level = _stricmp(argv[2], "level") == 0;
    struct search search = {
        .process_id = process_id,
        .window = NULL,
        .list = _stricmp(argv[2], "list") == 0,
        .window_class = save || level ? "#32770" : (argc == 4 ? argv[3] : NULL)
    };
    EnumWindows(find_top_level_window, (LPARAM)&search);
    if (search.list) {
        return 0;
    }
    if (search.window == NULL) {
        fprintf(stderr, "top-level window not found for process %lu\n", process_id);
        return 1;
    }
    if (save) {
        if (argc != 4) {
            fprintf(stderr, "save requires a Windows output path\n");
            return 2;
        }
        HWND edit = GetDlgItem(search.window, 0x47c);
        if (edit == NULL) {
            fprintf(stderr, "file-name control not found\n");
            return 1;
        }
        SendMessage(edit, WM_SETTEXT, 0, (LPARAM)argv[3]);
        SendMessage(search.window, WM_COMMAND, MAKEWPARAM(IDOK, BN_CLICKED), (LPARAM)GetDlgItem(search.window, IDOK));
        return 0;
    }
    if (level) {
        if (argc != 4) {
            fprintf(stderr, "level requires a hexadecimal level number\n");
            return 2;
        }
        HWND edit = GetDlgItem(search.window, 0x7f);
        if (edit == NULL) {
            fprintf(stderr, "level-number control not found\n");
            return 1;
        }
        SendMessage(edit, WM_SETTEXT, 0, (LPARAM)argv[3]);
        SendMessage(
            search.window,
            WM_COMMAND,
            MAKEWPARAM(IDOK, BN_CLICKED),
            (LPARAM)GetDlgItem(search.window, IDOK)
        );
        return 0;
    }
    char *end = NULL;
    unsigned long command = strtoul(argv[2], &end, 0);
    if (end == argv[2] || *end != '\0' || command > 0xffff) {
        fprintf(stderr, "invalid command id: %s\n", argv[2]);
        return 2;
    }
    printf("pid=%lu hwnd=0x%p command=0x%04lx\n", process_id, search.window, command);
    SendMessage(search.window, WM_COMMAND, MAKEWPARAM(command, 0), 0);
    return 0;
}
