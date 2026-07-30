#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <commctrl.h>
#include <stdint.h>
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

struct toolbar_button32 {
    int32_t bitmap;
    int32_t command;
    uint8_t state;
    uint8_t style;
    uint8_t reserved[2];
    uint32_t data;
    uint32_t string;
};

struct dialog_values {
    HANDLE process;
    void *remote;
};

static BOOL CALLBACK list_dialog_value(HWND window, LPARAM opaque) {
    struct dialog_values *values = (struct dialog_values *)opaque;
    char class_name[128] = {0};
    GetClassName(window, class_name, sizeof(class_name));
    if (_stricmp(class_name, "ComboBox") == 0) {
        LRESULT selected = SendMessage(window, CB_GETCURSEL, 0, 0);
        char text[256] = {0};
        SIZE_T read = 0;
        if (
            selected != CB_ERR &&
            SendMessage(window, CB_GETLBTEXT, selected, (LPARAM)values->remote) != CB_ERR
        ) {
            ReadProcessMemory(
                values->process,
                values->remote,
                text,
                sizeof(text) - 1,
                &read
            );
        }
        printf(
            "combo=0x%04lx selected=%ld text=%s\n",
            (unsigned long)GetDlgCtrlID(window),
            (long)selected,
            text
        );
    } else if (_stricmp(class_name, "Edit") == 0) {
        char text[256] = {0};
        SIZE_T read = 0;
        LRESULT length = SendMessage(
            window,
            WM_GETTEXT,
            sizeof(text),
            (LPARAM)values->remote
        );
        if (length > 0) {
            ReadProcessMemory(
                values->process,
                values->remote,
                text,
                sizeof(text) - 1,
                &read
            );
        }
        printf(
            "edit-parent=0x%04lx text=%s\n",
            (unsigned long)GetDlgCtrlID(GetParent(window)),
            text
        );
    } else if (
        _stricmp(class_name, "Button") == 0 &&
        GetDlgCtrlID(window) != IDOK &&
        GetDlgCtrlID(window) != IDCANCEL
    ) {
        char title[256] = {0};
        GetWindowText(window, title, sizeof(title));
        printf(
            "button=0x%04lx check=%ld title=%s\n",
            (unsigned long)GetDlgCtrlID(window),
            (long)SendMessage(window, BM_GETCHECK, 0, 0),
            title
        );
    }
    return TRUE;
}

static int list_dialog_values(HWND dialog, DWORD process_id) {
    HANDLE process = OpenProcess(
        PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE,
        FALSE,
        process_id
    );
    if (process == NULL) {
        fprintf(stderr, "cannot open dialog process\n");
        return 1;
    }
    void *remote = VirtualAllocEx(
        process,
        NULL,
        512,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE
    );
    if (remote == NULL) {
        CloseHandle(process);
        fprintf(stderr, "cannot allocate dialog exchange buffer\n");
        return 1;
    }
    struct dialog_values values = {.process = process, .remote = remote};
    EnumChildWindows(dialog, list_dialog_value, (LPARAM)&values);
    VirtualFreeEx(process, remote, 0, MEM_RELEASE);
    CloseHandle(process);
    return 0;
}

static BOOL CALLBACK find_toolbar(HWND window, LPARAM opaque) {
    HWND *found = (HWND *)opaque;
    char class_name[128] = {0};
    GetClassName(window, class_name, sizeof(class_name));
    if (_stricmp(class_name, TOOLBARCLASSNAME) == 0) {
        *found = window;
        return FALSE;
    }
    return TRUE;
}

static int list_toolbar_buttons(HWND parent, DWORD process_id) {
    HWND toolbar = NULL;
    EnumChildWindows(parent, find_toolbar, (LPARAM)&toolbar);
    if (toolbar == NULL) {
        fprintf(stderr, "toolbar not found\n");
        return 1;
    }
    HANDLE process = OpenProcess(
        PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE,
        FALSE,
        process_id
    );
    if (process == NULL) {
        fprintf(stderr, "cannot open toolbar process\n");
        return 1;
    }
    void *remote = VirtualAllocEx(
        process,
        NULL,
        512,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE
    );
    if (remote == NULL) {
        CloseHandle(process);
        fprintf(stderr, "cannot allocate toolbar exchange buffer\n");
        return 1;
    }
    LRESULT count = SendMessage(toolbar, TB_BUTTONCOUNT, 0, 0);
    for (LRESULT index = 0; index < count; index++) {
        struct toolbar_button32 button = {0};
        SIZE_T read = 0;
        if (
            SendMessage(toolbar, TB_GETBUTTON, index, (LPARAM)remote) &&
            ReadProcessMemory(
                process,
                remote,
                &button,
                sizeof(button),
                &read
            ) &&
            read == sizeof(button)
        ) {
            char text[256] = {0};
            LRESULT text_length = SendMessage(
                toolbar,
                TB_GETBUTTONTEXTA,
                (WPARAM)(uint32_t)button.command,
                (LPARAM)remote
            );
            if (text_length > 0) {
                ReadProcessMemory(
                    process,
                    remote,
                    text,
                    sizeof(text) - 1,
                    &read
                );
            }
            printf(
                "button=%ld command=0x%04lx bitmap=%ld state=0x%02x style=0x%02x text=%s\n",
                (long)index,
                (unsigned long)(uint32_t)button.command,
                (long)button.bitmap,
                button.state,
                button.style,
                text
            );
        }
    }
    VirtualFreeEx(process, remote, 0, MEM_RELEASE);
    CloseHandle(process);
    return 0;
}

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

static HWND find_process_window(DWORD process_id, const char *window_class) {
    struct search search = {
        .process_id = process_id,
        .window = NULL,
        .list = FALSE,
        .window_class = window_class
    };
    EnumWindows(find_top_level_window, (LPARAM)&search);
    return search.window;
}

static int open_level(DWORD process_id, const char *level_number) {
    HWND main_window = find_process_window(process_id, NULL);
    if (main_window == NULL) {
        fprintf(stderr, "main window not found for process %lu\n", process_id);
        return 1;
    }
    if (!PostMessage(main_window, WM_COMMAND, MAKEWPARAM(0x238e, 0), 0)) {
        fprintf(stderr, "cannot open level-number dialog\n");
        return 1;
    }

    HWND dialog = NULL;
    HWND edit = NULL;
    for (unsigned int attempt = 0; attempt < 200 && edit == NULL; attempt++) {
        Sleep(25);
        dialog = find_process_window(process_id, "#32770");
        if (dialog != NULL) {
            edit = GetDlgItem(dialog, 0x7f);
        }
    }
    if (edit == NULL) {
        fprintf(
            stderr,
            "level-number dialog was not ready within 5 seconds\n"
        );
        return 1;
    }
    SendMessage(edit, WM_SETTEXT, 0, (LPARAM)level_number);
    if (!PostMessage(
        dialog,
        WM_COMMAND,
        MAKEWPARAM(IDOK, BN_CLICKED),
        (LPARAM)GetDlgItem(dialog, IDOK)
    )) {
        fprintf(stderr, "cannot submit level-number dialog\n");
        return 1;
    }

    for (unsigned int attempt = 0; attempt < 400; attempt++) {
        Sleep(25);
        if (!IsWindow(dialog)) {
            return 0;
        }
    }
    fprintf(stderr, "level-number dialog did not close within 10 seconds\n");
    return 1;
}

int main(int argc, char **argv) {
    if (argc < 3 || argc > 4) {
        fprintf(
            stderr,
            "usage: wine-window-command.exe EXECUTABLE COMMAND_ID [WINDOW_CLASS]\n"
            "       wine-window-command.exe EXECUTABLE toolbar\n"
            "       wine-window-command.exe EXECUTABLE dialog-values\n"
            "       wine-window-command.exe EXECUTABLE click CONTROL_ID\n"
            "       wine-window-command.exe EXECUTABLE read ADDRESS,LENGTH\n"
            "       wine-window-command.exe EXECUTABLE write-byte ADDRESS,VALUE\n"
            "       wine-window-command.exe EXECUTABLE save WINDOWS_PATH\n"
            "       wine-window-command.exe EXECUTABLE level HEX_LEVEL\n"
            "       wine-window-command.exe EXECUTABLE open-level HEX_LEVEL\n"
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
    BOOL open_level_command = _stricmp(argv[2], "open-level") == 0;
    BOOL dialog_values = _stricmp(argv[2], "dialog-values") == 0;
    BOOL click = _stricmp(argv[2], "click") == 0;
    BOOL read = _stricmp(argv[2], "read") == 0;
    BOOL write_byte = _stricmp(argv[2], "write-byte") == 0;
    if (open_level_command) {
        if (argc != 4) {
            fprintf(stderr, "open-level requires a hexadecimal level number\n");
            return 2;
        }
        char *end = NULL;
        unsigned long level_value = strtoul(argv[3], &end, 16);
        if (
            end == argv[3] ||
            *end != '\0' ||
            level_value > 0x1ff
        ) {
            fprintf(stderr, "invalid hexadecimal level number: %s\n", argv[3]);
            return 2;
        }
        return open_level(process_id, argv[3]);
    }
    struct search search = {
        .process_id = process_id,
        .window = NULL,
        .list = _stricmp(argv[2], "list") == 0,
        .window_class = save || level || dialog_values || click
            ? "#32770"
            : (argc == 4 ? argv[3] : NULL)
    };
    EnumWindows(find_top_level_window, (LPARAM)&search);
    if (read) {
        if (argc != 4) {
            fprintf(stderr, "read requires ADDRESS,LENGTH\n");
            return 2;
        }
        char *separator = strchr(argv[3], ',');
        if (separator == NULL) {
            fprintf(stderr, "read requires ADDRESS,LENGTH\n");
            return 2;
        }
        *separator = '\0';
        char *address_end = NULL;
        char *length_end = NULL;
        unsigned long address = strtoul(argv[3], &address_end, 0);
        unsigned long length = strtoul(separator + 1, &length_end, 0);
        if (
            *address_end != '\0' ||
            *length_end != '\0' ||
            length == 0 ||
            length > 0x10000
        ) {
            fprintf(stderr, "invalid read range\n");
            return 2;
        }
        HANDLE process = OpenProcess(PROCESS_VM_READ, FALSE, process_id);
        if (process == NULL) {
            fprintf(stderr, "cannot open target process\n");
            return 1;
        }
        unsigned char *bytes = malloc(length);
        SIZE_T bytes_read = 0;
        BOOL ok = bytes != NULL && ReadProcessMemory(
            process,
            (void *)(uintptr_t)address,
            bytes,
            length,
            &bytes_read
        );
        if (ok) {
            for (SIZE_T index = 0; index < bytes_read; index++) {
                printf("%02x", bytes[index]);
            }
            putchar('\n');
        }
        free(bytes);
        CloseHandle(process);
        if (!ok || bytes_read != length) {
            fprintf(stderr, "cannot read requested range\n");
            return 1;
        }
        return 0;
    }
    if (write_byte) {
        if (argc != 4) {
            fprintf(stderr, "write-byte requires ADDRESS,VALUE\n");
            return 2;
        }
        char *separator = strchr(argv[3], ',');
        if (separator == NULL) {
            fprintf(stderr, "write-byte requires ADDRESS,VALUE\n");
            return 2;
        }
        *separator = '\0';
        char *address_end = NULL;
        char *value_end = NULL;
        unsigned long address = strtoul(argv[3], &address_end, 0);
        unsigned long value = strtoul(separator + 1, &value_end, 0);
        if (
            *address_end != '\0' ||
            *value_end != '\0' ||
            address == 0 ||
            value > 0xff
        ) {
            fprintf(stderr, "invalid byte write\n");
            return 2;
        }
        HANDLE process = OpenProcess(
            PROCESS_VM_OPERATION | PROCESS_VM_WRITE,
            FALSE,
            process_id
        );
        if (process == NULL) {
            fprintf(stderr, "cannot open target process for writing\n");
            return 1;
        }
        unsigned char byte = (unsigned char)value;
        SIZE_T bytes_written = 0;
        BOOL ok = WriteProcessMemory(
            process,
            (void *)(uintptr_t)address,
            &byte,
            sizeof(byte),
            &bytes_written
        );
        CloseHandle(process);
        if (!ok || bytes_written != sizeof(byte)) {
            fprintf(stderr, "cannot write requested byte\n");
            return 1;
        }
        return 0;
    }
    if (search.list) {
        return 0;
    }
    if (search.window == NULL) {
        fprintf(stderr, "top-level window not found for process %lu\n", process_id);
        return 1;
    }
    if (_stricmp(argv[2], "toolbar") == 0) {
        return list_toolbar_buttons(search.window, process_id);
    }
    if (dialog_values) {
        return list_dialog_values(search.window, process_id);
    }
    if (click) {
        if (argc != 4) {
            fprintf(stderr, "click requires a control id\n");
            return 2;
        }
        char *end = NULL;
        unsigned long control_id = strtoul(argv[3], &end, 0);
        if (end == argv[3] || *end != '\0' || control_id > 0xffff) {
            fprintf(stderr, "invalid control id: %s\n", argv[3]);
            return 2;
        }
        HWND control = GetDlgItem(search.window, (int)control_id);
        if (control == NULL) {
            fprintf(stderr, "dialog control not found: 0x%04lx\n", control_id);
            return 1;
        }
        SendMessage(control, BM_CLICK, 0, 0);
        return 0;
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
