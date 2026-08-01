#include <stdio.h>
#include <stdlib.h>
#include <windows.h>

int main(int argc, char **argv) {
    if (argc != 3) {
        fputs("usage: SendCommandToWindow <window-title> <decimal-command-id>\n", stderr);
        return 2;
    }
    HWND window = FindWindowA(NULL, argv[1]);
    if (window == NULL) {
        fputs("target window not found\n", stderr);
        return 1;
    }
    PostMessageA(window, WM_COMMAND, MAKEWPARAM(atoi(argv[2]), 0), 0);
    return 0;
}
