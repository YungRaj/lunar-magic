#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tlhelp32.h>

#define CATEGORY_CONTROL_ID 0x01de
#define LIST_CONTROL_ID 0x0065
#define STANDARD_CATEGORY_INDEX 0
#define CUSTOM_CATEGORY_INDEX 4
#define ADD_SPRITES_COMMAND 0x2331
#define TOGGLE_SPRITE_EDITING_COMMAND 0x2459
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

static int read_list_text(HWND list, int index, char *output, size_t capacity) {
    wchar_t wide[1024] = {0};
    LRESULT length = SendMessageW(list, LB_GETTEXT, index, (LPARAM)wide);
    return length != LB_ERR && (size_t)length < sizeof(wide) / sizeof(wide[0]) &&
        WideCharToMultiByte(CP_UTF8, 0, wide, -1, output, (int)capacity, NULL, NULL) != 0;
}

int main(int argc, char **argv) {
    if (argc != 7) {
        fputs(
            "usage: wine-custom-sprite-oracle.exe EXECUTABLE EXPECTED_DESCRIPTION|--standard-first|--expect-empty X Y READY_FILE CONTINUE_FILE\n",
            stderr
        );
        return 2;
    }
    char *x_end = NULL;
    char *y_end = NULL;
    unsigned long x = strtoul(argv[3], &x_end, 0);
    unsigned long y = strtoul(argv[4], &y_end, 0);
    if (!x_end || *x_end || !y_end || *y_end || x > 0xffff || y > 0xffff) return 2;
    BOOL expect_empty = strcmp(argv[2], "--expect-empty") == 0;
    BOOL expect_standard = strcmp(argv[2], "--standard-first") == 0;

    DWORD process_id = 0;
    for (unsigned attempt = 0; attempt < 400 && !process_id; attempt++) {
        process_id = find_process(argv[1]);
        if (!process_id) Sleep(25);
    }
    if (!process_id) return 3;
    HWND frame = wait_for_window(process_id, "LMFrame", NULL, 400);
    if (!frame) {
        fputs("Lunar Magic frame unavailable\n", stderr);
        return 4;
    }
    WindowSearch canvas = {process_id, "SMWLevelEditor", NULL, NULL, FALSE};
    for (unsigned attempt = 0; attempt < 400 && !canvas.window; attempt++) {
        EnumChildWindows(frame, find_child, (LPARAM)&canvas);
        if (!canvas.window) Sleep(25);
    }
    if (!canvas.window) {
        fputs("level canvas unavailable\n", stderr);
        return 4;
    }
    if (!expect_empty) {
        if (!PostMessageA(
                canvas.window, WM_COMMAND,
                MAKEWPARAM(TOGGLE_SPRITE_EDITING_COMMAND, 0), 0
            )) {
            fputs("cannot activate sprite editing mode\n", stderr);
            return 4;
        }
        Sleep(100);
    }
    HWND dialog = NULL;
    for (unsigned attempt = 0; attempt < 1200; attempt++) {
        if (attempt % 200 == 0 &&
            !PostMessageA(canvas.window, WM_COMMAND, MAKEWPARAM(ADD_SPRITES_COMMAND, 0), 0)) {
            fputs("cannot open Add Sprites window\n", stderr);
            return 4;
        }
        dialog = wait_for_window(process_id, NULL, "Add Sprites Window", 1);
        if (dialog && IsWindowVisible(dialog)) break;
        Sleep(25);
    }
    if (!dialog || !IsWindowVisible(dialog)) {
        fputs("Add Sprites window unavailable\n", stderr);
        return 4;
    }

    HWND category = descendant_control(dialog, CATEGORY_CONTROL_ID);
    HWND list = descendant_control(dialog, LIST_CONTROL_ID);
    if (!category || !list) return 5;
    int category_index = expect_standard ? STANDARD_CATEGORY_INDEX : CUSTOM_CATEGORY_INDEX;
    if (SendMessageA(category, CB_SETCURSEL, category_index, 0) == CB_ERR) return 6;
    SendMessageA(
        GetParent(category), WM_COMMAND,
        MAKEWPARAM(CATEGORY_CONTROL_ID, CBN_SELCHANGE), (LPARAM)category
    );
    if (SendMessageA(category, CB_GETCURSEL, 0, 0) != category_index) return 7;
    LRESULT list_count = LB_ERR;
    for (unsigned attempt = 0; attempt < 400; attempt++) {
        list_count = SendMessageA(list, LB_GETCOUNT, 0, 0);
        if (expect_standard ? list_count > 0 : list_count == (expect_empty ? 0 : 1)) break;
        Sleep(25);
    }
    if (expect_standard ? list_count <= 0 : list_count != (expect_empty ? 0 : 1)) {
        fprintf(stderr, "unexpected custom sprite list count: %ld\n", (long)list_count);
        return 7;
    }
    if (expect_empty) {
        puts("incomplete_description_entries=0");
        PostMessageA(frame, WM_CLOSE, 0, 0);
        return 0;
    }
    char description[1024] = {0};
    if (!read_list_text(list, 0, description, sizeof(description))) return 8;
    if (expect_standard && description[0] == '\0') return 9;
    if (!expect_standard && strcmp(description, argv[2]) != 0) {
        fprintf(stderr, "unexpected custom sprite description: %s\n", description);
        return 9;
    }
    if (SendMessageA(list, LB_SETCURSEL, 0, 0) == LB_ERR) return 10;
    SendMessageA(
        GetParent(list), WM_COMMAND,
        MAKEWPARAM(LIST_CONTROL_ID, LBN_SELCHANGE), (LPARAM)list
    );
    UpdateWindow(dialog);
    Sleep(100);

    WindowSearch preview = {process_id, "WindowSpriteViewx", NULL, NULL, FALSE};
    EnumChildWindows(dialog, find_child, (LPARAM)&preview);
    RECT preview_rect = {0};
    if (!preview.window || !GetWindowRect(preview.window, &preview_rect)) return 11;
    LONG preview_width = preview_rect.right - preview_rect.left;
    LONG preview_height = preview_rect.bottom - preview_rect.top;
    if (preview_width <= 0 || preview_height <= 0 ||
        preview_width > 4096 || preview_height > 4096) return 11;
    SetForegroundWindow(dialog);
    BringWindowToTop(dialog);
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
        if (attributes != INVALID_FILE_ATTRIBUTES && !(attributes & FILE_ATTRIBUTE_DIRECTORY)) break;
        Sleep(25);
    }
    if (GetFileAttributesA(argv[6]) == INVALID_FILE_ATTRIBUTES) return 11;

    LPARAM point = MAKELPARAM((WORD)x, (WORD)y);
    keybd_event(VK_CONTROL, 0, 0, 0);
    Sleep(25);
    SendMessageA(canvas.window, WM_RBUTTONDOWN, MK_CONTROL | MK_RBUTTON, point);
    SendMessageA(canvas.window, WM_RBUTTONUP, MK_CONTROL, point);
    keybd_event(VK_CONTROL, 0, KEYEVENTF_KEYUP, 0);
    if (!PostMessageA(frame, WM_COMMAND, MAKEWPARAM(SAVE_LEVEL_COMMAND, 0), 0)) return 13;
    Sleep(500);
    printf(
        "description=%s\npreview_rect=%ld,%ld,%ld,%ld\nplaced_at=%lu,%lu\n",
        description, preview_rect.left, preview_rect.top,
        preview_width, preview_height, x, y
    );
    PostMessageA(frame, WM_CLOSE, 0, 0);
    return 0;
}
