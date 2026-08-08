#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tlhelp32.h>

#define CATEGORY_CONTROL_ID 0x01de
#define LIST_CONTROL_ID 0x0065
#define CUSTOM_CATEGORY_INDEX 5
#define ADD_OBJECTS_COMMAND 0x2330
#define SAVE_LEVEL_COMMAND 0x23d2

typedef struct {
    DWORD process_id;
    const char *class_name;
    const char *title;
    HWND window;
    BOOL visible_only;
} WindowSearch;

typedef struct {
    int control_id;
    HWND window;
} ControlSearch;

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
    char title[256] = {0};
    GetWindowThreadProcessId(window, &process_id);
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    if (process_id == search->process_id &&
        (!search->class_name || _stricmp(class_name, search->class_name) == 0) &&
        (!search->title || strcmp(title, search->title) == 0) &&
        (!search->visible_only || IsWindowVisible(window))) {
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

static BOOL CALLBACK find_control(HWND window, LPARAM opaque) {
    ControlSearch *search = (ControlSearch *)opaque;
    if (GetDlgCtrlID(window) == search->control_id) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

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

static HWND descendant_control(HWND parent, int control_id) {
    ControlSearch search = {control_id, NULL};
    EnumChildWindows(parent, find_control, (LPARAM)&search);
    return search.window;
}

static HWND wait_for_window(
    DWORD process_id,
    const char *class_name,
    const char *title,
    unsigned attempts
) {
    for (unsigned attempt = 0; attempt < attempts; attempt++) {
        WindowSearch search = {process_id, class_name, title, NULL, TRUE};
        EnumWindows(find_window, (LPARAM)&search);
        if (search.window) return search.window;
        Sleep(25);
    }
    return NULL;
}

static int read_list_text(DWORD process_id, HWND list, int index, char *output, size_t capacity) {
    (void)process_id;
    wchar_t wide[1024] = {0};
    LRESULT length = SendMessageW(list, LB_GETTEXT, index, (LPARAM)wide);
    return length != LB_ERR && (size_t)length < sizeof(wide) / sizeof(wide[0]) &&
        WideCharToMultiByte(
            CP_UTF8, 0, wide, -1, output, (int)capacity, NULL, NULL
        ) != 0;
}

int main(int argc, char **argv) {
    if (argc != 7) {
        fputs(
            "usage: wine-custom-object-oracle.exe EXECUTABLE EXPECTED_DESCRIPTION X Y READY_FILE CONTINUE_FILE\n",
            stderr
        );
        return 2;
    }
    char *x_end = NULL;
    char *y_end = NULL;
    unsigned long x = strtoul(argv[3], &x_end, 0);
    unsigned long y = strtoul(argv[4], &y_end, 0);
    if (!x_end || *x_end || !y_end || *y_end || x > 0xffff || y > 0xffff) return 2;

    DWORD process_id = 0;
    for (unsigned attempt = 0; attempt < 400 && !process_id; attempt++) {
        process_id = find_process(argv[1]);
        if (!process_id) Sleep(25);
    }
    if (!process_id) return 3;
    WindowSearch frame = {
        process_id,
        "LMFrame",
        NULL,
        wait_for_window(process_id, "LMFrame", NULL, 400),
        FALSE,
    };
    WindowSearch dialog = {process_id, NULL, "Add Objects Window", NULL, TRUE};
    if (!frame.window) {
        fputs("Lunar Magic frame unavailable\n", stderr);
        return 4;
    }
    WindowSearch canvas = {process_id, "SMWLevelEditor", NULL, NULL, FALSE};
    for (unsigned attempt = 0; attempt < 400 && !canvas.window; attempt++) {
        EnumChildWindows(frame.window, find_child, (LPARAM)&canvas);
        if (!canvas.window) Sleep(25);
    }
    if (!canvas.window) {
        fputs("level canvas unavailable\n", stderr);
        return 4;
    }
    for (unsigned attempt = 0; attempt < 1200; attempt++) {
        if (attempt % 200 == 0 && !PostMessageA(
                canvas.window, WM_COMMAND, MAKEWPARAM(ADD_OBJECTS_COMMAND, 0), 0
            )) {
            fputs("cannot open Add Objects window\n", stderr);
            return 4;
        }
        if (!dialog.window) {
            dialog.window = wait_for_window(
                process_id, NULL, "Add Objects Window", 1
            );
        }
        if (dialog.window && IsWindowVisible(dialog.window)) break;
        Sleep(25);
    }
    if (!dialog.window || !IsWindowVisible(dialog.window)) {
        fputs("Add Objects window unavailable\n", stderr);
        return 4;
    }

    HWND category = descendant_control(dialog.window, CATEGORY_CONTROL_ID);
    HWND list = descendant_control(dialog.window, LIST_CONTROL_ID);
    if (!category || !list) return 5;
    if (SendMessageA(category, CB_SETCURSEL, CUSTOM_CATEGORY_INDEX, 0) == CB_ERR) return 6;
    HWND category_parent = GetParent(category);
    SendMessageA(
        category_parent,
        WM_COMMAND,
        MAKEWPARAM(CATEGORY_CONTROL_ID, CBN_SELCHANGE),
        (LPARAM)category
    );
    if (SendMessageA(category, CB_GETCURSEL, 0, 0) != CUSTOM_CATEGORY_INDEX) return 7;
    LRESULT list_count = LB_ERR;
    for (unsigned attempt = 0; attempt < 400; attempt++) {
        list_count = SendMessageA(list, LB_GETCOUNT, 0, 0);
        if (list_count == 1) break;
        Sleep(25);
    }
    if (list_count != 1) {
        fprintf(stderr, "unexpected custom object list count: %ld\n", (long)list_count);
        return 7;
    }
    char description[1024] = {0};
    if (!read_list_text(process_id, list, 0, description, sizeof(description))) return 8;
    if (strcmp(description, argv[2]) != 0) {
        fprintf(stderr, "unexpected custom object description: %s\n", description);
        return 9;
    }
    if (SendMessageA(list, LB_SETCURSEL, 0, 0) == LB_ERR) return 10;
    SendMessageA(
        GetParent(list),
        WM_COMMAND,
        MAKEWPARAM(LIST_CONTROL_ID, LBN_SELCHANGE),
        (LPARAM)list
    );
    UpdateWindow(dialog.window);
    Sleep(100);

    WindowSearch preview = {process_id, "WindowObjectViewx", NULL, NULL, FALSE};
    EnumChildWindows(dialog.window, find_child, (LPARAM)&preview);
    RECT preview_rect = {0};
    if (!preview.window || !GetWindowRect(preview.window, &preview_rect)) return 11;
    LONG preview_width = preview_rect.right - preview_rect.left;
    LONG preview_height = preview_rect.bottom - preview_rect.top;
    if (preview_width <= 0 || preview_height <= 0 ||
        preview_width > 4096 || preview_height > 4096) return 11;
    SetForegroundWindow(dialog.window);
    BringWindowToTop(dialog.window);
    FILE *ready = fopen(argv[5], "wb");
    if (!ready) return 11;
    int ready_written = fprintf(
        ready, "%ld %ld %ld %ld\n",
        preview_rect.left, preview_rect.top, preview_width, preview_height
    );
    int ready_closed = fclose(ready);
    if (ready_written < 0 || ready_closed != 0) return 11;
    for (unsigned attempt = 0; attempt < 400; attempt++) {
        DWORD attributes = GetFileAttributesA(argv[6]);
        if (attributes != INVALID_FILE_ATTRIBUTES && !(attributes & FILE_ATTRIBUTE_DIRECTORY)) {
            break;
        }
        Sleep(25);
    }
    if (GetFileAttributesA(argv[6]) == INVALID_FILE_ATTRIBUTES) return 11;

    LPARAM point = MAKELPARAM((WORD)x, (WORD)y);
    SendMessageA(canvas.window, WM_RBUTTONDOWN, MK_RBUTTON, point);
    SendMessageA(canvas.window, WM_RBUTTONUP, 0, point);
    if (!PostMessageA(frame.window, WM_COMMAND, MAKEWPARAM(SAVE_LEVEL_COMMAND, 0), 0)) return 13;
    HWND warning = wait_for_window(
        process_id, "#32770", "Undefined Exits Detected!", 40
    );
    if (warning) {
        HWND yes = GetDlgItem(warning, IDYES);
        if (!yes) EnumChildWindows(warning, find_yes_button, (LPARAM)&yes);
        if (!yes) return 13;
        SendMessageA(yes, BM_CLICK, 0, 0);
        for (unsigned attempt = 0; attempt < 400 && IsWindow(warning); attempt++) {
            Sleep(25);
        }
        if (IsWindow(warning)) return 13;
    }
    Sleep(500);
    printf(
        "description=%s\npreview_rect=%ld,%ld,%ld,%ld\nplaced_at=%lu,%lu\n",
        description, preview_rect.left, preview_rect.top,
        preview_width, preview_height, x, y
    );
    PostMessageA(frame.window, WM_CLOSE, 0, 0);
    return 0;
}
