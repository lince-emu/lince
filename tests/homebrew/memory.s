// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Instruction group: loads and stores (byte to doubleword, pairs, pre/post-index) on the data
// and bss segments. Prints "ok" when every value reads back, else "fail".

    .text
    .global main
main:
    adrp    x1, table
    add     x1, x1, :lo12:table
    ldr     x2, [x1]                 // 0x1122334455667788
    ldr     w3, [x1, #8]             // 0x99AABBCC
    ldrb    w4, [x1, #1]             // 0x77
    ldrh    w5, [x1, #2]             // 0x5566
    ldr     x6, =0x1122334455667788
    cmp     x2, x6
    b.ne    fail
    cmp     w4, #0x77
    b.ne    fail
    mov     w7, #0x5566
    cmp     w5, w7
    b.ne    fail

    adrp    x8, scratch
    add     x8, x8, :lo12:scratch
    ldr     x9, [x8]                 // bss starts zeroed
    cbnz    x9, fail
    stp     x2, x3, [x8]
    ldp     x10, x11, [x8]
    cmp     x10, x2
    b.ne    fail
    cmp     x11, x3
    b.ne    fail
    strb    w4, [x8, #16]!           // pre-index: x8 += 16
    ldrb    w12, [x8], #-16          // post-index: x8 -= 16
    cmp     w12, w4
    b.ne    fail
    str     w5, [x8, #4]
    ldr     x13, [x8]                // low word x3, high word 0x5566
    ldr     x14, =0x00005566
    cmp     x14, x13, lsr #32
    b.ne    fail
    b       pass

    .include "report.inc"
    .ltorg

    .data
    .balign 8
table:
    .quad   0x1122334455667788
    .word   0x99AABBCC

    .bss
    .balign 8
scratch:
    .space  32
