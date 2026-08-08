#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <string.h>
#include <tlhelp32.h>

#define OPEN_ENTRANCE_DIALOG_COMMAND 0x2524
#define SAVE_LEVEL_COMMAND 0x23d2

typedef struct {
    DWORD process_id;
    const char *class_name;
    HWND window;
    BOOL top_level;
} WindowSearch;

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
    if (process_id == search->process_id && IsWindowVisible(window) &&
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

static HWND wait_for_top_level(DWORD process_id, const char *class_name, unsigned attempts) {
    for (unsigned attempt = 0; attempt < attempts; attempt++) {
        WindowSearch search = {process_id, class_name, NULL, TRUE};
        EnumWindows(find_window, (LPARAM)&search);
        if (search.window) return search.window;
        Sleep(25);
    }
    return NULL;
}

static BOOL CALLBACK print_control(HWND window, LPARAM opaque) {
    (void)opaque;
    char class_name[128] = {0};
    char text[512] = {0};
    wchar_t wide[512] = {0};
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextW(window, wide, sizeof(wide) / sizeof(wide[0]));
    WideCharToMultiByte(CP_UTF8, 0, wide, -1, text, sizeof(text), NULL, NULL);
    printf(
        "control=%04x class=%s enabled=%d visible=%d checked=%ld selection=%ld count=%ld text=%s\n",
        GetDlgCtrlID(window) & 0xffff, class_name, IsWindowEnabled(window),
        IsWindowVisible(window), (long)SendMessageA(window, BM_GETCHECK, 0, 0),
        (long)SendMessageA(window, CB_GETCURSEL, 0, 0),
        (long)SendMessageA(window, CB_GETCOUNT, 0, 0), text
    );
    return TRUE;
}

static void notify(HWND dialog, HWND control, int id, int notification) {
    SendMessageA(
        dialog, WM_COMMAND, MAKEWPARAM(id, notification), (LPARAM)control
    );
}

static int set_check(HWND dialog, int id, BOOL checked) {
    HWND control = GetDlgItem(dialog, id);
    if (!control) return 0;
    SendMessageA(control, BM_SETCHECK, checked ? BST_CHECKED : BST_UNCHECKED, 0);
    notify(dialog, control, id, BN_CLICKED);
    return SendMessageA(control, BM_GETCHECK, 0, 0) ==
        (checked ? BST_CHECKED : BST_UNCHECKED);
}

static int set_combo(HWND dialog, int id, int selection) {
    HWND control = GetDlgItem(dialog, id);
    if (!control || SendMessageA(control, CB_SETCURSEL, selection, 0) == CB_ERR) return 0;
    notify(dialog, control, id, CBN_SELCHANGE);
    return SendMessageA(control, CB_GETCURSEL, 0, 0) == selection;
}

static int set_edit(HWND dialog, int id, const wchar_t *value) {
    HWND control = GetDlgItem(dialog, id);
    if (!control || !SetWindowTextW(control, value)) return 0;
    notify(dialog, control, id, EN_CHANGE);
    return 1;
}

static int publish_ready(HWND dialog, const char *ready_path, const char *continue_path) {
    RECT rectangle = {0};
    if (!GetWindowRect(dialog, &rectangle)) return 0;
    SetForegroundWindow(dialog);
    BringWindowToTop(dialog);
    FILE *ready = fopen(ready_path, "wb");
    if (!ready) return 0;
    int written = fprintf(
        ready, "%ld %ld %ld %ld\n", rectangle.left, rectangle.top,
        rectangle.right - rectangle.left, rectangle.bottom - rectangle.top
    );
    int closed = fclose(ready);
    if (written < 0 || closed != 0) return 0;
    for (unsigned attempt = 0; attempt < 400; attempt++) {
        DWORD attributes = GetFileAttributesA(continue_path);
        if (attributes != INVALID_FILE_ATTRIBUTES && !(attributes & FILE_ATTRIBUTE_DIRECTORY)) {
            return 1;
        }
        Sleep(25);
    }
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 5) {
        fputs(
            "usage: wine-main-midway-entrance-oracle.exe EXECUTABLE inspect|apply|cancel|reopen READY_FILE CONTINUE_FILE\n",
            stderr
        );
        return 2;
    }
    BOOL inspect = strcmp(argv[2], "inspect") == 0;
    BOOL apply = strcmp(argv[2], "apply") == 0;
    BOOL cancel = strcmp(argv[2], "cancel") == 0;
    BOOL reopen = strcmp(argv[2], "reopen") == 0;
    if (!inspect && !apply && !cancel && !reopen) return 2;
    DWORD process_id = 0;
    for (unsigned attempt = 0; attempt < 400 && !process_id; attempt++) {
        process_id = find_process(argv[1]);
        if (!process_id) Sleep(25);
    }
    if (!process_id) return 3;
    HWND frame = wait_for_top_level(process_id, "LMFrame", 400);
    if (!frame) return 4;
    RECT frame_rectangle = {0};
    if (GetWindowRect(frame, &frame_rectangle)) {
        SetWindowPos(
            frame, NULL, frame_rectangle.left, frame_rectangle.top, 1200, 800,
            SWP_NOACTIVATE | SWP_NOZORDER
        );
    }
    WindowSearch canvas = {process_id, "SMWLevelEditor", NULL, FALSE};
    for (unsigned attempt = 0; attempt < 400 && !canvas.window; attempt++) {
        EnumChildWindows(frame, find_child, (LPARAM)&canvas);
        if (!canvas.window) Sleep(25);
    }
    if (!canvas.window) return 4;
    if (!PostMessageA(
            canvas.window, WM_COMMAND,
            MAKEWPARAM(OPEN_ENTRANCE_DIALOG_COMMAND, 0), 0
        )) return 5;
    HWND dialog = wait_for_top_level(process_id, "#32770", 400);
    if (!dialog) return 6;
    if (GetWindowRect(frame, &frame_rectangle)) {
        SetWindowPos(
            dialog, NULL, frame_rectangle.left + 10, frame_rectangle.top, 0, 0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER
        );
    }
    char title[512] = {0};
    GetWindowTextA(dialog, title, sizeof(title));
    printf("dialog=%s\n", title);
    if (inspect) {
        EnumChildWindows(dialog, print_control, 0);
        SendMessageA(dialog, WM_COMMAND, IDCANCEL, 0);
        PostMessageA(frame, WM_CLOSE, 0, 0);
        return 0;
    }
    if (reopen) {
        EnumChildWindows(dialog, print_control, 0);
        if (!publish_ready(dialog, argv[3], argv[4])) return 8;
        SendMessageA(dialog, WM_COMMAND, IDCANCEL, 0);
        printf("mode=reopen\n");
        fflush(stdout);
        PostMessageA(frame, WM_CLOSE, 0, 0);
        return 0;
    }
    if (!set_edit(dialog, 0x019f, L"1A") ||
        !set_combo(dialog, 0x01a0, 3) ||
        !set_combo(dialog, 0x01a1, 4) ||
        !set_combo(dialog, 0x01a2, 1) ||
        !set_combo(dialog, 0x01a3, 3) ||
        !set_combo(dialog, 0x01a5, 2) ||
        !set_check(dialog, 0x01c1, TRUE) ||
        !set_check(dialog, 0x01c0, TRUE) ||
        !set_check(dialog, 0x028e, TRUE)) return 7;
    if (!set_check(dialog, 0x01e0, TRUE)) return 7;
    if (apply || cancel) {
        if (!IsWindowEnabled(GetDlgItem(dialog, 0x0197)) ||
            !IsWindowEnabled(GetDlgItem(dialog, 0x0069)) ||
            !IsWindowEnabled(GetDlgItem(dialog, 0x01e4))) return 7;
        if (!set_edit(dialog, 0x0197, L"0B") ||
            !set_edit(dialog, 0x0069, L"70") ||
            !set_edit(dialog, 0x006b, L"A0") ||
            !set_combo(dialog, 0x01e4, 2) ||
            !set_combo(dialog, 0x01e5, 3) ||
            !set_combo(dialog, 0x01e6, 1) ||
            !set_check(dialog, 0x01e7, TRUE) ||
            !set_check(dialog, 0x01e8, TRUE) ||
            !set_check(dialog, 0x006d, TRUE)) return 7;
    }
    if (!publish_ready(dialog, argv[3], argv[4])) return 8;
    if (cancel) {
        SendMessageA(dialog, WM_COMMAND, IDCANCEL, 0);
    } else {
        SendMessageA(dialog, WM_COMMAND, IDOK, 0);
        if (!PostMessageA(canvas.window, WM_COMMAND, MAKEWPARAM(SAVE_LEVEL_COMMAND, 0), 0)) {
            return 9;
        }
        Sleep(500);
    }
    printf("mode=%s\n", argv[2]);
    fflush(stdout);
    PostMessageA(frame, WM_CLOSE, 0, 0);
    return 0;
}
