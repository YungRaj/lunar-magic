#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tlhelp32.h>

struct editor {
    DWORD process_id;
    HWND dialog;
    HWND viewer;
};

struct window_search {
    DWORD process_id;
    const char *class_name;
    const char *title;
    HWND window;
};

struct child_search {
    int control_id;
    const char *class_name;
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

static BOOL CALLBACK find_window(HWND window, LPARAM opaque) {
    struct window_search *search = (struct window_search *)opaque;
    DWORD process_id = 0;
    char class_name[128] = {0};
    char title[256] = {0};
    GetWindowThreadProcessId(window, &process_id);
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    if (
        process_id == search->process_id &&
        IsWindowVisible(window) &&
        (search->class_name == NULL || strcmp(class_name, search->class_name) == 0) &&
        (search->title == NULL || strcmp(title, search->title) == 0)
    ) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static HWND wait_for_window(
    DWORD process_id,
    const char *class_name,
    const char *title,
    DWORD timeout_ms
) {
    DWORD waited = 0;
    while (waited <= timeout_ms) {
        struct window_search search = {
            .process_id = process_id,
            .class_name = class_name,
            .title = title,
            .window = NULL,
        };
        EnumWindows(find_window, (LPARAM)&search);
        if (search.window != NULL) {
            return search.window;
        }
        Sleep(50);
        waited += 50;
    }
    return NULL;
}

static BOOL CALLBACK find_child(HWND window, LPARAM opaque) {
    struct child_search *search = (struct child_search *)opaque;
    char class_name[128] = {0};
    GetClassNameA(window, class_name, sizeof(class_name));
    if (
        GetDlgCtrlID(window) == search->control_id &&
        (search->class_name == NULL || _stricmp(class_name, search->class_name) == 0)
    ) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static HWND child(HWND parent, int control_id, const char *class_name) {
    struct child_search search = {
        .control_id = control_id,
        .class_name = class_name,
        .window = NULL,
    };
    EnumChildWindows(parent, find_child, (LPARAM)&search);
    return search.window;
}

static BOOL CALLBACK find_viewer(HWND window, LPARAM opaque) {
    struct editor *editor = (struct editor *)opaque;
    char class_name[128] = {0};
    GetClassNameA(window, class_name, sizeof(class_name));
    if (_stricmp(class_name, "Window16x16view") == 0) {
        editor->viewer = window;
        return FALSE;
    }
    return TRUE;
}

static void set_page(struct editor *editor, const char *page) {
    HWND edit = GetDlgItem(editor->dialog, 0x74);
    SendMessageA(edit, WM_SETTEXT, 0, (LPARAM)page);
    SendMessageA(
        editor->dialog,
        WM_COMMAND,
        MAKEWPARAM(0x74, EN_CHANGE),
        (LPARAM)edit
    );
    SendMessageA(GetDlgItem(editor->dialog, 0x73), BM_CLICK, 0, 0);
    Sleep(500);
}

static void drag(HWND viewer, int left, int top, int right, int bottom) {
    SendMessageA(viewer, WM_MOUSEMOVE, 0, MAKELPARAM(left, top));
    SendMessageA(viewer, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(left, top));
    SendMessageA(viewer, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(right, bottom));
    SendMessageA(viewer, WM_LBUTTONUP, 0, MAKELPARAM(right, bottom));
}

static void prime_selection(struct editor *editor) {
    HWND mode = GetDlgItem(editor->dialog, 0x1d9);
    if (SendMessageA(mode, BM_GETCHECK, 0, 0) != BST_CHECKED) {
        SendMessageA(mode, BM_CLICK, 0, 0);
    }
    drag(editor->viewer, 4, 4, 12, 12);
    SendMessageA(mode, BM_CLICK, 0, 0);
}

static void select_tile_200(struct editor *editor) {
    set_page(editor, "02");
    prime_selection(editor);
    drag(editor->viewer, 8, 8, 9, 9);
    Sleep(100);
}

static void select_page_02(struct editor *editor) {
    set_page(editor, "02");
    prime_selection(editor);
    /* The Wine-scaled viewer's inclusive endpoint for an exact 16 by 16 page. */
    drag(editor->viewer, 8, 8, 255, 247);
    Sleep(100);
}

static int select_namespace(struct editor *editor, int index) {
    PostMessageA(GetDlgItem(editor->dialog, 0x6a), BM_CLICK, 0, 0);
    HWND popup = wait_for_window(editor->process_id, "#32768", NULL, 2000);
    if (popup == NULL) {
        fprintf(stderr, "Select Tiles popup did not open\n");
        return 0;
    }
    RECT bounds = {0};
    if (!GetWindowRect(popup, &bounds)) {
        fprintf(stderr, "cannot read Select Tiles popup bounds\n");
        return 0;
    }
    POINT point = {
        .x = (bounds.left + bounds.right) / 2,
        .y = bounds.top + 10 + index * 19,
    };
    SetCursorPos(point.x, point.y);
    mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
    mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    Sleep(250);
    return !IsWindow(popup) || !IsWindowVisible(popup);
}

static int open_transfer_dialog(struct editor *editor, int command, const char *title) {
    HWND button = GetDlgItem(editor->dialog, command);
    if (button == NULL || !PostMessageA(button, BM_CLICK, 0, 0)) {
        fprintf(stderr, "cannot invoke Map16 command 0x%04x\n", command);
        return 0;
    }
    return wait_for_window(editor->process_id, "#32770", title, 5000) != NULL;
}

static int complete_file_dialog(
    struct editor *editor,
    const char *title,
    int filter,
    const char *path
) {
    HWND dialog = wait_for_window(editor->process_id, "#32770", title, 1000);
    if (dialog == NULL) {
        fprintf(stderr, "%s dialog did not remain open\n", title);
        return 0;
    }
    HWND format = GetDlgItem(dialog, 0x470);
    if (format == NULL || SendMessageA(format, CB_SETCURSEL, filter, 0) == CB_ERR) {
        fprintf(stderr, "%s format index %d is unavailable\n", title, filter);
        return 0;
    }
    SendMessageA(
        dialog,
        WM_COMMAND,
        MAKEWPARAM(0x470, CBN_SELCHANGE),
        (LPARAM)format
    );
    HWND filename = child(dialog, 0x47c, "Edit");
    if (filename == NULL) {
        fprintf(stderr, "%s filename control is unavailable\n", title);
        return 0;
    }
    SendMessageA(filename, WM_SETTEXT, 0, (LPARAM)path);
    SendMessageA(GetDlgItem(dialog, IDOK), BM_CLICK, 0, 0);
    DWORD waited = 0;
    while (IsWindow(dialog) && IsWindowVisible(dialog) && waited < 10000) {
        Sleep(50);
        waited += 50;
    }
    if (IsWindow(dialog) && IsWindowVisible(dialog)) {
        fprintf(stderr, "%s dialog did not close\n", title);
        return 0;
    }
    return 1;
}

static int report_restore_prompt(struct editor *editor) {
    HWND prompt = wait_for_window(
        editor->process_id,
        "#32770",
        "Restore System Issue",
        5000
    );
    if (prompt == NULL) {
        return 0;
    }
    char text[512] = {0};
    HWND message = child(prompt, 0xffff, "Static");
    if (message != NULL) {
        GetWindowTextA(message, text, sizeof(text));
    }
    printf("restore-prompt=%s\n", text);
    SendMessageA(GetDlgItem(prompt, IDCANCEL), BM_CLICK, 0, 0);
    return 1;
}

int main(int argc, char **argv) {
    if (argc != 4) {
        fprintf(
            stderr,
            "usage: wine-map16-transfer-oracle.exe PROCESS ACTION WINDOWS_PATH\n"
            "actions: selected-export selected-import page-export-raw page-import-raw "
            "fg-export-raw fg-import-raw bg-export-raw bg-import-raw "
            "complete-export complete-import complete-import-expect-restore\n"
        );
        return 2;
    }
    struct editor editor = {.process_id = find_process(argv[1])};
    editor.dialog = wait_for_window(
        editor.process_id,
        "#32770",
        "16x16 Tile Map Editor",
        1000
    );
    if (editor.process_id == 0 || editor.dialog == NULL) {
        fprintf(stderr, "visible Map16 editor not found for %s\n", argv[1]);
        return 1;
    }
    EnumChildWindows(editor.dialog, find_viewer, (LPARAM)&editor);
    if (editor.viewer == NULL) {
        fprintf(stderr, "Window16x16view not found\n");
        return 1;
    }

    const char *action = argv[2];
    const char *path = argv[3];
    int command = 0;
    int filter = 0;
    const char *dialog_title = NULL;
    if (strcmp(action, "selected-export") == 0) {
        select_tile_200(&editor); command = 0x2266; dialog_title = "Save As";
    } else if (strcmp(action, "selected-import") == 0) {
        select_tile_200(&editor); command = 0x2267; dialog_title = "Open";
    } else if (strcmp(action, "page-export-raw") == 0) {
        select_page_02(&editor); command = 0x2266; filter = 1; dialog_title = "Save As";
    } else if (strcmp(action, "page-import-raw") == 0) {
        select_tile_200(&editor); command = 0x2267; filter = 1; dialog_title = "Open";
    } else if (strcmp(action, "fg-export-raw") == 0 || strcmp(action, "fg-import-raw") == 0) {
        if (!select_namespace(&editor, 0)) return 1;
        command = strcmp(action, "fg-export-raw") == 0 ? 0x2266 : 0x2267;
        filter = 1; dialog_title = command == 0x2266 ? "Save As" : "Open";
    } else if (strcmp(action, "bg-export-raw") == 0 || strcmp(action, "bg-import-raw") == 0) {
        if (!select_namespace(&editor, 1)) return 1;
        command = strcmp(action, "bg-export-raw") == 0 ? 0x2266 : 0x2267;
        filter = 1; dialog_title = command == 0x2266 ? "Save As" : "Open";
    } else if (strcmp(action, "complete-export") == 0) {
        command = 0x2268; dialog_title = "Save As";
    } else if (
        strcmp(action, "complete-import") == 0 ||
        strcmp(action, "complete-import-expect-restore") == 0
    ) {
        command = 0x2269; dialog_title = "Open";
    } else {
        fprintf(stderr, "unknown action: %s\n", action);
        return 2;
    }

    if (
        !open_transfer_dialog(&editor, command, dialog_title) ||
        !complete_file_dialog(&editor, dialog_title, filter, path)
    ) {
        return 1;
    }
    if (strcmp(action, "complete-import-expect-restore") == 0) {
        if (!report_restore_prompt(&editor)) {
            fprintf(stderr, "expected Restore System Issue prompt did not appear\n");
            return 1;
        }
    } else if (wait_for_window(
        editor.process_id,
        "#32770",
        "Restore System Issue",
        500
    ) != NULL) {
        fprintf(stderr, "unexpected Restore System Issue prompt\n");
        return 1;
    }
    printf(
        "action=%s command=0x%04x dialog=%s filter=%d path=%s\n",
        action,
        command,
        dialog_title,
        filter,
        path
    );
    return 0;
}
