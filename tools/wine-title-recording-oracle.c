#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct window_search {
    DWORD process_id;
    HWND frame;
    const char *class_name;
};

struct title_search {
    DWORD process_id;
    const char *title;
    HWND window;
};

static BOOL CALLBACK find_frame(HWND window, LPARAM opaque) {
    struct window_search *search = (struct window_search *)opaque;
    DWORD process_id = 0;
    char class_name[64] = {0};
    GetWindowThreadProcessId(window, &process_id);
    GetClassNameA(window, class_name, sizeof(class_name));
    if (process_id == search->process_id &&
        _stricmp(class_name, search->class_name) == 0) {
        search->frame = window;
        return FALSE;
    }
    return TRUE;
}

static HWND wait_for_frame(DWORD process_id) {
    for (unsigned attempt = 0; attempt < 400; attempt++) {
        struct window_search search = {
            .process_id = process_id,
            .frame = NULL,
            .class_name = "LMFrame",
        };
        EnumWindows(find_frame, (LPARAM)&search);
        if (search.frame != NULL) {
            return search.frame;
        }
        Sleep(25);
    }
    return NULL;
}

static HWND find_process_window(DWORD process_id, const char *class_name) {
    struct window_search search = {
        .process_id = process_id,
        .frame = NULL,
        .class_name = class_name,
    };
    EnumWindows(find_frame, (LPARAM)&search);
    return search.frame;
}

static HWND wait_for_process_window(DWORD process_id, const char *class_name) {
    for (unsigned attempt = 0; attempt < 200; attempt++) {
        HWND window = find_process_window(process_id, class_name);
        if (window != NULL) {
            return window;
        }
        Sleep(25);
    }
    return NULL;
}

static BOOL CALLBACK find_title(HWND window, LPARAM opaque) {
    struct title_search *search = (struct title_search *)opaque;
    DWORD process_id = 0;
    char title[4096] = {0};
    GetWindowThreadProcessId(window, &process_id);
    if (process_id != search->process_id) {
        return TRUE;
    }
    GetWindowTextA(window, title, sizeof(title));
    if (strcmp(title, search->title) == 0) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static HWND wait_for_title(DWORD process_id, const char *title, unsigned attempts) {
    for (unsigned attempt = 0; attempt < attempts; attempt++) {
        struct title_search search = {
            .process_id = process_id,
            .title = title,
            .window = NULL,
        };
        EnumWindows(find_title, (LPARAM)&search);
        if (search.window != NULL) {
            return search.window;
        }
        Sleep(25);
    }
    return NULL;
}

static void list_menu(HMENU menu, unsigned depth) {
    int count = GetMenuItemCount(menu);
    for (int position = 0; position < count; position++) {
        char title[512] = {0};
        UINT command = GetMenuItemID(menu, position);
        GetMenuStringA(menu, (UINT)position, title, sizeof(title), MF_BYPOSITION);
        for (unsigned indent = 0; indent < depth; indent++) {
            fputs("  ", stdout);
        }
        if (command == (UINT)-1) {
            printf("submenu title=%s\n", title);
        } else {
            printf("command=0x%04x title=%s\n", command, title);
        }
        HMENU child = GetSubMenu(menu, position);
        if (child != NULL) {
            list_menu(child, depth + 1);
        }
    }
}

static BOOL CALLBACK list_child(HWND window, LPARAM opaque) {
    (void)opaque;
    char class_name[128] = {0};
    char title[512] = {0};
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    printf(
        "child=0x%p id=0x%04lx class=%s title=%s\n",
        window,
        (unsigned long)GetDlgCtrlID(window),
        class_name,
        title
    );
    return TRUE;
}

static BOOL CALLBACK list_process_window(HWND window, LPARAM opaque) {
    DWORD expected_process_id = *(DWORD *)opaque;
    DWORD process_id = 0;
    char class_name[128] = {0};
    char title[512] = {0};
    GetWindowThreadProcessId(window, &process_id);
    if (process_id != expected_process_id) {
        return TRUE;
    }
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    printf("window=0x%p class=%s title=%s\n", window, class_name, title);
    EnumChildWindows(window, list_child, 0);
    return TRUE;
}

int main(int argc, char **argv) {
    if (argc < 3 || argc > 5) {
        fprintf(
            stderr,
            "usage: wine-title-recording-oracle.exe LUNAR_MAGIC_EXE ROM "
            "[COMMAND [accept|cancel]]\n"
        );
        return 2;
    }
    size_t command_len = strlen(argv[1]) + strlen(argv[2]) + 8;
    char *command = malloc(command_len);
    if (command == NULL) {
        fprintf(stderr, "cannot allocate command line\n");
        return 1;
    }
    snprintf(command, command_len, "\"%s\" \"%s\"", argv[1], argv[2]);
    STARTUPINFOA startup = {.cb = sizeof(startup)};
    PROCESS_INFORMATION process = {0};
    BOOL started = CreateProcessA(
        argv[1], command, NULL, NULL, FALSE, 0, NULL, NULL, &startup, &process
    );
    free(command);
    if (!started) {
        fprintf(stderr, "cannot start Lunar Magic: %lu\n", GetLastError());
        return 1;
    }
    HWND frame = wait_for_frame(process.dwProcessId);
    if (frame == NULL) {
        fprintf(stderr, "Lunar Magic frame was not ready within 10 seconds\n");
        TerminateProcess(process.hProcess, 1);
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
        return 1;
    }
    Sleep(3000);
    HWND overworld = NULL;
    for (unsigned attempt = 0; attempt < 3 && overworld == NULL; attempt++) {
        PostMessageA(frame, WM_COMMAND, 0x232d, 0);
        overworld = wait_for_process_window(process.dwProcessId, "OVFrame");
    }
    if (overworld == NULL) {
        fprintf(stderr, "Overworld Editor did not open\n");
        EnumWindows(list_process_window, (LPARAM)&process.dwProcessId);
        fflush(stdout);
        TerminateProcess(process.hProcess, 1);
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
        return 1;
    }
    if (argc >= 4) {
        char *end = NULL;
        unsigned long command_id = strtoul(argv[3], &end, 0);
        if (end == argv[3] || *end != '\0' || command_id > 0xffff) {
            fprintf(stderr, "invalid command: %s\n", argv[3]);
            TerminateProcess(process.hProcess, 1);
            CloseHandle(process.hThread);
            CloseHandle(process.hProcess);
            return 2;
        }
        printf("posting command=0x%04lx to Overworld Editor\n", command_id);
        PostMessageA(overworld, WM_COMMAND, (WPARAM)command_id, 0);
        if (argc == 5) {
            HWND dialog = wait_for_process_window(process.dwProcessId, "#32770");
            if (dialog == NULL) {
                fprintf(stderr, "command did not open a dialog\n");
                TerminateProcess(process.hProcess, 1);
                CloseHandle(process.hThread);
                CloseHandle(process.hProcess);
                return 1;
            }
            int button_id = 0;
            if (_stricmp(argv[4], "accept") == 0) {
                button_id = IDOK;
            } else if (_stricmp(argv[4], "cancel") == 0) {
                button_id = IDCANCEL;
            } else {
                fprintf(stderr, "decision must be accept or cancel\n");
                TerminateProcess(process.hProcess, 1);
                CloseHandle(process.hThread);
                CloseHandle(process.hProcess);
                return 2;
            }
            HWND button = GetDlgItem(dialog, button_id);
            if (button == NULL) {
                fprintf(stderr, "dialog does not expose requested button %d\n", button_id);
                TerminateProcess(process.hProcess, 1);
                CloseHandle(process.hThread);
                CloseHandle(process.hProcess);
                return 1;
            }
            printf("clicking dialog button=%d\n", button_id);
            fflush(stdout);
            SendMessageA(button, BM_CLICK, 0, 0);
            if (button_id == IDOK && command_id == 0x1f46) {
                HWND expansion = wait_for_title(
                    process.dwProcessId,
                    "Not enough room...!",
                    160
                );
                if (expansion != NULL) {
                    HWND yes = GetDlgItem(expansion, IDYES);
                    if (yes == NULL) {
                        fprintf(stderr, "expansion prompt has no Yes button\n");
                        TerminateProcess(process.hProcess, 1);
                        CloseHandle(process.hThread);
                        CloseHandle(process.hProcess);
                        return 1;
                    }
                    puts("clicking expansion prompt button=6");
                    fflush(stdout);
                    SendMessageA(yes, BM_CLICK, 0, 0);
                }
            }
            if (button_id == IDOK &&
                (command_id == 0x1f46 || command_id == 0x1f47)) {
                Sleep(2000);
                puts("posting command=0x1f40 to save Overworld Editor state");
                fflush(stdout);
                PostMessageA(overworld, WM_COMMAND, 0x1f40, 0);
            }
        }
        Sleep(2000);
    }
    EnumWindows(list_process_window, (LPARAM)&process.dwProcessId);
    HMENU menu = GetMenu(frame);
    if (menu == NULL) {
        puts("menu=none");
    } else {
        list_menu(menu, 0);
    }
    PostMessageA(frame, WM_CLOSE, 0, 0);
    if (WaitForSingleObject(process.hProcess, 5000) == WAIT_TIMEOUT) {
        TerminateProcess(process.hProcess, 1);
        WaitForSingleObject(process.hProcess, 5000);
    }
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    return 0;
}
