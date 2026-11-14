// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Start block of every B1 test NRO (research N1-N2): a branch over the header, the unused
// module-header location, then 0x80 zero bytes that tools/elf2nro fills with the NRO0 header.

    .section .text.nro_start, "ax"
    .global nro_start
nro_start:
    b       main
    .word   0, 0, 0
    .space  0x80
