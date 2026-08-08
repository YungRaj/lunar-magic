#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <tlhelp32.h>

struct editor {
    DWORD process_id;
    HWND dialog;
    HWND viewer;
};

struct editor_state {
    char page[32];
    char acts_like[32];
    char top_left[32];
    char top_right[32];
    char bottom_left[32];
    char bottom_right[32];
    LRESULT palette;
    LRESULT priority;
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

static BOOL CALLBACK find_dialog(HWND window, LPARAM opaque) {
    struct editor *editor = (struct editor *)opaque;
    DWORD process_id = 0;
    char class_name[128] = {0};
    char title[128] = {0};
    GetWindowThreadProcessId(window, &process_id);
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    if (
        process_id == editor->process_id &&
        strcmp(class_name, "#32770") == 0 &&
        strcmp(title, "16x16 Tile Map Editor") == 0
    ) {
        editor->dialog = window;
        return FALSE;
    }
    return TRUE;
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

static void read_text(HWND dialog, int control_id, char output[32]) {
    SendMessageA(GetDlgItem(dialog, control_id), WM_GETTEXT, 32, (LPARAM)output);
}

static struct editor_state capture_state(struct editor *editor) {
    struct editor_state state = {0};
    read_text(editor->dialog, 0x74, state.page);
    read_text(editor->dialog, 0x66, state.acts_like);
    read_text(editor->dialog, 0x1da, state.top_left);
    read_text(editor->dialog, 0x1db, state.top_right);
    read_text(editor->dialog, 0x1dc, state.bottom_left);
    read_text(editor->dialog, 0x1dd, state.bottom_right);
    state.palette = SendMessageA(GetDlgItem(editor->dialog, 0x6c), CB_GETCURSEL, 0, 0);
    state.priority = SendMessageA(GetDlgItem(editor->dialog, 0x6b), CB_GETCURSEL, 0, 0);
    return state;
}

static int states_equal(const struct editor_state *left, const struct editor_state *right) {
    return
        strcmp(left->page, right->page) == 0 &&
        strcmp(left->acts_like, right->acts_like) == 0 &&
        strcmp(left->top_left, right->top_left) == 0 &&
        strcmp(left->top_right, right->top_right) == 0 &&
        strcmp(left->bottom_left, right->bottom_left) == 0 &&
        strcmp(left->bottom_right, right->bottom_right) == 0 &&
        left->palette == right->palette &&
        left->priority == right->priority;
}

static void print_state(FILE *output, const char *label, const struct editor_state *state) {
    fprintf(
        output,
        "%s page=%s acts=%s subtiles=%s,%s,%s,%s palette=%ld priority=%ld\n",
        label,
        state->page,
        state->acts_like,
        state->top_left,
        state->top_right,
        state->bottom_left,
        state->bottom_right,
        (long)state->palette,
        (long)state->priority
    );
}

static void set_edit(struct editor *editor, int control_id, const char *value) {
    HWND control = GetDlgItem(editor->dialog, control_id);
    SendMessageA(control, WM_SETTEXT, 0, (LPARAM)value);
    SendMessageA(
        editor->dialog,
        WM_COMMAND,
        MAKEWPARAM(control_id, EN_CHANGE),
        (LPARAM)control
    );
    SendMessageA(
        editor->dialog,
        WM_COMMAND,
        MAKEWPARAM(control_id, EN_KILLFOCUS),
        (LPARAM)control
    );
    Sleep(50);
}

static void set_combo(struct editor *editor, int control_id, int index) {
    HWND control = GetDlgItem(editor->dialog, control_id);
    SendMessageA(control, CB_SETCURSEL, index, 0);
    SendMessageA(
        editor->dialog,
        WM_COMMAND,
        MAKEWPARAM(control_id, CBN_SELCHANGE),
        (LPARAM)control
    );
    SendMessageA(
        editor->dialog,
        WM_COMMAND,
        MAKEWPARAM(control_id, CBN_SELENDOK),
        (LPARAM)control
    );
    Sleep(50);
}

static void reselect_visible_tile(struct editor *editor) {
    HWND mode = GetDlgItem(editor->dialog, 0x1d9);
    if (SendMessageA(mode, BM_GETCHECK, 0, 0) != BST_CHECKED) {
        SendMessageA(mode, BM_CLICK, 0, 0);
    }
    SetFocus(editor->viewer);
    SendMessageA(editor->viewer, WM_MOUSEMOVE, 0, MAKELPARAM(4, 4));
    Sleep(50);
    SendMessageA(editor->viewer, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(4, 4));
    SendMessageA(editor->viewer, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(12, 12));
    SendMessageA(editor->viewer, WM_LBUTTONUP, 0, MAKELPARAM(12, 12));
    SendMessageA(mode, BM_CLICK, 0, 0);
    SendMessageA(editor->viewer, WM_MOUSEMOVE, 0, MAKELPARAM(8, 8));
    SendMessageA(editor->viewer, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(8, 8));
    SendMessageA(editor->viewer, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(9, 9));
    SendMessageA(editor->viewer, WM_LBUTTONUP, 0, MAKELPARAM(9, 9));
    Sleep(100);
}

static int select_tile_1f0(struct editor *editor) {
    set_edit(editor, 0x74, "02");
    SendMessageA(GetDlgItem(editor->dialog, 0x73), BM_CLICK, 0, 0);
    SendMessageA(GetDlgItem(editor->dialog, 0x74), WM_KEYDOWN, VK_RETURN, 0);
    SendMessageA(GetDlgItem(editor->dialog, 0x74), WM_KEYUP, VK_RETURN, 0xc0000000);
    reselect_visible_tile(editor);
    char page[32] = {0};
    char acts_like[32] = {0};
    char top_left[32] = {0};
    char top_right[32] = {0};
    char bottom_left[32] = {0};
    char bottom_right[32] = {0};
    read_text(editor->dialog, 0x74, page);
    read_text(editor->dialog, 0x66, acts_like);
    read_text(editor->dialog, 0x1da, top_left);
    read_text(editor->dialog, 0x1db, top_right);
    read_text(editor->dialog, 0x1dc, bottom_left);
    read_text(editor->dialog, 0x1dd, bottom_right);
    int selected =
        strcmp(page, "02") == 0 &&
        strcmp(acts_like, "1F0") == 0 &&
        strcmp(top_left, "192") == 0 &&
        strcmp(top_right, "193") == 0 &&
        strcmp(bottom_left, "194") == 0 &&
        strcmp(bottom_right, "195") == 0;
    if (!selected) {
        fprintf(
            stderr,
            "selection page=%s acts=%s tl=%s tr=%s bl=%s br=%s\n",
            page,
            acts_like,
            top_left,
            top_right,
            bottom_left,
            bottom_right
        );
    }
    return selected;
}

static unsigned click_history_until_disabled(struct editor *editor, int control_id) {
    HWND control = GetDlgItem(editor->dialog, control_id);
    unsigned count = 0;
    while (IsWindowEnabled(control) && count < 64) {
        SendMessageA(control, BM_CLICK, 0, 0);
        Sleep(50);
        count++;
    }
    return count;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: wine-map16-editor-oracle.exe PROCESS\n");
        return 2;
    }
    struct editor editor = {.process_id = find_process(argv[1])};
    EnumWindows(find_dialog, (LPARAM)&editor);
    if (editor.dialog == NULL) {
        fprintf(stderr, "visible 16x16 Tile Map Editor dialog not found\n");
        return 1;
    }
    EnumChildWindows(editor.dialog, find_viewer, (LPARAM)&editor);
    if (editor.viewer == NULL) {
        fprintf(stderr, "Window16x16view not found\n");
        return 1;
    }
    int initial_undo = IsWindowEnabled(GetDlgItem(editor.dialog, 0x2279));
    int initial_redo = IsWindowEnabled(GetDlgItem(editor.dialog, 0x227a));
    if (initial_undo || initial_redo || !select_tile_1f0(&editor)) {
        fprintf(stderr, "initial undo=%d redo=%d\n", initial_undo, initial_redo);
        fprintf(stderr, "Map16 editor did not begin at the expected clean tile $1F0\n");
        return 1;
    }

    struct editor_state initial = capture_state(&editor);

    set_edit(&editor, 0x1da, "123");
    set_edit(&editor, 0x1db, "234");
    set_edit(&editor, 0x1dc, "345");
    set_edit(&editor, 0x1dd, "056");
    set_edit(&editor, 0x66, "130");
    set_combo(&editor, 0x6c, 4);
    set_combo(&editor, 0x6b, 2);
    SendMessageA(GetDlgItem(editor.dialog, 0x28a), BM_CLICK, 0, 0);
    SendMessageA(GetDlgItem(editor.dialog, 0x28b), BM_CLICK, 0, 0);
    Sleep(100);
    struct editor_state modified = capture_state(&editor);

    unsigned undo_count = click_history_until_disabled(&editor, 0x2279);
    reselect_visible_tile(&editor);
    struct editor_state undone = capture_state(&editor);
    unsigned redo_count = click_history_until_disabled(&editor, 0x227a);
    reselect_visible_tile(&editor);
    struct editor_state redone = capture_state(&editor);
    if (
        undo_count != 9 || redo_count != 9 ||
        states_equal(&initial, &modified) ||
        !states_equal(&initial, &undone) ||
        !states_equal(&modified, &redone)
    ) {
        fprintf(stderr, "undo=%u redo=%u\n", undo_count, redo_count);
        print_state(stderr, "initial", &initial);
        print_state(stderr, "modified", &modified);
        print_state(stderr, "undone", &undone);
        print_state(stderr, "redone", &redone);
        fprintf(stderr, "Map16 edit history did not restore both exact states\n");
        return 1;
    }

    printf("field\tvalue\n");
    printf("page\t02\n");
    printf("tile\t200\n");
    printf("initial_subtiles\t%s,%s,%s,%s\n", initial.top_left, initial.top_right,
        initial.bottom_left, initial.bottom_right);
    printf("initial_acts_like\t%s\n", initial.acts_like);
    printf("initial_palette_index\t%ld\n", (long)initial.palette);
    printf("initial_priority_index\t%ld\n", (long)initial.priority);
    printf("modified_subtiles\t%s,%s,%s,%s\n", modified.top_left, modified.top_right,
        modified.bottom_left, modified.bottom_right);
    printf("modified_acts_like\t%s\n", modified.acts_like);
    printf("modified_palette_index\t%ld\n", (long)modified.palette);
    printf("modified_priority_index\t%ld\n", (long)modified.priority);
    printf("undo_steps\t%u\n", undo_count);
    printf("undo_restored_initial\t1\n");
    printf("redo_steps\t%u\n", redo_count);
    printf("redo_restored_modified\t1\n");
    return 0;
}
