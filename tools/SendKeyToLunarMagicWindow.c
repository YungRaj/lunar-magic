#include <stdio.h>
#include <stdlib.h>
#include <windows.h>

int main(int argc, char **argv) {
    if (argc != 3) {
        fputs("usage: SendKeyToLunarMagicWindow <window-title> <virtual-key-decimal>\n", stderr);
        return 2;
    }
    HWND window = FindWindowA(NULL, argv[1]);
    if (window == NULL) {
        fputs("target window not found\n", stderr);
        return 1;
    }
    WPARAM key = (WPARAM)atoi(argv[2]);
    PostMessageA(window, WM_KEYDOWN, key, 0);
    PostMessageA(window, WM_KEYUP, key, 0xc0000000);
    return 0;
}
