// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Instruction group: integer data processing (moves, add/sub, logical, shifts, compare).
// Prints "ok" when every result matches the value worked out by hand, else "fail".

    .text
    .global main
main:
    movz    x2, #0x1234, lsl #16
    movk    x2, #0x5678
    ldr     x3, =0x12345678
    cmp     x2, x3
    b.ne    fail

    mov     x4, #100
    add     x5, x4, #23              // 123
    sub     x6, x5, x4               // 23
    add     x7, x6, x6, lsl #2       // 115
    subs    x8, x7, #115             // 0, sets Z
    b.ne    fail

    mov     x9, #0xF0
    and     x10, x9, #0x3C           // 0x30
    orr     x11, x10, #0x0F          // 0x3F
    eor     x12, x11, x9             // 0xCF
    cmp     x12, #0xCF
    b.ne    fail

    lsl     x13, x12, #4             // 0xCF0
    lsr     x14, x13, #8             // 0xC
    mvn     w15, wzr                 // 0xFFFFFFFF, upper half cleared
    adds    w16, w15, #1             // 0, carry out
    b.cc    fail
    cmp     x14, #0xC
    b.ne    fail
    lsr     x17, x15, #32            // 0 if the w write cleared the top
    cbnz    x17, fail
    b       pass

    .include "report.inc"
    .ltorg
