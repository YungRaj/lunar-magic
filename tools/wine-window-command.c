#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <commctrl.h>
#include <limits.h>
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

struct title_search {
    DWORD process_id;
    HWND window;
    const char *title;
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

struct child_control_search {
    int control_id;
    const char *window_class;
    HWND window;
};

static BOOL CALLBACK find_child_control(HWND window, LPARAM opaque) {
    struct child_control_search *search = (struct child_control_search *)opaque;
    char class_name[128] = {0};
    GetClassName(window, class_name, sizeof(class_name));
    if (
        GetDlgCtrlID(window) == search->control_id &&
        _stricmp(class_name, search->window_class) == 0
    ) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static HWND read_process_window_handle(DWORD process_id, uintptr_t address) {
    HANDLE process = OpenProcess(PROCESS_VM_READ, FALSE, process_id);
    HWND window = NULL;
    SIZE_T bytes_read = 0;
    BOOL ok = process != NULL && ReadProcessMemory(
        process,
        (void *)address,
        &window,
        sizeof(window),
        &bytes_read
    );
    if (process != NULL) {
        CloseHandle(process);
    }
    if (!ok || bytes_read != sizeof(window) || !IsWindow(window)) {
        return NULL;
    }
    return window;
}

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
            "button=0x%04lx check=%ld enabled=%d visible=%d title=%s\n",
            (unsigned long)GetDlgCtrlID(window),
            (long)SendMessage(window, BM_GETCHECK, 0, 0),
            IsWindowEnabled(window),
            IsWindowVisible(window),
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

static void list_menu_items(HMENU menu, unsigned int depth) {
    int count = GetMenuItemCount(menu);
    for (int position = 0; position < count; position++) {
        char title[512] = {0};
        GetMenuStringA(menu, (UINT)position, title, sizeof(title), MF_BYPOSITION);
        UINT command = GetMenuItemID(menu, position);
        for (unsigned int indent = 0; indent < depth; indent++) {
            fputs("  ", stdout);
        }
        if (command == (UINT)-1) {
            printf("submenu title=%s\n", title);
        } else {
            printf("command=0x%04x title=%s\n", command, title);
        }
        HMENU child = GetSubMenu(menu, position);
        if (child != NULL) {
            list_menu_items(child, depth + 1);
        }
    }
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

static BOOL CALLBACK find_top_level_window_by_title(HWND window, LPARAM opaque) {
    struct title_search *search = (struct title_search *)opaque;
    DWORD process_id = 0;
    char title[512] = {0};
    GetWindowThreadProcessId(window, &process_id);
    if (process_id != search->process_id) {
        return TRUE;
    }
    GetWindowText(window, title, sizeof(title));
    if (strcmp(title, search->title) == 0) {
        search->window = window;
        return FALSE;
    }
    return TRUE;
}

static HWND find_process_window_by_title(DWORD process_id, const char *title) {
    struct title_search search = {
        .process_id = process_id,
        .window = NULL,
        .title = title
    };
    EnumWindows(find_top_level_window_by_title, (LPARAM)&search);
    return search.window;
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
            "       wine-window-command.exe EXECUTABLE menu\n"
            "       wine-window-command.exe EXECUTABLE dialog-values\n"
            "       wine-window-command.exe EXECUTABLE children\n"
            "       wine-window-command.exe EXECUTABLE click CONTROL_ID\n"
            "       wine-window-command.exe EXECUTABLE set-text CONTROL_ID,TEXT\n"
            "       wine-window-command.exe EXECUTABLE select CONTROL_ID,INDEX\n"
            "       wine-window-command.exe EXECUTABLE clipboard-bmp WINDOWS_PATH\n"
            "       wine-window-command.exe EXECUTABLE clipboard-bmp-paste WINDOWS_PATH\n"
            "       wine-window-command.exe EXECUTABLE command-at HWND_ADDRESS,COMMAND_ID\n"
            "       wine-window-command.exe EXECUTABLE post-command COMMAND_ID [WINDOW_CLASS]\n"
            "       wine-window-command.exe EXECUTABLE read ADDRESS,LENGTH\n"
            "       wine-window-command.exe EXECUTABLE find-u32 VALUE\n"
            "       wine-window-command.exe EXECUTABLE write-byte ADDRESS,VALUE\n"
            "       wine-window-command.exe EXECUTABLE slot-oracle PRIMARY,ALTERNATE,COMPOSITION,SPLIT\n"
            "       wine-window-command.exe EXECUTABLE slot-oracle-expanded PRIMARY,ALTERNATE,COMPOSITION,SPLIT,ROUTE,ADDITIVE\n"
            "       wine-window-command.exe EXECUTABLE key VIRTUAL_KEY\n"
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
    BOOL menu = _stricmp(argv[2], "menu") == 0;
    BOOL level = _stricmp(argv[2], "level") == 0;
    BOOL open_level_command = _stricmp(argv[2], "open-level") == 0;
    BOOL dialog_values = _stricmp(argv[2], "dialog-values") == 0;
    BOOL children = _stricmp(argv[2], "children") == 0;
    BOOL click = _stricmp(argv[2], "click") == 0;
    BOOL set_text = _stricmp(argv[2], "set-text") == 0;
    BOOL select_combo = _stricmp(argv[2], "select") == 0;
    BOOL clipboard_bmp = _stricmp(argv[2], "clipboard-bmp") == 0;
    BOOL clipboard_bmp_paste = _stricmp(argv[2], "clipboard-bmp-paste") == 0;
    BOOL command_at = _stricmp(argv[2], "command-at") == 0;
    BOOL post_command = _stricmp(argv[2], "post-command") == 0;
    BOOL read = _stricmp(argv[2], "read") == 0;
    BOOL find_u32 = _stricmp(argv[2], "find-u32") == 0;
    BOOL write_byte = _stricmp(argv[2], "write-byte") == 0;
    BOOL slot_oracle = _stricmp(argv[2], "slot-oracle") == 0;
    BOOL slot_oracle_expanded = _stricmp(argv[2], "slot-oracle-expanded") == 0;
    BOOL key = _stricmp(argv[2], "key") == 0;
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
        .window_class = save || level || dialog_values || children || click || set_text || select_combo
            ? "#32770"
            : (menu || post_command || clipboard_bmp || clipboard_bmp_paste || key
                ? "LMFrame"
                : (argc == 4 ? argv[3] : NULL))
    };
    EnumWindows(find_top_level_window, (LPARAM)&search);
    if (find_u32) {
        if (argc != 4) {
            fprintf(stderr, "find-u32 requires VALUE\n");
            return 2;
        }
        char *end = NULL;
        unsigned long value = strtoul(argv[3], &end, 0);
        if (end == argv[3] || *end != '\0' || value > UINT32_MAX) {
            fprintf(stderr, "invalid 32-bit value: %s\n", argv[3]);
            return 2;
        }
        HANDLE process = OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, FALSE, process_id);
        if (process == NULL) {
            fprintf(stderr, "cannot open target process\n");
            return 1;
        }
        uintptr_t address = 0x00400000;
        uintptr_t limit = 0x04000000;
        unsigned matches = 0;
        while (address < limit) {
            MEMORY_BASIC_INFORMATION region;
            SIZE_T queried = VirtualQueryEx(
                process,
                (void *)address,
                &region,
                sizeof(region)
            );
            if (queried != sizeof(region) || region.RegionSize == 0) {
                break;
            }
            uintptr_t next = (uintptr_t)region.BaseAddress + region.RegionSize;
            BOOL readable =
                region.State == MEM_COMMIT &&
                (region.Protect & (PAGE_GUARD | PAGE_NOACCESS)) == 0;
            if (readable && region.RegionSize <= SIZE_MAX) {
                unsigned char *bytes = malloc(region.RegionSize);
                SIZE_T bytes_read = 0;
                BOOL ok = bytes != NULL && ReadProcessMemory(
                    process,
                    region.BaseAddress,
                    bytes,
                    region.RegionSize,
                    &bytes_read
                );
                if (ok) {
                    for (SIZE_T offset = 0; offset + sizeof(uint32_t) <= bytes_read; offset++) {
                        uint32_t candidate = 0;
                        memcpy(&candidate, bytes + offset, sizeof(candidate));
                        if (candidate == (uint32_t)value) {
                            printf(
                                "0x%08lx\n",
                                (unsigned long)((uintptr_t)region.BaseAddress + offset)
                            );
                            matches++;
                        }
                    }
                }
                free(bytes);
            }
            if (next <= address) {
                break;
            }
            address = next;
        }
        CloseHandle(process);
        if (matches == 0) {
            fprintf(stderr, "32-bit value not found\n");
            return 1;
        }
        return 0;
    }
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
    if (slot_oracle || slot_oracle_expanded) {
        if (argc != 4) {
            fprintf(stderr, "%s requires one comma-separated value list\n", argv[2]);
            return 2;
        }
        const unsigned value_count = slot_oracle_expanded ? 6 : 4;
        unsigned long values[6] = {0};
        char *cursor = argv[3];
        for (unsigned index = 0; index < value_count; index++) {
            char *end = NULL;
            values[index] = strtoul(cursor, &end, 0);
            if (
                end == cursor ||
                values[index] > 0xff ||
                (index + 1 < value_count && *end != ',') ||
                (index + 1 == value_count && *end != '\0')
            ) {
                fprintf(stderr, "invalid slot-oracle values\n");
                return 2;
            }
            cursor = end + (index + 1 < value_count ? 1 : 0);
        }
        if (
            values[3] > 1 ||
            (slot_oracle_expanded && (values[4] > 1 || values[5] > 1))
        ) {
            fprintf(stderr, "slot-oracle flags must be zero or one\n");
            return 2;
        }
        HANDLE process = OpenProcess(
            PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION |
                PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE,
            FALSE,
            process_id
        );
        if (process == NULL) {
            fprintf(stderr, "cannot open target process for slot oracle\n");
            return 1;
        }
        const uintptr_t input_addresses[3] = {0x00857ba8, 0x009203bc, 0x00816658};
        BOOL ok = TRUE;
        for (unsigned index = 0; index < 3; index++) {
            uint32_t value = (uint32_t)values[index];
            SIZE_T written = 0;
            ok = ok && WriteProcessMemory(
                process,
                (void *)input_addresses[index],
                &value,
                sizeof(value),
                &written
            ) && written == sizeof(value);
        }
        const uintptr_t override_addresses[4] = {
            0x005f1aca, 0x0060adaf, 0x00600902, 0x00609703
        };
        const unsigned char override_values[4] = {
            slot_oracle_expanded && values[4] != 0 ? 4 : 0,
            slot_oracle_expanded && values[4] != 0 ? 4 : 0,
            slot_oracle_expanded && values[5] == 0 ? 4 : 0,
            slot_oracle_expanded && values[5] != 0 ? 4 : 0
        };
        for (unsigned index = 0; index < 4; index++) {
            SIZE_T written = 0;
            ok = ok && WriteProcessMemory(
                process,
                (void *)override_addresses[index],
                &override_values[index],
                sizeof(override_values[index]),
                &written
            ) && written == sizeof(override_values[index]);
        }
        unsigned char header_byte = 0;
        SIZE_T transferred = 0;
        ok = ok && ReadProcessMemory(
            process,
            (void *)0x0060b6ba,
            &header_byte,
            sizeof(header_byte),
            &transferred
        ) && transferred == sizeof(header_byte);
        header_byte = (unsigned char)((header_byte & 0x7f) | (values[3] != 0 ? 0x80 : 0));
        transferred = 0;
        ok = ok && WriteProcessMemory(
            process,
            (void *)0x0060b6ba,
            &header_byte,
            sizeof(header_byte),
            &transferred
        ) && transferred == sizeof(header_byte);
        if (!ok) {
            CloseHandle(process);
            fprintf(stderr, "cannot stage slot-oracle inputs\n");
            return 1;
        }
        HANDLE thread = CreateRemoteThread(
            process,
            NULL,
            0,
            (LPTHREAD_START_ROUTINE)(uintptr_t)0x004692b0,
            NULL,
            0,
            NULL
        );
        if (thread == NULL || WaitForSingleObject(thread, 5000) != WAIT_OBJECT_0) {
            if (thread != NULL) {
                CloseHandle(thread);
            }
            CloseHandle(process);
            fprintf(stderr, "slot-oracle dispatcher call failed\n");
            return 1;
        }
        CloseHandle(thread);
        const uintptr_t output_addresses[5] = {
            0x0061fc40, 0x008f3910, 0x0084a988, 0x00852a2c, 0x0091c558
        };
        for (unsigned output = 0; output < 5; output++) {
            unsigned char bytes[5] = {0};
            transferred = 0;
            ok = ReadProcessMemory(
                process,
                (void *)output_addresses[output],
                bytes,
                sizeof(bytes),
                &transferred
            ) && transferred == sizeof(bytes);
            if (!ok) {
                CloseHandle(process);
                fprintf(stderr, "cannot read slot-oracle outputs\n");
                return 1;
            }
            for (unsigned index = 0; index < sizeof(bytes); index++) {
                printf("%02x", bytes[index]);
            }
        }
        putchar('\n');
        CloseHandle(process);
        return 0;
    }
    if (command_at) {
        if (argc != 4) {
            fprintf(stderr, "command-at requires HWND_ADDRESS,COMMAND_ID\n");
            return 2;
        }
        char *separator = strchr(argv[3], ',');
        if (separator == NULL) {
            fprintf(stderr, "command-at requires HWND_ADDRESS,COMMAND_ID\n");
            return 2;
        }
        *separator = '\0';
        char *address_end = NULL;
        char *command_end = NULL;
        unsigned long address = strtoul(argv[3], &address_end, 0);
        unsigned long command = strtoul(separator + 1, &command_end, 0);
        if (
            *address_end != '\0' ||
            *command_end != '\0' ||
            address == 0 ||
            command > 0xffff
        ) {
            fprintf(stderr, "invalid command-at arguments\n");
            return 2;
        }
        HANDLE process = OpenProcess(PROCESS_VM_READ, FALSE, process_id);
        HWND target = NULL;
        SIZE_T bytes_read = 0;
        BOOL read_ok = process != NULL && ReadProcessMemory(
            process,
            (void *)(uintptr_t)address,
            &target,
            sizeof(target),
            &bytes_read
        );
        if (process != NULL) {
            CloseHandle(process);
        }
        if (!read_ok || bytes_read != sizeof(target) || !IsWindow(target)) {
            fprintf(stderr, "cannot resolve target window at 0x%08lx\n", address);
            return 1;
        }
        if (!PostMessage(target, WM_COMMAND, MAKEWPARAM(command, 0), 0)) {
            fprintf(stderr, "cannot post command 0x%04lx\n", command);
            return 1;
        }
        return 0;
    }
    if (post_command) {
        if (argc != 4) {
            fprintf(stderr, "post-command requires a command id\n");
            return 2;
        }
        char *end = NULL;
        unsigned long command = strtoul(argv[3], &end, 0);
        if (end == argv[3] || *end != '\0' || command > 0xffff) {
            fprintf(stderr, "invalid command id: %s\n", argv[3]);
            return 2;
        }
        if (!PostMessage(search.window, WM_COMMAND, MAKEWPARAM(command, 0), 0)) {
            fprintf(stderr, "cannot post command 0x%04lx\n", command);
            return 1;
        }
        return 0;
    }
    if (key) {
        if (argc != 4) {
            fprintf(stderr, "key requires a virtual-key code\n");
            return 2;
        }
        char *end = NULL;
        unsigned long virtual_key = strtoul(argv[3], &end, 0);
        if (end == argv[3] || *end != '\0' || virtual_key > 0xff) {
            fprintf(stderr, "invalid virtual-key code: %s\n", argv[3]);
            return 2;
        }
        if (
            !PostMessage(search.window, WM_KEYDOWN, virtual_key, 0) ||
            !PostMessage(search.window, WM_KEYUP, virtual_key, 0xc0000000)
        ) {
            fprintf(stderr, "cannot post virtual-key code 0x%02lx\n", virtual_key);
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
    if (children) {
        EnumChildWindows(search.window, list_child_window, 0);
        return 0;
    }
    if (menu) {
        HMENU root = GetMenu(search.window);
        if (root == NULL) {
            fprintf(stderr, "menu not found\n");
            return 1;
        }
        list_menu_items(root, 0);
        return 0;
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
        if (!PostMessage(control, BM_CLICK, 0, 0)) {
            fprintf(stderr, "cannot click dialog control: 0x%04lx\n", control_id);
            return 1;
        }
        return 0;
    }
    if (set_text) {
        if (argc != 4) {
            fprintf(stderr, "set-text requires CONTROL_ID,TEXT\n");
            return 2;
        }
        char *separator = strchr(argv[3], ',');
        if (separator == NULL) {
            fprintf(stderr, "set-text requires CONTROL_ID,TEXT\n");
            return 2;
        }
        *separator = '\0';
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
        struct child_control_search edit_search = {
            .control_id = (int)control_id,
            .window_class = "Edit",
            .window = NULL
        };
        EnumChildWindows(search.window, find_child_control, (LPARAM)&edit_search);
        if (edit_search.window != NULL) {
            control = edit_search.window;
        }
        PostMessage(control, EM_SETSEL, 0, -1);
        for (const unsigned char *character = (unsigned char *)(separator + 1);
             *character != '\0';
             character++) {
            PostMessage(control, WM_CHAR, *character, 0);
        }
        return 0;
    }
    if (select_combo) {
        if (argc != 4) {
            fprintf(stderr, "select requires CONTROL_ID,INDEX\n");
            return 2;
        }
        char *separator = strchr(argv[3], ',');
        if (separator == NULL) {
            fprintf(stderr, "select requires CONTROL_ID,INDEX\n");
            return 2;
        }
        *separator = '\0';
        char *control_end = NULL;
        char *index_end = NULL;
        unsigned long control_id = strtoul(argv[3], &control_end, 0);
        unsigned long index = strtoul(separator + 1, &index_end, 0);
        if (
            control_end == argv[3] ||
            *control_end != '\0' ||
            index_end == separator + 1 ||
            *index_end != '\0' ||
            control_id > 0xffff ||
            index > INT_MAX
        ) {
            fprintf(stderr, "invalid combo selection: %s,%s\n", argv[3], separator + 1);
            return 2;
        }
        HWND control = GetDlgItem(search.window, (int)control_id);
        if (control == NULL) {
            fprintf(stderr, "combo control not found: 0x%04lx\n", control_id);
            return 1;
        }
        LRESULT selected = SendMessage(control, CB_SETCURSEL, index, 0);
        if (selected == CB_ERR) {
            fprintf(stderr, "combo selection rejected: 0x%04lx,%lu\n", control_id, index);
            return 1;
        }
        SendMessage(
            search.window,
            WM_COMMAND,
            MAKEWPARAM((WORD)control_id, CBN_SELCHANGE),
            (LPARAM)control
        );
        return 0;
    }
    if (clipboard_bmp || clipboard_bmp_paste) {
        if (argc != 4) {
            fprintf(stderr, "%s requires a Windows BMP path\n", argv[2]);
            return 2;
        }
        FILE *input = fopen(argv[3], "rb");
        if (input == NULL) {
            fprintf(stderr, "cannot open BMP: %s\n", argv[3]);
            return 1;
        }
        BITMAPFILEHEADER header;
        if (
            fread(&header, sizeof(header), 1, input) != 1 ||
            header.bfType != 0x4d42 ||
            header.bfSize < sizeof(header) ||
            header.bfOffBits < sizeof(header) ||
            header.bfOffBits > header.bfSize
        ) {
            fclose(input);
            fprintf(stderr, "invalid BMP file header\n");
            return 1;
        }
        SIZE_T dib_size = (SIZE_T)header.bfSize - sizeof(header);
        HGLOBAL memory = GlobalAlloc(GMEM_MOVEABLE, dib_size);
        void *dib = memory == NULL ? NULL : GlobalLock(memory);
        BOOL loaded = dib != NULL && fread(dib, 1, dib_size, input) == dib_size;
        if (loaded) {
            DWORD info_size = 0;
            if (dib_size < sizeof(info_size)) {
                loaded = FALSE;
            } else {
                memcpy(&info_size, dib, sizeof(info_size));
                loaded = info_size >= sizeof(info_size) && info_size <= dib_size;
            }
        }
        if (loaded && dib_size >= sizeof(BITMAPINFOHEADER)) {
            BITMAPINFOHEADER *info = (BITMAPINFOHEADER *)dib;
            SIZE_T pixel_offset = (SIZE_T)header.bfOffBits - sizeof(header);
            if (
                info->biSize >= sizeof(*info) &&
                info->biSize <= dib_size &&
                info->biHeight != LONG_MIN &&
                info->biHeight < 0 &&
                info->biWidth > 0 &&
                info->biPlanes == 1 &&
                info->biCompression == BI_RGB &&
                info->biBitCount > 0 &&
                pixel_offset >= info->biSize &&
                pixel_offset <= dib_size
            ) {
                SIZE_T rows = (SIZE_T)(-info->biHeight);
                SIZE_T width = (SIZE_T)info->biWidth;
                SIZE_T bits_per_pixel = (SIZE_T)info->biBitCount;
                SIZE_T row_size = 0;
                if (
                    width <= (SIZE_MAX - 31) / bits_per_pixel &&
                    width * bits_per_pixel + 31 <= SIZE_MAX
                ) {
                    row_size = ((width * bits_per_pixel + 31) / 32) * 4;
                }
                if (row_size != 0 && rows <= (dib_size - pixel_offset) / row_size) {
                    unsigned char *pixels = (unsigned char *)dib + pixel_offset;
                    unsigned char *row = malloc(row_size);
                    if (row == NULL) {
                        loaded = FALSE;
                    } else {
                        for (SIZE_T top = 0; top < rows / 2; top++) {
                            unsigned char *upper = pixels + top * row_size;
                            unsigned char *lower = pixels + (rows - 1 - top) * row_size;
                            memcpy(row, upper, row_size);
                            memcpy(upper, lower, row_size);
                            memcpy(lower, row, row_size);
                        }
                        free(row);
                        info->biHeight = -info->biHeight;
                    }
                }
            }
        }
        if (dib != NULL) {
            GlobalUnlock(memory);
        }
        fclose(input);
        if (!loaded) {
            if (memory != NULL) {
                GlobalFree(memory);
            }
            fprintf(stderr, "cannot load BMP DIB payload\n");
            return 1;
        }
        HBITMAP bitmap = (HBITMAP)LoadImageA(
            NULL,
            argv[3],
            IMAGE_BITMAP,
            0,
            0,
            LR_LOADFROMFILE | LR_CREATEDIBSECTION
        );
        if (bitmap == NULL) {
            GlobalFree(memory);
            fprintf(stderr, "cannot create Windows bitmap from BMP\n");
            return 1;
        }
        if (!OpenClipboard(search.window)) {
            GlobalFree(memory);
            DeleteObject(bitmap);
            fprintf(stderr, "cannot open Windows clipboard\n");
            return 1;
        }
        BOOL emptied = EmptyClipboard();
        BOOL dib_published = emptied && SetClipboardData(CF_DIB, memory) != NULL;
        BOOL bitmap_published = emptied && SetClipboardData(CF_BITMAP, bitmap) != NULL;
        CloseClipboard();
        if (!dib_published) {
            GlobalFree(memory);
        }
        if (!bitmap_published) {
            DeleteObject(bitmap);
        }
        if (!dib_published || !bitmap_published) {
            fprintf(stderr, "cannot publish BMP to Windows clipboard\n");
            return 1;
        }
        if (clipboard_bmp_paste) {
            /*
             * Lunar Magic 3.63 has both a legacy Window16x16 window and the
             * modeless Map16 editor dialog. Bitmap paste belongs to the
             * latter. DAT_00a09270 is the modeless dialog HWND; targeting the
             * legacy window silently routes the command through the wrong
             * dispatcher.
             */
            HWND paste_target = read_process_window_handle(process_id, 0x00a09270);
            if (paste_target == NULL) {
                fprintf(stderr, "modeless 16x16 Tile Map Editor dialog not found\n");
                return 1;
            }
            if (
                !IsClipboardFormatAvailable(CF_DIB) ||
                !IsClipboardFormatAvailable(CF_BITMAP)
            ) {
                fprintf(stderr, "published bitmap clipboard formats are unavailable\n");
                return 1;
            }
            if (!PostMessage(paste_target, WM_COMMAND, MAKEWPARAM(0x2276, 0), 0)) {
                fprintf(stderr, "cannot post bitmap paste command\n");
                return 1;
            }
            HWND import_dialog = NULL;
            for (unsigned int attempt = 0; attempt < 200 && import_dialog == NULL; attempt++) {
                Sleep(25);
                import_dialog = find_process_window_by_title(
                    process_id,
                    "Convert and Paste Bitmap (in hex)"
                );
            }
            if (import_dialog == NULL) {
                fprintf(stderr, "bitmap conversion dialog was not ready within 5 seconds\n");
                return 1;
            }
            while (IsWindow(import_dialog)) {
                Sleep(25);
            }
        }
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
