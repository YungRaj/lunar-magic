#include <windows.h>

#include <stdio.h>

/* A deterministic external-editor stand-in for Lunar Magic/Wine launch oracles.
 * argv[1] is a create-new log path and argv[2] is the graphics file supplied by `%1`.
 * The helper records every direct argument, flips the first graphics byte, flushes it, and exits.
 */
int main(int argc, char **argv) {
    FILE *log;
    HANDLE graphics;
    DWORD size;
    DWORD transferred;
    BYTE first;
    int index;

    if (argc < 3) {
        return 2;
    }
    log = fopen(argv[1], "wbx");
    if (log == NULL) {
        return 3;
    }
    for (index = 0; index < argc; index++) {
        if (fprintf(log, "argv[%d]=%s\r\n", index, argv[index]) < 0) {
            fclose(log);
            return 4;
        }
    }
    if (fclose(log) != 0) {
        return 5;
    }

    graphics = CreateFileA(argv[2], GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ, NULL,
                           OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    if (graphics == INVALID_HANDLE_VALUE) {
        return 6;
    }
    size = GetFileSize(graphics, NULL);
    if (size == INVALID_FILE_SIZE || size == 0) {
        CloseHandle(graphics);
        return 7;
    }
    if (!ReadFile(graphics, &first, 1, &transferred, NULL) || transferred != 1) {
        CloseHandle(graphics);
        return 8;
    }
    first ^= 0x0f;
    if (SetFilePointer(graphics, 0, NULL, FILE_BEGIN) == INVALID_SET_FILE_POINTER ||
        !WriteFile(graphics, &first, 1, &transferred, NULL) || transferred != 1 ||
        !FlushFileBuffers(graphics)) {
        CloseHandle(graphics);
        return 9;
    }
    CloseHandle(graphics);
    return 0;
}
