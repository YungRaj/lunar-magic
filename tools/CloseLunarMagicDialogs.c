#include <string.h>
#include <windows.h>

static BOOL CALLBACK close_dialog(HWND window, LPARAM unused) {
    char class_name[64] = {0};
    (void)unused;
    GetClassNameA(window, class_name, sizeof(class_name));
    if (IsWindowVisible(window) && strcmp(class_name, "#32770") == 0) {
        PostMessageA(window, WM_COMMAND, MAKEWPARAM(IDCANCEL, 0), 0);
    }
    return TRUE;
}

int main(void) {
    EnumWindows(close_dialog, 0);
    return 0;
}
