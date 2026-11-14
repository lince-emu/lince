/* SPDX-FileCopyrightText: 2026 Lince contributors */
/* SPDX-License-Identifier: GPL-3.0-or-later */

/* Lays out a B1 test program as an NRO image (docs/research/nro-and-b1-svcs.md N1-N3):
 * text at 0 starting with the start block and header space, then page-aligned ro and data.
 * tools/elf2nro turns the three load segments into the NRO file. */

ENTRY(nro_start)

PHDRS
{
    text PT_LOAD FLAGS(5);
    ro PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}

SECTIONS
{
    . = 0;
    .text : { KEEP(*(.text.nro_start)) *(.text .text.*) } :text
    . = ALIGN(0x1000);
    .rodata : { *(.rodata .rodata.*) } :ro
    . = ALIGN(0x1000);
    .data : { *(.data .data.*) } :data
    .bss : { *(.bss .bss.*) *(COMMON) } :data
    /DISCARD/ : { *(.comment) *(.note .note.*) *(.eh_frame*) }
}
