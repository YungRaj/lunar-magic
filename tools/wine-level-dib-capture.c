#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <tlhelp32.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/*
 * Lunar Magic 3.63 primary level-editor DIB globals, authenticated in Ghidra:
 *   CreatePrimaryEditorRenderSurface @ 0044DA20
 *   width                         @ 005E772C
 *   height                        @ 005E7730
 *   top-down BGRA pixel pointer   @ 00E27840
 */
enum {
    LEVEL_DIB_WIDTH_ADDRESS = 0x005e772c,
    LEVEL_DIB_HEIGHT_ADDRESS = 0x005e7730,
    LEVEL_DIB_POINTER_ADDRESS = 0x00e27840,
    MAX_LEVEL_DIB_DIMENSION = 16384
};

#pragma pack(push, 1)
struct bitmap_file_header {
    uint16_t type;
    uint32_t size;
    uint16_t reserved_1;
    uint16_t reserved_2;
    uint32_t pixel_offset;
};
#pragma pack(pop)

static DWORD find_process(const char *name) {
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    PROCESSENTRY32 entry = {.dwSize = sizeof(entry)};
    DWORD found = 0;
    if (snapshot != INVALID_HANDLE_VALUE && Process32First(snapshot, &entry)) {
        do {
            if (_stricmp(entry.szExeFile, name) == 0) {
                found = entry.th32ProcessID;
                break;
            }
        } while (Process32Next(snapshot, &entry));
    }
    if (snapshot != INVALID_HANDLE_VALUE) CloseHandle(snapshot);
    return found;
}

static BOOL read_exact(HANDLE process, uintptr_t address, void *output, SIZE_T length) {
    SIZE_T bytes_read = 0;
    return ReadProcessMemory(process, (void *)address, output, length, &bytes_read) &&
           bytes_read == length;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: wine-level-dib-capture.exe EXECUTABLE OUTPUT.bmp\n");
        return 2;
    }

    DWORD process_id = find_process(argv[1]);
    if (process_id == 0) {
        fprintf(stderr, "process not found: %s\n", argv[1]);
        return 1;
    }
    HANDLE process = OpenProcess(PROCESS_VM_READ, FALSE, process_id);
    if (process == NULL) {
        fprintf(stderr, "cannot open process %lu\n", (unsigned long)process_id);
        return 1;
    }

    uint32_t width = 0;
    uint32_t height = 0;
    uint32_t pixels_address = 0;
    BOOL metadata_ok =
        read_exact(process, LEVEL_DIB_WIDTH_ADDRESS, &width, sizeof(width)) &&
        read_exact(process, LEVEL_DIB_HEIGHT_ADDRESS, &height, sizeof(height)) &&
        read_exact(process, LEVEL_DIB_POINTER_ADDRESS, &pixels_address, sizeof(pixels_address));
    if (!metadata_ok || width == 0 || height == 0 || width > MAX_LEVEL_DIB_DIMENSION ||
        height > MAX_LEVEL_DIB_DIMENSION || pixels_address == 0) {
        CloseHandle(process);
        fprintf(
            stderr,
            "invalid live level DIB metadata: width=%lu height=%lu pixels=0x%08lx\n",
            (unsigned long)width,
            (unsigned long)height,
            (unsigned long)pixels_address
        );
        return 1;
    }

    uint64_t pixel_bytes_64 = (uint64_t)width * (uint64_t)height * 4;
    if (pixel_bytes_64 > SIZE_MAX || pixel_bytes_64 > UINT32_MAX) {
        CloseHandle(process);
        fprintf(stderr, "live level DIB is too large\n");
        return 1;
    }
    SIZE_T pixel_bytes = (SIZE_T)pixel_bytes_64;
    unsigned char *pixels = malloc(pixel_bytes);
    if (pixels == NULL ||
        !read_exact(process, (uintptr_t)pixels_address, pixels, pixel_bytes)) {
        free(pixels);
        CloseHandle(process);
        fprintf(stderr, "cannot read live level DIB pixels\n");
        return 1;
    }
    CloseHandle(process);

    struct bitmap_file_header file_header = {
        .type = 0x4d42,
        .size = (uint32_t)(sizeof(struct bitmap_file_header) + sizeof(BITMAPINFOHEADER) +
                           pixel_bytes),
        .reserved_1 = 0,
        .reserved_2 = 0,
        .pixel_offset = sizeof(struct bitmap_file_header) + sizeof(BITMAPINFOHEADER)
    };
    BITMAPINFOHEADER dib_header = {0};
    dib_header.biSize = sizeof(dib_header);
    dib_header.biWidth = (LONG)width;
    dib_header.biHeight = -(LONG)height;
    dib_header.biPlanes = 1;
    dib_header.biBitCount = 32;
    dib_header.biCompression = BI_RGB;
    dib_header.biSizeImage = (DWORD)pixel_bytes;

    FILE *output = fopen(argv[2], "wb");
    BOOL wrote = output != NULL &&
                 fwrite(&file_header, sizeof(file_header), 1, output) == 1 &&
                 fwrite(&dib_header, sizeof(dib_header), 1, output) == 1 &&
                 fwrite(pixels, 1, pixel_bytes, output) == pixel_bytes;
    if (output != NULL) fclose(output);
    free(pixels);
    if (!wrote) {
        fprintf(stderr, "cannot write capture: %s\n", argv[2]);
        return 1;
    }

    printf(
        "captured %lux%lu BGRA level DIB from 0x%08lx\n",
        (unsigned long)width,
        (unsigned long)height,
        (unsigned long)pixels_address
    );
    return 0;
}
