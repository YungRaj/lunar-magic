#include <stdio.h>
#include <windows.h>
#include <commctrl.h>

static BOOL CALLBACK dump_toolbar(HWND window, LPARAM unused) {
    char class_name[64] = {0};
    GetClassNameA(window, class_name, sizeof(class_name));
    if (lstrcmpA(class_name, "ToolbarWindow32") != 0) return TRUE;
    printf("toolbar hwnd=%p visible=%d count=%ld\n", (void *)window, IsWindowVisible(window),
           (long)SendMessageA(window, TB_BUTTONCOUNT, 0, 0));
    return TRUE;
}

int main(void) {
    HWND frame = FindWindowA("LMFrame", NULL);
    if (frame == NULL) {
        fputs("Lunar Magic frame not found\n", stderr);
        return 1;
    }
    EnumChildWindows(frame, dump_toolbar, 0);
    return 0;
}
