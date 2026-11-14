// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// B2 program (T-049): threads on two cores synchronized with a mutex, a condition variable and
// an event, then IPC sessions to sm:, set, fsp-srv, time:u and hid. Each step prints one line
// with OutputDebugString: "<step> ok", or "<step> fail XXXXXXXX" with the first non-zero
// result (or a step-specific value). b2.expected lists the lines of a passing run.
//
// Steps, in output order (the code printed on failure in parentheses):
//   tls      TPIDRRO_EL0 is non-zero (1)
//   event    CreateEvent (result)
//   threads  CreateThread on cores 1 and 2, StartThread on both (first result)
//   wait     WaitSynchronization on the event the second worker to finish signals (result)
//   counter  both workers added 1000 under the mutex, worker B after A's condvar signal (the count)
//   cores    GetCurrentProcessorNumber was 1 in worker A and 2 in B (core A + 16 * core B - 0x21)
//   close    CloseHandle on both threads and both event handles (first result)
//   sm       ConnectToNamedPort "sm:", RegisterClient (first result)
//   set      GetServiceHandle "set", GetLanguageCode, CloseHandle (first result)
//   fsp-srv  GetServiceHandle, SetCurrentProcess, OpenSdCardFileSystem, CloseHandle x2 (first)
//   time     GetServiceHandle "time:u", GetStandardUserSystemClock, GetCurrentTime, CloseHandle x2
//   hid      GetServiceHandle, CreateAppletResource, CloseHandle x2 (first result)
//
// Sources: docs/research/b2-svcs-and-ipc.md (V*, C*, T*, S* rows). Values that note marks as
// unsettled are GUESS here, and the HLE (T-051 to T-054) implements the same reading:
// - mutex word: owner handle, bit 30 = waiters; the kernel returns from ArbitrateLock with the
//   lock handed to the caller (V12, GUESS);
// - TIPC: request type = 16 + command id; no padding before raw data; the reply's first raw
//   word is the result; PID and handles use the HIPC special header (T2, T3, GUESS);
// - CMIF requests use message type 4 with an SFCI version 0 header (C3, C6);
// - service names are the name's bytes in memory order, zero padded to 8 (S1, GUESS);
// - SVCs preserve every register they do not return in (GUESS); x9 is reloaded anyway.

    .macro addr reg, label
    adrp    \reg, \label
    add     \reg, \reg, :lo12:\label
    .endm

    // Keep the first non-zero code in w27.
    .macro keep
    cmp     w27, #0
    csel    w27, w0, w27, eq
    .endm

    // Print "<name> ok" or "<name> fail <w27>".
    .macro step name
    addr    x0, \name\()_s
    mov     x1, #(\name\()_e - \name\()_s)
    mov     w2, w27
    bl      report
    .endm

    .equ    SVC_CREATE_THREAD, 0x08
    .equ    SVC_START_THREAD, 0x09
    .equ    SVC_EXIT_THREAD, 0x0A
    .equ    SVC_SIGNAL_EVENT, 0x11
    .equ    SVC_CLOSE_HANDLE, 0x16
    .equ    SVC_WAIT_SYNCHRONIZATION, 0x18
    .equ    SVC_ARBITRATE_LOCK, 0x1A
    .equ    SVC_ARBITRATE_UNLOCK, 0x1B
    .equ    SVC_WAIT_KEY, 0x1C
    .equ    SVC_SIGNAL_KEY, 0x1D
    .equ    SVC_CONNECT_TO_NAMED_PORT, 0x1F
    .equ    SVC_SEND_SYNC_REQUEST, 0x21
    .equ    SVC_OUTPUT_DEBUG_STRING, 0x27
    .equ    SVC_EXIT_PROCESS, 0x07
    .equ    SVC_GET_CURRENT_PROCESSOR, 0x10
    .equ    SVC_CREATE_EVENT, 0x45
    .equ    WAITERS, 0x40000000
    .equ    PRIORITY, 0x2C
    .equ    ROUNDS, 1000
    .equ    STACK_SIZE, 0x4000
    .equ    WITH_PID, 1

// Main thread. x19 TLS (IPC message buffer), x20 sm: session, x21 service session, x23/x24
// event writable/readable, x25/x26 worker handles, x27 step code.
    .text
    .global main
main:
    mrs     x19, tpidrro_el0
    cmp     x19, #0
    cset    w27, eq
    step    tls

    svc     #SVC_CREATE_EVENT
    mov     w27, w0
    mov     w23, w1
    mov     w24, w2
    addr    x9, event_w
    str     w23, [x9]
    step    event

    mov     w27, #0
    addr    x1, worker
    addr    x2, block_a
    addr    x3, stack_a
    add     x3, x3, #STACK_SIZE
    mov     w4, #PRIORITY
    mov     w5, #1
    svc     #SVC_CREATE_THREAD
    keep
    mov     w25, w1
    addr    x9, block_a
    str     w25, [x9]
    addr    x1, worker
    addr    x2, block_b
    addr    x3, stack_b
    add     x3, x3, #STACK_SIZE
    mov     w4, #PRIORITY
    mov     w5, #2
    svc     #SVC_CREATE_THREAD
    keep
    mov     w26, w1
    addr    x9, block_b
    str     w26, [x9]
    mov     w0, w25
    svc     #SVC_START_THREAD
    keep
    mov     w0, w26
    svc     #SVC_START_THREAD
    keep
    step    threads

    addr    x1, wait_handles
    str     w24, [x1]
    mov     w2, #1
    mov     x3, #-1
    svc     #SVC_WAIT_SYNCHRONIZATION
    mov     w27, w0
    step    wait

    addr    x9, counter
    ldr     w10, [x9]
    mov     w11, #(2 * ROUNDS)
    cmp     w10, w11
    csel    w27, wzr, w10, eq
    step    counter

    addr    x9, block_a
    ldr     w10, [x9, #4]
    addr    x9, block_b
    ldr     w11, [x9, #4]
    orr     w10, w10, w11, lsl #4
    sub     w27, w10, #0x21          // worker A on core 1, B on core 2: 0x21
    step    cores

    mov     w27, #0
    mov     w0, w25
    svc     #SVC_CLOSE_HANDLE
    keep
    mov     w0, w26
    svc     #SVC_CLOSE_HANDLE
    keep
    mov     w0, w23
    svc     #SVC_CLOSE_HANDLE
    keep
    mov     w0, w24
    svc     #SVC_CLOSE_HANDLE
    keep
    step    close

    // sm: connect, then RegisterClient (TIPC 16 + 0) with a PID.
    addr    x1, sm_port
    svc     #SVC_CONNECT_TO_NAMED_PORT
    mov     w27, w0
    mov     w20, w1
    mov     w10, #16
    str     w10, [x19]
    mov     w10, #0x80000000         // special header, no raw data
    str     w10, [x19, #4]
    mov     w10, #1                  // send PID
    str     w10, [x19, #8]
    str     wzr, [x19, #0xC]
    str     wzr, [x19, #0x10]
    mov     w0, w20
    svc     #SVC_SEND_SYNC_REQUEST
    keep
    bl      tipc_reply
    keep
    step    sm

    // set: GetLanguageCode (CMIF 0).
    addr    x0, name_set
    ldr     x0, [x0]
    bl      get_service
    mov     w21, w1
    mov     w27, w0
    mov     w0, w21
    mov     w1, #0
    mov     w2, #0
    bl      cmif_call
    keep
    mov     w0, w21
    svc     #SVC_CLOSE_HANDLE
    keep
    step    set

    // fsp-srv: SetCurrentProcess (CMIF 1, PID + u64), OpenSdCardFileSystem (CMIF 18).
    addr    x0, name_fsp
    ldr     x0, [x0]
    bl      get_service
    mov     w21, w1
    mov     w27, w0
    mov     w0, w21
    mov     w1, #1
    mov     w2, #WITH_PID
    bl      cmif_call
    keep
    mov     w0, w21
    mov     w1, #18
    mov     w2, #0
    bl      cmif_call
    keep
    mov     w22, w1
    mov     w0, w22
    svc     #SVC_CLOSE_HANDLE
    keep
    mov     w0, w21
    svc     #SVC_CLOSE_HANDLE
    keep
    step    fsp

    // time:u: GetStandardUserSystemClock (CMIF 0), then ISystemClock GetCurrentTime (CMIF 0).
    addr    x0, name_time
    ldr     x0, [x0]
    bl      get_service
    mov     w21, w1
    mov     w27, w0
    mov     w0, w21
    mov     w1, #0
    mov     w2, #0
    bl      cmif_call
    keep
    mov     w22, w1
    mov     w0, w22
    mov     w1, #0
    mov     w2, #0
    bl      cmif_call
    keep
    mov     w0, w22
    svc     #SVC_CLOSE_HANDLE
    keep
    mov     w0, w21
    svc     #SVC_CLOSE_HANDLE
    keep
    step    time

    // hid: CreateAppletResource (CMIF 0, PID + applet resource user id 0).
    addr    x0, name_hid
    ldr     x0, [x0]
    bl      get_service
    mov     w21, w1
    mov     w27, w0
    mov     w0, w21
    mov     w1, #0
    mov     w2, #WITH_PID
    bl      cmif_call
    keep
    mov     w22, w1
    mov     w0, w22
    svc     #SVC_CLOSE_HANDLE
    keep
    mov     w0, w21
    svc     #SVC_CLOSE_HANDLE
    keep
    step    hid

    mov     w0, w20
    svc     #SVC_CLOSE_HANDLE
    svc     #SVC_EXIT_PROCESS

// get_service: x0 = service name (u64). Sends sm: GetServiceHandle (TIPC 16 + 1) on w20.
// Returns w0 = first non-zero result, w1 = session handle (0 when the send failed).
get_service:
    mov     x9, x0
    mov     w10, #17
    str     w10, [x19]
    mov     w10, #2                  // raw data: two words, the name
    str     w10, [x19, #4]
    str     x9, [x19, #8]
    mov     w0, w20
    svc     #SVC_SEND_SYNC_REQUEST
    mov     w1, #0
    cbnz    w0, 1f
    b       tipc_reply               // tail call: returns to get_service's caller
1:  ret

// tipc_reply: parses the TIPC reply in the TLS buffer. Returns w0 = result, w1 = first handle
// (0 if none).
tipc_reply:
    ldr     w10, [x19, #4]
    mov     x11, #8
    mov     w1, #0
    tbz     w10, #31, 3f
    ldr     w12, [x19, #8]
    add     x11, x11, #4
    tbz     w12, #0, 2f
    add     x11, x11, #8
2:  ubfx    w13, w12, #1, #4
    ubfx    w14, w12, #5, #4
    add     w13, w13, w14
    cbz     w13, 3f
    ldr     w1, [x19, x11]
    add     x11, x11, x13, lsl #2
3:  ldr     w0, [x19, x11]
    ret

// cmif_call: w0 = session, w1 = command id, w2 = WITH_PID to send a PID and one u64 of 0.
// Returns w0 = first non-zero result, w1 = first handle of the reply (0 if none).
cmif_call:
    mov     w9, w0
    mov     w10, #4
    str     w10, [x19]
    tbnz    w2, #0, 1f
    mov     w10, #8                  // 0x10 padding + 0x10 SFCI
    str     w10, [x19, #4]
    mov     x11, #0x10
    b       2f
1:  mov     w10, #10                 // 0x10 padding + 0x10 SFCI + u64
    orr     w10, w10, #0x80000000
    str     w10, [x19, #4]
    mov     w10, #1
    str     w10, [x19, #8]
    str     wzr, [x19, #0xC]
    str     wzr, [x19, #0x10]
    mov     x11, #0x20
    str     xzr, [x19, #0x30]
2:  add     x11, x19, x11
    ldr     w10, =0x49434653         // "SFCI"
    str     w10, [x11]
    str     wzr, [x11, #4]           // version 0, reserved
    str     w1, [x11, #8]
    str     wzr, [x11, #0xC]         // token
    mov     w0, w9
    svc     #SVC_SEND_SYNC_REQUEST
    cbnz    w0, 9f
    ldr     w10, [x19, #4]
    mov     x11, #8
    mov     w1, #0
    tbz     w10, #31, 4f
    ldr     w12, [x19, #8]
    add     x11, x11, #4
    tbz     w12, #0, 3f
    add     x11, x11, #8
3:  ubfx    w13, w12, #1, #4
    ubfx    w14, w12, #5, #4
    add     w13, w13, w14
    cbz     w13, 4f
    ldr     w1, [x19, x11]
    add     x11, x11, x13, lsl #2
4:  add     x11, x11, #15
    and     x11, x11, #~15
    add     x11, x19, x11
    ldr     w0, [x11, #8]            // SFCO result
9:  ret

// report: x0 = name, x1 = length, w2 = code. Prints "<name> ok" or "<name> fail XXXXXXXX".
report:
    addr    x3, line
    mov     x4, #0
1:  ldrb    w5, [x0, x4]
    strb    w5, [x3, x4]
    add     x4, x4, #1
    cmp     x4, x1
    b.ne    1b
    cbnz    w2, 2f
    addr    x6, ok_text
    mov     x7, #3
    b       3f
2:  addr    x6, fail_text
    mov     x7, #6
3:  ldrb    w5, [x6], #1
    strb    w5, [x3, x4]
    add     x4, x4, #1
    subs    x7, x7, #1
    b.ne    3b
    cbz     w2, 5f
    mov     w7, #28
4:  lsr     w5, w2, w7
    and     w5, w5, #0xF
    add     w8, w5, #'0'
    add     w9, w5, #('a' - 10)
    cmp     w5, #10
    csel    w5, w8, w9, lo
    strb    w5, [x3, x4]
    add     x4, x4, #1
    subs    w7, w7, #4
    b.pl    4b
5:  mov     x0, x3
    mov     x1, x4
    svc     #SVC_OUTPUT_DEBUG_STRING
    ret

// Worker thread. x0 = its block {handle, core, role}. Role 0 sets `ready` and signals the
// condition variable; role 1 waits for it. Both then add ROUNDS to `counter` under the mutex;
// the second to finish signals the event.
worker:
    mov     x19, x0
    ldr     w20, [x19]
    svc     #SVC_GET_CURRENT_PROCESSOR
    str     w0, [x19, #4]
    ldr     w21, [x19, #8]
    bl      lock
    cbnz    w21, 2f
    addr    x9, ready
    mov     w10, #1
    str     w10, [x9]
    addr    x0, condvar
    mov     w1, #1
    svc     #SVC_SIGNAL_KEY
    b       3f
2:  addr    x9, ready
    ldr     w10, [x9]
    cbnz    w10, 3f
    addr    x0, condvar
    addr    x1, mutex
    mov     w2, w20
    mov     x3, #-1
    svc     #SVC_WAIT_KEY
    b       2b
3:  bl      unlock
    mov     w22, #ROUNDS
4:  bl      lock
    addr    x9, counter
    ldr     w10, [x9]
    add     w10, w10, #1
    str     w10, [x9]
    bl      unlock
    subs    w22, w22, #1
    b.ne    4b
    bl      lock
    addr    x9, done
    ldr     w10, [x9]
    add     w10, w10, #1
    str     w10, [x9]
    cmp     w10, #2
    b.ne    5f
    addr    x9, event_w
    ldr     w0, [x9]
    svc     #SVC_SIGNAL_EVENT
5:  bl      unlock
    svc     #SVC_EXIT_THREAD

// lock: takes `mutex` for the thread whose handle is in w20.
lock:
1:  addr    x9, mutex
    ldaxr   w10, [x9]
    cbnz    w10, 2f
    stlxr   w11, w20, [x9]
    cbnz    w11, 1b
    ret
2:  tbnz    w10, #30, 3f
    orr     w12, w10, #WAITERS
    stlxr   w11, w12, [x9]
    cbnz    w11, 1b
    b       4f
3:  clrex
4:  and     w0, w10, #~WAITERS
    mov     x1, x9
    mov     w2, w20
    svc     #SVC_ARBITRATE_LOCK
    addr    x9, mutex
    ldr     w10, [x9]
    and     w10, w10, #~WAITERS
    cmp     w10, w20
    b.ne    1b
    ret

// unlock: releases `mutex`; the kernel hands it to a waiter when the waiters bit is set.
unlock:
    addr    x9, mutex
1:  ldxr    w10, [x9]
    cmp     w10, w20
    b.ne    2f
    stlxr   w11, wzr, [x9]
    cbnz    w11, 1b
    ret
2:  clrex
    mov     x0, x9
    svc     #SVC_ARBITRATE_UNLOCK
    ret

    .ltorg

    .section .rodata
tls_s:      .ascii  "tls"
tls_e:
event_s:    .ascii  "event"
event_e:
threads_s:  .ascii  "threads"
threads_e:
wait_s:     .ascii  "wait"
wait_e:
counter_s:  .ascii  "counter"
counter_e:
cores_s:    .ascii  "cores"
cores_e:
close_s:    .ascii  "close"
close_e:
sm_s:       .ascii  "sm"
sm_e:
set_s:      .ascii  "set"
set_e:
fsp_s:      .ascii  "fsp-srv"
fsp_e:
time_s:     .ascii  "time"
time_e:
hid_s:      .ascii  "hid"
hid_e:
ok_text:    .ascii  " ok"
fail_text:  .ascii  " fail "
sm_port:    .asciz  "sm:"
    .balign 8
name_set:   .ascii  "set"
    .space  5
name_fsp:   .ascii  "fsp-srv"
    .space  1
name_time:  .ascii  "time:u"
    .space  2
name_hid:   .ascii  "hid"
    .space  5

    .data
    .balign 8
block_a:    .word   0, 0, 0          // handle, core, role 0 (signals)
block_b:    .word   0, 0, 1          // handle, core, role 1 (waits)

    .bss
    .balign 16
mutex:      .space  4
condvar:    .space  4
ready:      .space  4
counter:    .space  4
done:       .space  4
event_w:    .space  4
wait_handles: .space 4
    .balign 16
line:       .space  32
    .balign 16
stack_a:    .space  STACK_SIZE
stack_b:    .space  STACK_SIZE
