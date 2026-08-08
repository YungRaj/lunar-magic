#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <string.h>
#include <tlhelp32.h>

#define OPEN_SECONDARY_EXIT_DIALOG_COMMAND 0x2525
#define SAVE_LEVEL_COMMAND 0x23d2
#define TARGET_EXIT_INDEX 0x1ffe

typedef struct {
    DWORD process_id;
    const char *class_name;
    const char *title;
    HWND excluded;
    HWND window;
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
    char title[512] = {0};
    GetWindowThreadProcessId(window, &process_id);
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    if (window != search->excluded && process_id == search->process_id && IsWindowVisible(window) &&
        (!search->class_name || _stricmp(class_name, search->class_name) == 0) &&
        (!search->title || strcmp(title, search->title) == 0)) {
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

static HWND wait_for_window(
    DWORD process_id, const char *class_name, const char *title, HWND excluded, unsigned attempts
) {
    for (unsigned attempt = 0; attempt < attempts; attempt++) {
        WindowSearch search = {process_id, class_name, title, excluded, NULL};
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
        GetDlgCtrlID(window) & 0xffff, class_name, IsWindowEnabled(window), IsWindowVisible(window),
        (long)SendMessageA(window, BM_GETCHECK, 0, 0),
        (long)SendMessageA(window, CB_GETCURSEL, 0, 0),
        (long)SendMessageA(window, CB_GETCOUNT, 0, 0), text
    );
    return TRUE;
}

static void notify(HWND dialog, HWND control, int id, int notification) {
    SendMessageA(dialog, WM_COMMAND, MAKEWPARAM(id, notification), (LPARAM)control);
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

static int select_target(HWND dialog) {
    return set_combo(dialog, 0x006d, TARGET_EXIT_INDEX);
}

static int configure_target(HWND dialog) {
    if (!select_target(dialog)) return 0;
    return set_edit(dialog, 0x015d, L"1AB") &&
        set_edit(dialog, 0x019f, L"1D") &&
        set_combo(dialog, 0x01a0, 6) &&
        set_combo(dialog, 0x01a1, 11) &&
        set_combo(dialog, 0x01a2, 2) &&
        set_combo(dialog, 0x01a3, 3) &&
        set_combo(dialog, 0x01a5, 4) &&
        set_check(dialog, 0x01c1, TRUE) &&
        set_check(dialog, 0x01c0, TRUE) &&
        set_check(dialog, 0x028e, TRUE);
}

static int publish_ready(HWND window, const char *ready_path, const char *continue_path) {
    RECT rectangle = {0};
    if (!GetWindowRect(window, &rectangle)) return 0;
    SetForegroundWindow(window);
    BringWindowToTop(window);
    FILE *ready = fopen(ready_path, "wb");
    if (!ready) return 0;
    int written = fprintf(
        ready, "%ld %ld %ld %ld\n", rectangle.left, rectangle.top,
        rectangle.right - rectangle.left, rectangle.bottom - rectangle.top
    );
    int closed = fclose(ready);
    if (written < 0 || closed != 0) return 0;
    for (unsigned attempt = 0; attempt < 800; attempt++) {
        DWORD attributes = GetFileAttributesA(continue_path);
        if (attributes != INVALID_FILE_ATTRIBUTES && !(attributes & FILE_ATTRIBUTE_DIRECTORY)) {
            return 1;
        }
        Sleep(25);
    }
    return 0;
}

static HWND open_dialog(HWND frame, DWORD process_id) {
    if (!PostMessageA(
            frame, WM_COMMAND, MAKEWPARAM(OPEN_SECONDARY_EXIT_DIALOG_COMMAND, 0), 0
        )) return NULL;
    return wait_for_window(
        process_id, "#32770", "Modify Secondary Entrances (in hex)", NULL, 800
    );
}

static int save_and_accept_prompts(HWND frame, HWND canvas, DWORD process_id) {
    if (!PostMessageA(canvas, WM_COMMAND, MAKEWPARAM(SAVE_LEVEL_COMMAND, 0), 0)) return 0;
    for (unsigned pass = 0; pass < 8; pass++) {
        Sleep(250);
        HWND prompt = wait_for_window(process_id, "#32770", NULL, NULL, 1);
        if (!prompt) continue;
        char title[512] = {0};
        GetWindowTextA(prompt, title, sizeof(title));
        printf("save-prompt=%s\n", title);
        HWND yes = GetDlgItem(prompt, IDYES);
        HWND ok = GetDlgItem(prompt, IDOK);
        if (yes) SendMessageA(prompt, WM_COMMAND, IDYES, (LPARAM)yes);
        else if (ok) SendMessageA(prompt, WM_COMMAND, IDOK, (LPARAM)ok);
        else return 0;
    }
    Sleep(500);
    PostMessageA(frame, WM_CLOSE, 0, 0);
    return 1;
}

int main(int argc, char **argv) {
    if (argc != 5) {
        fputs(
            "usage: wine-secondary-exit-oracle.exe EXECUTABLE inspect|apply|cancel-reopen|reopen|clear-slot|clear-all-no|clear-all-yes READY_FILE CONTINUE_FILE\n",
            stderr
        );
        return 2;
    }
    const char *mode = argv[2];
    BOOL inspect = strcmp(mode, "inspect") == 0;
    BOOL apply = strcmp(mode, "apply") == 0;
    BOOL cancel_reopen = strcmp(mode, "cancel-reopen") == 0;
    BOOL reopen = strcmp(mode, "reopen") == 0;
    BOOL clear_slot = strcmp(mode, "clear-slot") == 0;
    BOOL clear_all_no = strcmp(mode, "clear-all-no") == 0;
    BOOL clear_all_yes = strcmp(mode, "clear-all-yes") == 0;
    if (!inspect && !apply && !cancel_reopen && !reopen && !clear_slot &&
        !clear_all_no && !clear_all_yes) return 2;

    DWORD process_id = 0;
    for (unsigned attempt = 0; attempt < 800 && !process_id; attempt++) {
        process_id = find_process(argv[1]);
        if (!process_id) Sleep(25);
    }
    if (!process_id) return 3;
    HWND frame = wait_for_window(process_id, "LMFrame", NULL, NULL, 800);
    if (!frame) return 4;
    RECT frame_rectangle = {0};
    if (GetWindowRect(frame, &frame_rectangle)) {
        SetWindowPos(
            frame, NULL, frame_rectangle.left, frame_rectangle.top, 1200, 800,
            SWP_NOACTIVATE | SWP_NOZORDER
        );
    }
    WindowSearch canvas_search = {process_id, "SMWLevelEditor", NULL, NULL, NULL};
    for (unsigned attempt = 0; attempt < 800 && !canvas_search.window; attempt++) {
        EnumChildWindows(frame, find_child, (LPARAM)&canvas_search);
        if (!canvas_search.window) Sleep(25);
    }
    HWND canvas = canvas_search.window;
    if (!canvas) return 4;
    HWND dialog = open_dialog(frame, process_id);
    if (!dialog) return 5;
    if (GetWindowRect(frame, &frame_rectangle)) {
        SetWindowPos(
            dialog, NULL, frame_rectangle.left + 10, frame_rectangle.top, 0, 0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER
        );
    }
    printf("dialog=Modify Secondary Entrances (in hex)\n");

    if (inspect) {
        EnumChildWindows(dialog, print_control, 0);
        SendMessageA(dialog, WM_COMMAND, IDCANCEL, 0);
        PostMessageA(frame, WM_CLOSE, 0, 0);
        return 0;
    }
    if (apply || cancel_reopen) {
        if (!configure_target(dialog)) return 6;
        if (!publish_ready(dialog, argv[3], argv[4])) return 7;
        if (cancel_reopen) {
            SendMessageA(dialog, WM_COMMAND, IDCANCEL, 0);
            dialog = open_dialog(frame, process_id);
            if (!dialog || !select_target(dialog)) return 8;
            char destination[32] = {0};
            GetDlgItemTextA(dialog, 0x015d, destination, sizeof(destination));
            printf("cancel-reopen-destination=%s\n", destination);
            if (strcmp(destination, "0") != 0) return 8;
            SendMessageA(dialog, WM_COMMAND, IDCANCEL, 0);
            PostMessageA(frame, WM_CLOSE, 0, 0);
            return 0;
        }
        SendMessageA(dialog, WM_COMMAND, IDOK, 0);
        return save_and_accept_prompts(frame, canvas, process_id) ? 0 : 9;
    }

    if (!select_target(dialog)) return 6;
    if (reopen) {
        EnumChildWindows(dialog, print_control, 0);
        if (!publish_ready(dialog, argv[3], argv[4])) return 7;
        SendMessageA(dialog, WM_COMMAND, IDCANCEL, 0);
        PostMessageA(frame, WM_CLOSE, 0, 0);
        return 0;
    }
    if (clear_slot) {
        HWND button = GetDlgItem(dialog, 0x0066);
        if (!button) return 6;
        SendMessageA(dialog, WM_COMMAND, MAKEWPARAM(0x0066, BN_CLICKED), (LPARAM)button);
        if (!publish_ready(dialog, argv[3], argv[4])) return 7;
        SendMessageA(dialog, WM_COMMAND, IDOK, 0);
        return save_and_accept_prompts(frame, canvas, process_id) ? 0 : 9;
    }

    HWND clear_all = GetDlgItem(dialog, 0x0065);
    if (!clear_all) return 6;
    if (!PostMessageA(
            dialog, WM_COMMAND, MAKEWPARAM(0x0065, BN_CLICKED), (LPARAM)clear_all
        )) return 6;
    HWND prompt = wait_for_window(process_id, "#32770", NULL, dialog, 800);
    if (!prompt) return 10;
    char prompt_title[512] = {0};
    GetWindowTextA(prompt, prompt_title, sizeof(prompt_title));
    printf("clear-all-prompt=%s\n", prompt_title);
    if (!publish_ready(prompt, argv[3], argv[4])) return 7;
    if (clear_all_no) {
        SendMessageA(prompt, WM_COMMAND, IDNO, (LPARAM)GetDlgItem(prompt, IDNO));
        char destination[32] = {0};
        GetDlgItemTextA(dialog, 0x015d, destination, sizeof(destination));
        printf("clear-all-no-destination=%s\n", destination);
        if (_stricmp(destination, "1AB") != 0) return 11;
        SendMessageA(dialog, WM_COMMAND, IDCANCEL, 0);
        PostMessageA(frame, WM_CLOSE, 0, 0);
        return 0;
    }
    SendMessageA(prompt, WM_COMMAND, IDYES, (LPARAM)GetDlgItem(prompt, IDYES));
    if (!select_target(dialog)) return 6;
    char destination[32] = {0};
    GetDlgItemTextA(dialog, 0x015d, destination, sizeof(destination));
    printf("clear-all-yes-destination=%s\n", destination);
    if (strcmp(destination, "0") != 0) return 11;
    SendMessageA(dialog, WM_COMMAND, IDOK, 0);
    return save_and_accept_prompts(frame, canvas, process_id) ? 0 : 9;
}
