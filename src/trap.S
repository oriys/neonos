# 第 05～08 课共用的 S-mode trap 汇编。
#
# `trap_entry`：内核自身故障入口，进入时 sp 已可信；只报告并停机。
# `user_trap_entry`：用户来源入口，先切可信 trap 栈，再保存完整用户现场。
# 第 08 课开始，Rust handler 对可恢复 syscall 会返回；汇编再从同一个 TrapFrame
# 恢复用户寄存器、sepc/sstatus/sscratch，并执行 sret。

.section .text.trap, "ax"
.balign 4
.globl trap_entry
trap_entry:
    addi sp, sp, -{tf_size}

    # 必须先保存原始 t0，之后才能把 t0 借作临时寄存器。
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
# 用户运行期间：
#   sp       = user stack
#   sscratch = trusted trap_stack_top
.balign 4
.globl user_trap_entry
user_trap_entry:
    # 第一个动作不信任 user sp：交换后当前 sp 立即变成 trusted trap stack top，
    # 原 user sp 则进入 sscratch。
    csrrw sp, sscratch, sp

    addi sp, sp, -{tf_size}

    # 在复用这些寄存器前先保存用户原值。
    sd t0, {x5}(sp)
    sd gp, {x3}(sp)
    sd tp, {x4}(sp)

    # 保存 trap 前的 user sp。
    csrr t0, sscratch
    sd t0, {x2}(sp)

    # 从这里起如果内核自己故障，应进入 kernel trap_entry，而不是再次按用户来源换栈。
    la t0, trap_entry
    csrw stvec, t0

    # 用户可以任意改 gp/tp；调用 Rust 前恢复可信 kernel ABI 环境。
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

    # Rust 现在可以：
    # - 第 07 课：打印证据并永远停住；
    # - 第 08 课：修改 frame.a0 / frame.sepc 后正常 return。
    mv a0, sp
    call rust_user_trap_handler

    # ---------- 第 08 课恢复路径 ----------
    # Rust 只有在“这是可恢复的用户 syscall”时才会返回到这里。

    # 先从修改后的 TrapFrame 恢复受内核控制的 CSR。
    ld t0, {sepc}(sp)
    csrw sepc, t0
    ld t0, {sstatus}(sp)
    csrw sstatus, t0

    # x2 也属于 TrapFrame 的用户现场。把它写进 sscratch，最后再交换回真正 sp。
    ld t0, {x2}(sp)
    csrw sscratch, t0

    # 在仍使用可信 trap stack 时，先恢复所有不影响当前 frame 基址的用户寄存器。
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

    # 用户即将继续运行，下一次用户 trap 必须重新走“先换可信栈”的入口。
    # 把这一步放得尽可能靠近 sret，缩短 S-mode 使用 user_trap_entry 的窗口。
    la t0, user_trap_entry
    csrw stvec, t0

    # t0 是最后一个需要从 frame 恢复的普通寄存器；之后不再借用它。
    ld t0, {x5}(sp)

    # 回到 trap_stack_top 后，把 sp 与 sscratch 原子交换：
    #   sp       <- frame.x2（用户 sp）
    #   sscratch <- trap_stack_top（下次 trap 的可信入口栈）
    addi sp, sp, {tf_size}
    csrrw sp, sscratch, sp

    # `sepc` 已由 Rust 推进到 ecall 后；所有用户寄存器也已经从 frame 恢复。
    sret

# ---------- 第 05 课受控非法指令 ----------
.balign 4
.globl trigger_illegal_instruction
trigger_illegal_instruction:
.globl trap_trigger_label
trap_trigger_label:
    .4byte 0
    ret
