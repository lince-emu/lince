// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Shared tail of the self-checking programs: each branches to `pass` or `fail`, which print
// "ok" or "fail" with OutputDebugString (SVC 0x27) and exit (SVC 0x07).

pass:
    adrp    x0, ok_text
    add     x0, x0, :lo12:ok_text
    mov     x1, #2
    b       report
fail:
    adrp    x0, fail_text
    add     x0, x0, :lo12:fail_text
    mov     x1, #4
report:
    svc     #0x27
    svc     #0x07

    .section .rodata
ok_text:
    .ascii  "ok"
fail_text:
    .ascii  "fail"
    .text
