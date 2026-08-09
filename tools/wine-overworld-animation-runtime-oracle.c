#include <windows.h>
#include <tlhelp32.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static PROCESS_INFORMATION launched_process;

static void clean_up_launched_process(void) {
    if (launched_process.hProcess == NULL) {
        return;
    }
    if (WaitForSingleObject(launched_process.hProcess, 0) == WAIT_TIMEOUT) {
        TerminateProcess(launched_process.hProcess, 1);
        WaitForSingleObject(launched_process.hProcess, 5000);
    }
    CloseHandle(launched_process.hThread);
    CloseHandle(launched_process.hProcess);
    launched_process.hThread = NULL;
    launched_process.hProcess = NULL;
}

struct window_search {
    DWORD process_id;
    const char *class_name;
    const char *title;
    HWND window;
};

static BOOL CALLBACK find_window(HWND window, LPARAM opaque) {
    struct window_search *search = (struct window_search *)opaque;
    DWORD process_id = 0;
    char class_name[128] = {0};
    char title[256] = {0};
    GetWindowThreadProcessId(window, &process_id);
    if (search->process_id != 0 && process_id != search->process_id) {
        return TRUE;
    }
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    if (
        (search->class_name == NULL || _stricmp(class_name, search->class_name) == 0) &&
        (search->title == NULL || strcmp(title, search->title) == 0)
    ) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static HWND process_window(DWORD process_id, const char *class_name, const char *title) {
    struct window_search search = {
        .process_id = process_id,
        .class_name = class_name,
        .title = title,
        .window = NULL,
    };
    EnumWindows(find_window, (LPARAM)&search);
    return search.window;
}

static HWND wait_for_window(
    DWORD process_id,
    const char *class_name,
    const char *title,
    DWORD timeout_ms
) {
    for (DWORD elapsed = 0; elapsed < timeout_ms; elapsed += 25) {
        HWND window = process_window(process_id, class_name, title);
        if (window != NULL) {
            return window;
        }
        Sleep(25);
    }
    return NULL;
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

static BOOL read_process_u32(DWORD process_id, uintptr_t address, DWORD *value) {
    HANDLE process = OpenProcess(PROCESS_VM_READ, FALSE, process_id);
    SIZE_T read = 0;
    BOOL ok = process != NULL && ReadProcessMemory(
        process, (void *)address, value, sizeof(*value), &read
    );
    if (process != NULL) {
        CloseHandle(process);
    }
    return ok && read == sizeof(*value);
}

static int set_text(HWND dialog, int control_id, const char *text) {
    HWND control = GetDlgItem(dialog, control_id);
    if (control == NULL) {
        fprintf(stderr, "cannot set control 0x%04x\n", control_id);
        return 1;
    }
    SendMessageA(control, EM_SETSEL, 0, -1);
    for (const unsigned char *character = (const unsigned char *)text;
         *character != '\0'; character++) {
        SendMessageA(control, WM_CHAR, *character, 0);
    }
    return 0;
}

static int click_ok(HWND dialog) {
    HWND ok = GetDlgItem(dialog, IDOK);
    if (ok == NULL || !PostMessageA(
        dialog,
        WM_COMMAND,
        MAKEWPARAM(IDOK, BN_CLICKED),
        (LPARAM)ok
    )) {
        fprintf(stderr, "cannot submit dialog\n");
        return 1;
    }
    return 0;
}

static BOOL CALLBACK print_child(HWND window, LPARAM opaque) {
    (void)opaque;
    char class_name[128] = {0};
    char title[256] = {0};
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    printf(
        "child id=0x%04x class=%s title=%s\n",
        GetDlgCtrlID(window),
        class_name,
        title
    );
    return TRUE;
}

int main(int argc, char **argv) {
    if (argc < 2 || argc > 3) {
        fprintf(
            stderr,
            "usage: wine-overworld-animation-runtime-oracle.exe EXECUTABLE [ROM]\n"
        );
        return 2;
    }
    DWORD process_id = 0;
    if (argc == 3) {
        size_t command_length = strlen(argv[1]) + strlen(argv[2]) + 8;
        char *command = malloc(command_length);
        if (command == NULL) {
            fprintf(stderr, "cannot allocate Lunar Magic command line\n");
            return 1;
        }
        snprintf(command, command_length, "\"%s\" \"%s\"", argv[1], argv[2]);
        STARTUPINFOA startup = {.cb = sizeof(startup)};
        if (!CreateProcessA(
            argv[1], command, NULL, NULL, FALSE, 0, NULL, NULL, &startup,
            &launched_process
        )) {
            free(command);
            fprintf(stderr, "cannot start Lunar Magic: %lu\n", GetLastError());
            return 1;
        }
        free(command);
        atexit(clean_up_launched_process);
        process_id = launched_process.dwProcessId;
    } else {
        process_id = find_process(argv[1]);
    }
    HWND main_window = wait_for_window(process_id, "LMFrame", "Lunar Magic", 10000);
    if (main_window == NULL && argc == 2) {
        main_window = wait_for_window(0, "LMFrame", "Lunar Magic", 10000);
        if (main_window != NULL) {
            GetWindowThreadProcessId(main_window, &process_id);
        }
    }
    if (main_window == NULL) {
        fprintf(stderr, "Lunar Magic frame not ready\n");
        return 1;
    }
    printf("main-ready pid=%lu\n", (unsigned long)process_id);
    fflush(stdout);
    DWORD level = 0xffffffff;
    for (DWORD elapsed = 0; elapsed < 15000; elapsed += 25) {
        if (read_process_u32(process_id, 0x005e7738, &level) && level == 0x105) {
            break;
        }
        Sleep(25);
    }
    printf("level-ready value=0x%08lx\n", (unsigned long)level);
    fflush(stdout);
    if (level != 0x105) {
        fprintf(stderr, "level 105 did not finish loading\n");
        return 1;
    }
    Sleep(3000);
    ShowWindow(main_window, SW_RESTORE);
    BringWindowToTop(main_window);
    SetForegroundWindow(main_window);
    Sleep(500);
    HWND overworld = NULL;
    for (unsigned attempt = 0; attempt < 3 && overworld == NULL; attempt++) {
        PostMessageA(main_window, WM_COMMAND, MAKEWPARAM(0x232d, 0), 0);
        overworld = wait_for_window(process_id, "OVFrame", "Overworld Editor", 10000);
    }
    if (overworld == NULL) {
        fprintf(stderr, "overworld editor not ready\n");
        return 1;
    }
    printf("overworld-ready\n");
    fflush(stdout);
    if (!PostMessageA(overworld, WM_COMMAND, MAKEWPARAM(0x2530, 0), 0)) {
        fprintf(stderr, "cannot open overworld ExAnimation editor\n");
        return 1;
    }
    HWND animation = wait_for_window(
        process_id,
        "#32770",
        NULL,
        10000
    );
    if (animation == NULL) {
        fprintf(stderr, "ExAnimation editor not ready\n");
        return 1;
    }
    char animation_title[256] = {0};
    GetWindowTextA(animation, animation_title, sizeof(animation_title));
    printf("animation-ready title=%s\n", animation_title);
    fflush(stdout);
    Sleep(1000);
    HWND type = NULL;
    for (DWORD elapsed = 0; elapsed < 5000; elapsed += 25) {
        type = GetDlgItem(animation, 0x0066);
        if (type != NULL && SendMessageA(type, CB_GETCOUNT, 0, 0) > 1) {
            break;
        }
        Sleep(25);
    }
    if (type == NULL || SendMessageA(type, CB_SETCURSEL, 1, 0) == CB_ERR) {
        EnumChildWindows(animation, print_child, 0);
        fflush(stdout);
        fprintf(stderr, "cannot select ExAnimation type\n");
        return 1;
    }
    SendMessageA(
        animation,
        WM_COMMAND,
        MAKEWPARAM(0x0066, CBN_SELCHANGE),
        (LPARAM)type
    );
    if (
        set_text(animation, 0x0074, "00A0") ||
        set_text(animation, 0x0071, "00") ||
        set_text(animation, 0x0191, "0500") ||
        click_ok(animation)
    ) {
        return 1;
    }
    for (DWORD elapsed = 0; IsWindow(animation) && elapsed < 10000; elapsed += 25) {
        Sleep(25);
    }
    if (IsWindow(animation)) {
        fprintf(stderr, "ExAnimation editor did not close\n");
        return 1;
    }
    Sleep(2000);
    if (!PostMessageA(overworld, WM_COMMAND, MAKEWPARAM(0x1f40, 0), 0)) {
        fprintf(stderr, "cannot save overworld\n");
        return 1;
    }
    Sleep(1000);
    HWND save_prompt = process_window(process_id, "#32770", NULL);
    if (save_prompt != NULL) {
        char save_title[256] = {0};
        GetWindowTextA(save_prompt, save_title, sizeof(save_title));
        printf("save-dialog title=%s\n", save_title);
        fflush(stdout);
        HWND yes = GetDlgItem(save_prompt, IDYES);
        if (yes == NULL) {
            EnumChildWindows(save_prompt, print_child, 0);
            fflush(stdout);
            return 1;
        }
        SendMessageA(yes, BM_CLICK, 0, 0);
        for (DWORD elapsed = 0; IsWindow(save_prompt) && elapsed < 10000; elapsed += 25) {
            Sleep(25);
        }
        if (IsWindow(save_prompt)) {
            fprintf(stderr, "overworld-save confirmation did not close\n");
            return 1;
        }
        Sleep(1000);
        HWND follow_up = process_window(process_id, "#32770", NULL);
        if (follow_up != NULL) {
            char follow_up_title[256] = {0};
            GetWindowTextA(follow_up, follow_up_title, sizeof(follow_up_title));
            printf("save-follow-up title=%s\n", follow_up_title);
            EnumChildWindows(follow_up, print_child, 0);
            fflush(stdout);
            return 1;
        }
    }
    Sleep(9000);
    printf("domain=overworld type=1 destination=00A0 frames=00 source=0500 saved=1\n");
    return 0;
}
