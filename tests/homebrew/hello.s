// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// B1 exit program: OutputDebugString("hello") (SVC 0x27, research S9), then ExitProcess
// (SVC 0x07, research S8).

    .text
    .global main
main:
    adrp    x0, message
    add     x0, x0, :lo12:message
    mov     x1, #5
    svc     #0x27
    svc     #0x07

    .section .rodata
message:
    .ascii  "hello"
