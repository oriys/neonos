# 第 05～07 课共用的 S-mode trap 汇编。
#
# `trap_entry`：第 05 课的内核自身故障入口，进入时 sp 已经可信。
# `user_trap_entry`：第 07 课的用户来源入口，进入第一步先把 user sp 换成可信 trap stack。
#
# 两个入口都使用 Rust `TrapFrame` 注入的同一组 offset 常量。

.section .text.trap, "ax"
.balign 4
.globl trap_entry
trap_entry:
    # 第 05 课内核 trap：当前 sp 已经是可信 kernel stack。
    addi sp, sp, -{tf_size}

    # 先保存原始 t0，再把它当临时寄存器恢复“分配 frame 前”的旧 sp。
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

# 第 07 课：来自 U-mode 的 trap 不能继续使用用户 sp。
# 返回用户前约定：
#   sp       = user stack
#   sscratch = trusted trap_stack_top
.balign 4
.globl user_trap_entry
user_trap_entry:
    # 不借任何普通临时寄存器，先原子交换两个 sp 值：
    #   sp       <- trusted trap_stack_top
    #   sscratch <- original user_sp
    csrrw sp, sscratch, sp

    # 现在才在可信 trap stack 上分配 TrapFrame。
    addi sp, sp, -{tf_size}

    # t0、用户 gp、用户 tp 都必须在被内核复用前先保存。
    sd t0, {x5}(sp)
    sd gp, {x3}(sp)
    sd tp, {x4}(sp)

    # 原 user sp 由刚才的交换保存在 sscratch，不是当前可信 sp。
    csrr t0, sscratch
    sd t0, {x2}(sp)

    # 已保存 t0 后，尽早把后续 S-mode 故障入口切回第 05 课的 kernel trap entry。
    la t0, trap_entry
    csrw stvec, t0

    # 用户可能任意改 gp/tp。调用 Rust 前从可信 kernel-owned 存储恢复。
    la t0, kernel_gp_saved
    ld gp, 0(t0)
    la t0, kernel_tp_saved
    ld tp, 0(t0)

    # 保存剩余用户整数寄存器。gp/tp/t0/sp 的“用户值”已经在上面保存。
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

    # 硬件已经把“trap 来源特权级”等信息写进 sstatus；此处保存真实结果。
    csrr t0, sstatus
    sd t0, {sstatus}(sp)
    csrr t0, sepc
    sd t0, {sepc}(sp)
    csrr t0, scause
    sd t0, {scause}(sp)
    csrr t0, stval
    sd t0, {stval}(sp)

    # 完整 frame 已就绪，调用不会返回的用户 trap Rust handler。
    mv a0, sp
    call rust_user_trap_handler

2:
    wfi
    j 2b

# 第 05 课受控非法指令触发函数。
.balign 4
.globl trigger_illegal_instruction
trigger_illegal_instruction:
.globl trap_trigger_label
trap_trigger_label:
    .4byte 0
    ret
