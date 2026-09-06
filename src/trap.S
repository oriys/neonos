# 第 05～10 课共用的 S-mode trap 汇编。
#
# `trap_entry`：内核自身故障入口，进入时 sp 已可信；只报告并停机。
# `user_trap_entry`：用户来源入口，先切可信 trap 栈，再保存完整用户现场。
# Rust user trap dispatcher 返回：
#   a0 = 0  -> ResumeUser：从 TrapFrame 恢复并 sret
#   a0 = 1  -> TerminateProcess：放弃用户现场，恢复 run_user 保存的 KernelContext
#
# KernelContext 固定大小由 Rust 注入：{kctx_size} bytes。

.section .text.trap, "ax"
.balign 4
.globl trap_entry
trap_entry:
    addi sp, sp, -{tf_size}

    sd t0, {x5}(sp)
    addi t0, sp, {tf_size}
    sd t0, {x2}(sp)

    sd zero, {x0}(sp)
    sd ra, {x1}(sp)
    sd gp, {x3}(sp)
    sd tp, {x4}(sp)
    sd t1, {x6}(sp)
    sd t2, {x7}(sp)
    sd s0, {x8}(sp)
    sd s1, {x9}(sp)
    sd a0, {x10}(sp)
    sd a1, {x11}(sp)
    sd a2, {x12}(sp)
    sd a3, {x13}(sp)
    sd a4, {x14}(sp)
    sd a5, {x15}(sp)
    sd a6, {x16}(sp)
    sd a7, {x17}(sp)
    sd s2, {x18}(sp)
    sd s3, {x19}(sp)
    sd s4, {x20}(sp)
    sd s5, {x21}(sp)
    sd s6, {x22}(sp)
    sd s7, {x23}(sp)
    sd s8, {x24}(sp)
    sd s9, {x25}(sp)
    sd s10, {x26}(sp)
    sd s11, {x27}(sp)
    sd t3, {x28}(sp)
    sd t4, {x29}(sp)
    sd t5, {x30}(sp)
    sd t6, {x31}(sp)

    csrr t0, sstatus
    sd t0, {sstatus}(sp)
    csrr t0, sepc
    sd t0, {sepc}(sp)
    csrr t0, scause
    sd t0, {scause}(sp)
    csrr t0, stval
    sd t0, {stval}(sp)

    mv a0, sp
    call rust_trap_handler

1:
    wfi
    j 1b

# ---------- U-mode 来源 trap ----------
.balign 4
.globl user_trap_entry
user_trap_entry:
    # 第一条先脱离 user-controlled sp。
    csrrw sp, sscratch, sp
    addi sp, sp, -{tf_size}

    # 复用前先保存用户值。
    sd t0, {x5}(sp)
    sd gp, {x3}(sp)
    sd tp, {x4}(sp)

    csrr t0, sscratch
    sd t0, {x2}(sp)

    # S-mode handler 内再次 fault 时走独立 kernel trap path。
    la t0, trap_entry
    csrw stvec, t0

    # 恢复可信 kernel ABI 的 gp/tp 后才能进入 Rust。
    la t0, kernel_gp_saved
    ld gp, 0(t0)
    la t0, kernel_tp_saved
    ld tp, 0(t0)

    sd zero, {x0}(sp)
    sd ra, {x1}(sp)
    sd t1, {x6}(sp)
    sd t2, {x7}(sp)
    sd s0, {x8}(sp)
    sd s1, {x9}(sp)
    sd a0, {x10}(sp)
    sd a1, {x11}(sp)
    sd a2, {x12}(sp)
    sd a3, {x13}(sp)
    sd a4, {x14}(sp)
    sd a5, {x15}(sp)
    sd a6, {x16}(sp)
    sd a7, {x17}(sp)
    sd s2, {x18}(sp)
    sd s3, {x19}(sp)
    sd s4, {x20}(sp)
    sd s5, {x21}(sp)
    sd s6, {x22}(sp)
    sd s7, {x23}(sp)
    sd s8, {x24}(sp)
    sd s9, {x25}(sp)
    sd s10, {x26}(sp)
    sd s11, {x27}(sp)
    sd t3, {x28}(sp)
    sd t4, {x29}(sp)
    sd t5, {x30}(sp)
    sd t6, {x31}(sp)

    csrr t0, sstatus
    sd t0, {sstatus}(sp)
    csrr t0, sepc
    sd t0, {sepc}(sp)
    csrr t0, scause
    sd t0, {scause}(sp)
    csrr t0, stval
    sd t0, {stval}(sp)

    # a0 返回的是“汇编下一步做什么”，不是用户 syscall result。
    # 用户 result 已由 Rust 写回 frame.x10。
    mv a0, sp
    call rust_user_trap_dispatch

    li t0, 1
    beq a0, t0, terminate_process

    # ---------- ResumeUser ----------
    ld t0, {sepc}(sp)
    csrw sepc, t0
    ld t0, {sstatus}(sp)
    csrw sstatus, t0

    # x2 也从修改后的 frame 恢复；最后用 sscratch 与 trusted sp 交换。
    ld t0, {x2}(sp)
    csrw sscratch, t0

    ld ra, {x1}(sp)
    ld gp, {x3}(sp)
    ld tp, {x4}(sp)
    ld t1, {x6}(sp)
    ld t2, {x7}(sp)
    ld s0, {x8}(sp)
    ld s1, {x9}(sp)
    ld a0, {x10}(sp)
    ld a1, {x11}(sp)
    ld a2, {x12}(sp)
    ld a3, {x13}(sp)
    ld a4, {x14}(sp)
    ld a5, {x15}(sp)
    ld a6, {x16}(sp)
    ld a7, {x17}(sp)
    ld s2, {x18}(sp)
    ld s3, {x19}(sp)
    ld s4, {x20}(sp)
    ld s5, {x21}(sp)
    ld s6, {x22}(sp)
    ld s7, {x23}(sp)
    ld s8, {x24}(sp)
    ld s9, {x25}(sp)
    ld s10, {x26}(sp)
    ld s11, {x27}(sp)
    ld t3, {x28}(sp)
    ld t4, {x29}(sp)
    ld t5, {x30}(sp)
    ld t6, {x31}(sp)

    # 尽量靠近 sret 才重新允许下一次 user trap 使用 user_trap_entry。
    la t0, user_trap_entry
    csrw stvec, t0
    ld t0, {x5}(sp)

    addi sp, sp, {tf_size}
    csrrw sp, sscratch, sp
    sret

# ---------- TerminateProcess ----------
# 到这里时 Rust trap handler 已经正常 return，所有 Rust 局部借用/栈帧都已结束。
# 当前 CPU 仍在可信 trap stack 上，因此可以安全放弃整个用户 TrapFrame，
# 恢复进入用户之前 `run_user` 保存的内核调用现场。
terminate_process:
    # 管理模式不再需要“下次用户 trap 的栈顶”语义。
    csrw sscratch, zero

    la t0, kernel_context

    ld ra, {kctx_ra}(t0)
    ld gp, {kctx_gp}(t0)
    ld tp, {kctx_tp}(t0)
    ld s0, {kctx_s0}(t0)
    ld s1, {kctx_s1}(t0)
    ld s2, {kctx_s2}(t0)
    ld s3, {kctx_s3}(t0)
    ld s4, {kctx_s4}(t0)
    ld s5, {kctx_s5}(t0)
    ld s6, {kctx_s6}(t0)
    ld s7, {kctx_s7}(t0)
    ld s8, {kctx_s8}(t0)
    ld s9, {kctx_s9}(t0)
    ld s10, {kctx_s10}(t0)
    ld s11, {kctx_s11}(t0)

    # sp 最后恢复。t1 是 caller-saved，不属于 KernelContext 的持久 ABI 责任。
    ld t1, {kctx_sp}(t0)
    mv sp, t1

    # `ret` 使用刚恢复的 ra，真正返回当初正常 call run_user 的 Rust 管理函数。
    ret

# ---------- 第 05 课受控非法指令 ----------
.balign 4
.globl trigger_illegal_instruction
trigger_illegal_instruction:
.globl trap_trigger_label
trap_trigger_label:
    .4byte 0
    ret
