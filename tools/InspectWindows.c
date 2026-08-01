#include <stdio.h>
#include <windows.h>

static BOOL CALLBACK print_child(HWND window, LPARAM unused) {
    char class_name[256] = {0};
    char title[512] = {0};
    RECT bounds = {0};
    (void)unused;
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    GetWindowRect(window, &bounds);
    printf("  child=%p id=%ld class=%s title=%s rect=%ld,%ld,%ld,%ld\n", window,
           (long)GetDlgCtrlID(window), class_name, title, (long)bounds.left, (long)bounds.top,
           (long)bounds.right, (long)bounds.bottom);
    return TRUE;
}

static BOOL CALLBACK print_window(HWND window, LPARAM unused) {
    char class_name[256] = {0};
    char title[512] = {0};
    RECT bounds = {0};
    (void)unused;
    if (!IsWindowVisible(window)) {
        return TRUE;
    }
    GetClassNameA(window, class_name, sizeof(class_name));
    GetWindowTextA(window, title, sizeof(title));
    GetWindowRect(window, &bounds);
    printf("window=%p class=%s title=%s rect=%ld,%ld,%ld,%ld\n", window, class_name, title,
           (long)bounds.left, (long)bounds.top, (long)bounds.right, (long)bounds.bottom);
    EnumChildWindows(window, print_child, 0);
    return TRUE;
}

int main(void) {
    EnumWindows(print_window, 0);
    return 0;
}
