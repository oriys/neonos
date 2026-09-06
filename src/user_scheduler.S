# This blob explicitly uses RV64M, which is part of the configured RV64GC target.
.option push
.option arch, +m
.section .text.user_scheduler, "ax"
.balign 4
.globl sched_code_start
sched_code_start:
# Inputs: s0=marker, s1=iterations, s4=mode, s5=timer flag PA (lesson 13 only).
# a3 holds a private register sentinel; stack and s2 counter survive all traps.
.globl sched_entry
sched_entry:
    li a3, 0x12345678
    add t0, a3, s0
    sd t0, 0(sp)
    li s2, 0
    li s3, 0
sched_loop:
    li t0, 0x12345678
    bne a3, t0, sched_fail
    add t0, a3, s0
    ld t1, 0(sp)
    bne t0, t1, sched_fail
    addi s2, s2, 1
    add s3, s3, s2
    # mode 0: trace/yield, 1: long no-syscall computation,
    # 2: high-frequency yield, 3: ordinary syscall, 4: wait for timer flag.
    li t0, 1
    beq s4, t0, sched_next
    li t0, 4
    beq s4, t0, sched_timer_wait
    li t0, 3
    beq s4, t0, sched_syscall
    li t0, 2
    beq s4, t0, sched_yield
    mv a0, s0
    li a7, 1
    ecall
    bnez a0, sched_fail
sched_yield:
    # A finite user CPU burst keeps allotment measurable against kernel overhead.
    li t2, 10000
1:
    addi t2, t2, -1
    bnez t2, 1b
    li a7, 3
    ecall
    bnez a0, sched_fail
    j sched_next
sched_syscall:
    li a7, 999
    ecall
    li t0, -1
    bne a0, t0, sched_fail
sched_next:
    bltu s2, s1, sched_loop
    # independent closed-form checksum in user registers
    addi t0, s1, 1
    mul t0, t0, s1
    srli t0, t0, 1
    bne s3, t0, sched_fail
    li a0, 0
    li a7, 2
    ecall
    j sched_fail
sched_timer_wait:
    ld t0, 0(s5)
    beqz t0, sched_loop
    # verify sentinel again after timer stopped, then finish.
    li t0, 0x12345678
    bne a3, t0, sched_fail
    add t0, a3, s0
    ld t1, 0(sp)
    bne t0, t1, sched_fail
    li a0, 0
    li a7, 2
    ecall
sched_fail:
    li a0, 99
    li a7, 2
    ecall
    j sched_fail
.globl sched_fault
sched_fault:
    .4byte 0
    j sched_fail
.globl sched_code_end
sched_code_end:

.option pop
