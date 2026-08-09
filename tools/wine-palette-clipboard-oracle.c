#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tlhelp32.h>

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
    if (snapshot != INVALID_HANDLE_VALUE) CloseHandle(snapshot);
    return process_id;
}

struct window_search { DWORD process_id; HWND palette; };

static BOOL CALLBACK find_palette(HWND window, LPARAM opaque) {
    struct window_search *search = (struct window_search *)opaque;
    DWORD process_id = 0;
    char class_name[64] = {0};
    char title[128] = {0};
    GetWindowThreadProcessId(window, &process_id);
    if (process_id != search->process_id) return TRUE;
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    if (_stricmp(class_name, "#32770") == 0 && strcmp(title, "Palette Editor") == 0) {
        search->palette = window;
        return FALSE;
    }
    return TRUE;
}

static int dump_lunar_magic_clipboard(void) {
    if (!OpenClipboard(NULL)) return 1;
    for (UINT format = EnumClipboardFormats(0); format != 0; format = EnumClipboardFormats(format)) {
        char name[128] = {0};
        GetClipboardFormatNameA(format, name, sizeof(name));
        if (strstr(name, "Lunar Magic Color") == NULL) continue;
        HANDLE memory = GetClipboardData(format);
        SIZE_T size = memory == NULL ? 0 : GlobalSize(memory);
        const uint8_t *bytes = memory == NULL ? NULL : GlobalLock(memory);
        printf("name=%s size=%lu bytes=", name, (unsigned long)size);
        if (bytes != NULL) {
            for (SIZE_T index = 0; index < size; index++) printf("%02X", bytes[index]);
            GlobalUnlock(memory);
        }
        putchar('\n');
    }
    CloseClipboard();
    return 0;
}

static int invoke_copy(DWORD process_id, HWND palette, BOOL row) {
    HANDLE process = OpenProcess(
        PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE |
            PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION,
        FALSE, process_id);
    if (process == NULL) return 1;
    uint8_t code[32] = {0};
    size_t length = 0;
#define EMIT(byte) code[length++] = (uint8_t)(byte)
#define EMIT_DWORD(value) do { uint32_t emitted = (uint32_t)(value); memcpy(code + length, &emitted, 4); length += 4; } while (0)
    void *row_data = NULL;
    if (row) {
        uint32_t colors[16];
        for (uint32_t index = 0; index < 16; index++) {
            uint32_t channel = index * 17;
            colors[index] = channel | (channel << 8) | (channel << 16);
        }
        row_data = VirtualAllocEx(process, NULL, sizeof(colors), MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        SIZE_T written = 0;
        if (row_data == NULL ||
            !WriteProcessMemory(process, row_data, colors, sizeof(colors), &written) ||
            written != sizeof(colors)) goto failure;
        EMIT(0x68); EMIT_DWORD(row_data);
        EMIT(0x68); EMIT_DWORD(palette);
        EMIT(0xb8); EMIT_DWORD(0x0056fd80);
        EMIT(0xff); EMIT(0xd0);
        EMIT(0x83); EMIT(0xc4); EMIT(0x08);
    } else {
        uint32_t snes = 0x7fdd;
        SIZE_T written = 0;
        if (!WriteProcessMemory(process, (void *)0x00e0251c, &snes, sizeof(snes), &written) ||
            written != sizeof(snes)) goto failure;
        EMIT(0x68); EMIT_DWORD(0);
        EMIT(0x68); EMIT_DWORD(0x00eff7ff);
        EMIT(0x68); EMIT_DWORD(palette);
        EMIT(0xb8); EMIT_DWORD(0x0056fa10);
        EMIT(0xff); EMIT(0xd0);
        EMIT(0x83); EMIT(0xc4); EMIT(0x0c);
    }
    EMIT(0xc3);
#undef EMIT_DWORD
#undef EMIT
    void *remote_code = VirtualAllocEx(
        process, NULL, length, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
    SIZE_T written = 0;
    if (remote_code == NULL ||
        !WriteProcessMemory(process, remote_code, code, length, &written) || written != length)
        goto failure;
    HANDLE thread = CreateRemoteThread(
        process, NULL, 0, (LPTHREAD_START_ROUTINE)remote_code, NULL, 0, NULL);
    int ok = thread != NULL && WaitForSingleObject(thread, 5000) == WAIT_OBJECT_0;
    if (thread != NULL) CloseHandle(thread);
    VirtualFreeEx(process, remote_code, 0, MEM_RELEASE);
    if (row_data != NULL) VirtualFreeEx(process, row_data, 0, MEM_RELEASE);
    CloseHandle(process);
    return ok ? dump_lunar_magic_clipboard() : 1;
failure:
    if (row_data != NULL) VirtualFreeEx(process, row_data, 0, MEM_RELEASE);
    CloseHandle(process);
    return 1;
}

static int publish_one(const char *name, const uint8_t *bytes, size_t size) {
    UINT format = RegisterClipboardFormatA(name);
    HGLOBAL memory = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size);
    void *destination = memory == NULL ? NULL : GlobalLock(memory);
    if (format == 0 || destination == NULL) {
        if (memory != NULL) GlobalFree(memory);
        return 1;
    }
    memcpy(destination, bytes, size);
    GlobalUnlock(memory);
    if (!OpenClipboard(NULL)) { GlobalFree(memory); return 1; }
    EmptyClipboard();
    if (SetClipboardData(format, memory) == NULL) {
        CloseClipboard();
        GlobalFree(memory);
        return 1;
    }
    CloseClipboard();
    return 0;
}

static int invoke_decoder(DWORD process_id, HWND palette, BOOL row) {
    HANDLE process = OpenProcess(
        PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE |
            PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION,
        FALSE, process_id);
    if (process == NULL) return 1;
    void *output = NULL;
    uint8_t code[24] = {0};
    size_t length = 0;
#define EMIT(byte) code[length++] = (uint8_t)(byte)
#define EMIT_DWORD(value) do { uint32_t emitted = (uint32_t)(value); memcpy(code + length, &emitted, 4); length += 4; } while (0)
    if (row) {
        output = VirtualAllocEx(process, NULL, 64, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        if (output == NULL) goto failure;
        EMIT(0x68); EMIT_DWORD(output);
        EMIT(0x68); EMIT_DWORD(palette);
        EMIT(0xb8); EMIT_DWORD(0x0056fff0);
        EMIT(0xff); EMIT(0xd0);
        EMIT(0x83); EMIT(0xc4); EMIT(0x08);
    } else {
        EMIT(0x68); EMIT_DWORD(palette);
        EMIT(0xb8); EMIT_DWORD(0x0056fc10);
        EMIT(0xff); EMIT(0xd0);
        EMIT(0x83); EMIT(0xc4); EMIT(0x04);
    }
    EMIT(0xc3);
#undef EMIT_DWORD
#undef EMIT
    void *remote_code = VirtualAllocEx(
        process, NULL, length, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
    SIZE_T written = 0;
    if (remote_code == NULL ||
        !WriteProcessMemory(process, remote_code, code, length, &written) || written != length)
        goto failure;
    HANDLE thread = CreateRemoteThread(
        process, NULL, 0, (LPTHREAD_START_ROUTINE)remote_code, NULL, 0, NULL);
    DWORD result = 0xffffffff;
    int ok = thread != NULL && WaitForSingleObject(thread, 5000) == WAIT_OBJECT_0 &&
        GetExitCodeThread(thread, &result);
    if (thread != NULL) CloseHandle(thread);
    VirtualFreeEx(process, remote_code, 0, MEM_RELEASE);
    printf("result=%08lX", (unsigned long)result);
    if (ok && row && result == 1) {
        uint32_t colors[16] = {0};
        SIZE_T loaded = 0;
        ok = ReadProcessMemory(process, output, colors, sizeof(colors), &loaded) &&
            loaded == sizeof(colors);
        printf(" colors=");
        for (size_t index = 0; index < 16; index++) printf("%04lX", (unsigned long)colors[index]);
    }
    putchar('\n');
    if (output != NULL) VirtualFreeEx(process, output, 0, MEM_RELEASE);
    CloseHandle(process);
    return ok ? 0 : 1;
failure:
    if (output != NULL) VirtualFreeEx(process, output, 0, MEM_RELEASE);
    CloseHandle(process);
    return 1;
}

int main(int argc, char **argv) {
    BOOL invoke = argc == 3 &&
        (strcmp(argv[2], "invoke-color") == 0 || strcmp(argv[2], "invoke-row") == 0);
    BOOL decode = argc == 3 &&
        (strcmp(argv[2], "decode-color") == 0 || strcmp(argv[2], "reject-color") == 0 ||
         strcmp(argv[2], "decode-row") == 0 || strcmp(argv[2], "reject-row") == 0);
    if (!invoke && !decode) {
        fprintf(stderr, "usage: wine-palette-clipboard-oracle.exe PROCESS invoke-color|invoke-row|decode-color|reject-color|decode-row|reject-row\n");
        return 2;
    }
    struct window_search search = {.process_id = find_process(argv[1]), .palette = NULL};
    EnumWindows(find_palette, (LPARAM)&search);
    if (search.palette == NULL) {
        fprintf(stderr, "Palette Editor not found\n");
        return 1;
    }
    if (invoke) return invoke_copy(search.process_id, search.palette, strcmp(argv[2], "invoke-row") == 0);
    if (decode) {
        BOOL row = strstr(argv[2], "row") != NULL;
        BOOL reject = strstr(argv[2], "reject") != NULL;
        uint8_t payload[132] = {0};
        size_t size = row ? sizeof(payload) : 12;
        if (row) {
            for (uint32_t index = 0; index < 16; index++) {
                uint32_t rgb = index * 17;
                rgb |= rgb << 8 | rgb << 16;
                memcpy(payload + 4 + index * 4, &rgb, 4);
                uint32_t snes = index * 0x0842;
                memcpy(payload + 68 + index * 4, &snes, 4);
            }
        } else {
            uint32_t rgb = 0x00eff7ff, snes = 0x7fdd;
            memcpy(payload + 4, &rgb, 4);
            memcpy(payload + 8, &snes, 4);
        }
        if (reject) size--;
        if (publish_one(row ? "Lunar Magic Color Row V2" : "Lunar Magic Color V2", payload, size))
            return 1;
        return invoke_decoder(search.process_id, search.palette, row);
    }
    return 2;
}
