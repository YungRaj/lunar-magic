#include <stdio.h>
#include <windows.h>
#include <commctrl.h>

int main(void) {
    HWND frame = FindWindowA("LMFrame", NULL);
    HWND toolbar = FindWindowExA(frame, NULL, "ToolbarWindow32", NULL);
    DWORD process_id = 0;
    GetWindowThreadProcessId(toolbar, &process_id);
    HANDLE process = OpenProcess(PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE, FALSE,
                                 process_id);
    if (toolbar == NULL || process == NULL) {
        fputs("Lunar Magic toolbar process not found\n", stderr);
        return 1;
    }
    int count = (int)SendMessageA(toolbar, TB_BUTTONCOUNT, 0, 0);
    void *remote = VirtualAllocEx(process, NULL, 512, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    printf("count=%d\n", count);
    for (int index = 0; index < count; index++) {
        TBBUTTON button = {0};
        if (SendMessageA(toolbar, TB_GETBUTTON, index, (LPARAM)remote) &&
            ReadProcessMemory(process, remote, &button, sizeof(button), NULL)) {
            char text[512] = {0};
            LRESULT text_length =
                SendMessageA(toolbar, TB_GETBUTTONTEXTA, button.idCommand, (LPARAM)remote);
            if (text_length >= 0) {
                ReadProcessMemory(process, remote, text, sizeof(text) - 1, NULL);
            }
            printf("index=%d command=%d bitmap=%d state=%u style=%u text=%s\n", index,
                   button.idCommand, button.iBitmap, button.fsState, button.fsStyle, text);
        }
    }
    VirtualFreeEx(process, remote, 0, MEM_RELEASE);
    CloseHandle(process);
    return 0;
}
