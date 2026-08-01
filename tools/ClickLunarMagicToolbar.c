#include <stdio.h>
#include <stdlib.h>
#include <windows.h>

int main(int argc, char **argv) {
    if (argc != 2) {
        fputs("usage: ClickLunarMagicToolbar <x>\n", stderr);
        return 2;
    }
    HWND frame = FindWindowA("LMFrame", NULL);
    HWND toolbar = FindWindowExA(frame, NULL, "ToolbarWindow32", NULL);
    if (frame == NULL || toolbar == NULL) {
        fputs("Lunar Magic toolbar not found\n", stderr);
        return 1;
    }
    int x = atoi(argv[1]);
    LPARAM point = MAKELPARAM(x, 14);
    SendMessageA(toolbar, WM_LBUTTONDOWN, MK_LBUTTON, point);
    SendMessageA(toolbar, WM_LBUTTONUP, 0, point);
    return 0;
}
