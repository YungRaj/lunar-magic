#include <stdio.h>
#include <stdlib.h>
#include <windows.h>

int main(int argc, char **argv) {
    if (argc != 2) {
        fputs("usage: SendLunarMagicCommand <decimal-command-id>\n", stderr);
        return 2;
    }
    HWND frame = FindWindowA("LMFrame", NULL);
    if (frame == NULL) {
        fputs("Lunar Magic frame not found\n", stderr);
        return 1;
    }
    PostMessageA(frame, WM_COMMAND, MAKEWPARAM(atoi(argv[1]), 0), 0);
    return 0;
}
