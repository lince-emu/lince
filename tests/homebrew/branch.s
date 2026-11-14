// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Instruction group: branches (b, bl/ret, cbz/cbnz, tbz/tbnz, conditional branches) driving a
// loop that sums 1..10. Prints "ok" when the sum is 55, else "fail".

    .text
    .global main
main:
    mov     x19, #0                  // sum
    mov     x20, #10                 // counter
loop:
    mov     x0, x20
    bl      add_to_sum
    sub     x20, x20, #1
    cbnz    x20, loop

    cbz     x19, fail
    tbnz    x19, #3, fail            // 55 = 0b110111: bit 3 clear
    tbz     x19, #0, fail            // bit 0 set
    cmp     x19, #55
    b.eq    pass
    b       fail

add_to_sum:
    add     x19, x19, x0
    ret

    .include "report.inc"
