# 第 07～09 课：U-mode 进入桥、用户程序和可信运行资源。

# ---------- 第 07/08 课：单向 enter_user_mode ----------
# 这个入口不会正常 ret；它适合“本次教学实验最终停在用户/trap 路径”的课次。
.section .text.user_enter, "ax"
.balign 4
.globl enter_user_mode
enter_user_mode:
    # a0=user_sp, a1=entry, a2=sstatus, a3=trap_stack_top
    la t0, kernel_gp_saved
    sd gp, 0(t0)
    la t0, kernel_tp_saved
    sd tp, 0(t0)

    la t0, user_trap_entry
    csrw stvec, t0
    csrw sscratch, a3
    csrw sepc, a1
    csrw sstatus, a2

    mv t0, a0

    mv ra, zero
    mv gp, zero
    mv tp, zero
    mv t1, zero
    mv t2, zero
    mv s0, zero
    mv s1, zero
    mv a0, zero
    mv a1, zero
    mv a2, zero
    mv a3, zero
    mv a4, zero
    mv a5, zero
    mv a6, zero
    mv a7, zero
    mv s2, zero
    mv s3, zero
    mv s4, zero
    mv s5, zero
    mv s6, zero
    mv s7, zero
    mv s8, zero
    mv s9, zero
    mv s10, zero
    mv s11, zero
    mv t3, zero
    mv t4, zero
    mv t5, zero
    mv t6, zero

    mv sp, t0
    mv t0, zero
    sret

# ---------- 第 09 课：成对的 run_user ----------
# Rust 管理代码用普通 C ABI call 进入这里；有效 exit 后 trap.S 恢复本函数保存的
# KernelContext 并 ret，于是原 Rust caller 像普通函数调用一样重新获得控制。
.balign 4
.globl run_user
run_user:
    # a0=user_sp, a1=entry, a2=sstatus, a3=trap_stack_top
    # 先保存调用者要求跨 call 保持的内核现场。
    la t0, kernel_context
    sd ra, {kctx_ra}(t0)
    sd sp, {kctx_sp}(t0)
    sd gp, {kctx_gp}(t0)
    sd tp, {kctx_tp}(t0)
    sd s0, {kctx_s0}(t0)
    sd s1, {kctx_s1}(t0)
    sd s2, {kctx_s2}(t0)
    sd s3, {kctx_s3}(t0)
    sd s4, {kctx_s4}(t0)
    sd s5, {kctx_s5}(t0)
    sd s6, {kctx_s6}(t0)
    sd s7, {kctx_s7}(t0)
    sd s8, {kctx_s8}(t0)
    sd s9, {kctx_s9}(t0)
    sd s10, {kctx_s10}(t0)
    sd s11, {kctx_s11}(t0)

    # 用户 trap 期间 Rust 需要恢复当前内核 gp/tp；仍保留第 07 课的专门可信副本。
    la t0, kernel_gp_saved
    sd gp, 0(t0)
    la t0, kernel_tp_saved
    sd tp, 0(t0)

    # 接下来和 enter_user_mode 一样构造可信 sret 状态。
    la t0, user_trap_entry
    csrw stvec, t0
    csrw sscratch, a3
    csrw sepc, a1
    csrw sstatus, a2

    mv t0, a0

    mv ra, zero
    mv gp, zero
    mv tp, zero
    mv t1, zero
    mv t2, zero
    mv s0, zero
    mv s1, zero
    mv a0, zero
    mv a1, zero
    mv a2, zero
    mv a3, zero
    mv a4, zero
    mv a5, zero
    mv a6, zero
    mv a7, zero
    mv s2, zero
    mv s3, zero
    mv s4, zero
    mv s5, zero
    mv s6, zero
    mv s7, zero
    mv s8, zero
    mv s9, zero
    mv s10, zero
    mv s11, zero
    mv t3, zero
    mv t4, zero
    mv t5, zero
    mv t6, zero

    mv sp, t0
    mv t0, zero
    sret

# ---------- 第 07 课：一次 ecall ----------
.section .text.user, "ax"
.balign 4
.globl user_start
user_start:
.globl user_entry
user_entry:
    li a7, 1
    li a0, 85
.globl user_ecall
user_ecall:
    ecall
.globl user_after_ecall
user_after_ecall:
    j user_after_ecall
.globl user_end
user_end:

# ---------- 第 08 课：可恢复 syscall ----------
.balign 4
.globl user08_start
user08_start:
.globl user08_entry
user08_entry:
    li s2, 0x2222333344445555
    li s3, 0x6666777788889999
    li t1, 0x1122334455667788
    sd t1, 0(sp)

    li a7, 999
    li a0, 0
.globl user08_ecall_unknown
user08_ecall_unknown:
    ecall
    li t0, -1
    bne a0, t0, user08_fail

    li a7, 1
    li a0, 256
.globl user08_ecall_invalid
user08_ecall_invalid:
    ecall
    li t0, -2
    bne a0, t0, user08_fail

    li t0, 0x2222333344445555
    bne s2, t0, user08_fail
    li t0, 0x6666777788889999
    bne s3, t0, user08_fail
    ld t1, 0(sp)
    li t0, 0x1122334455667788
    bne t1, t0, user08_fail

    li a7, 1
    li a0, 79
.globl user08_ecall_o
user08_ecall_o:
    ecall
    bnez a0, user08_fail

    li a7, 1
    li a0, 75
.globl user08_ecall_k
user08_ecall_k:
    ecall
    bnez a0, user08_fail

    li a7, 1
    li a0, 10
.globl user08_ecall_newline
user08_ecall_newline:
    ecall
    bnez a0, user08_fail

    li t0, 0x2222333344445555
    bne s2, t0, user08_fail
    li t0, 0x6666777788889999
    bne s3, t0, user08_fail
    ld t1, 0(sp)
    li t0, 0x1122334455667788
    bne t1, t0, user08_fail

.globl user08_done
user08_done:
    j user08_done

user08_fail:
    li a7, 1
    li a0, 70
    ecall
user08_fail_loop:
    j user08_fail_loop
.globl user08_end
user08_end:

# ---------- 第 09 课 A：非法 exit 返回；合法 exit 不返回 ----------
.balign 4
.globl user09_start
user09_start:
.globl user09_entry
user09_entry:
    # exit(256) 非法：必须像普通错误 syscall 一样返回 -2。
    li a7, 2
    li a0, 256
.globl user09_ecall_invalid_exit
user09_ecall_invalid_exit:
    ecall
    li t0, -2
    bne a0, t0, user09_fail

    # exit(7) 合法：这条 ecall 后面的任何用户指令都不应该执行。
    li a7, 2
    li a0, 7
.globl user09_ecall_exit7
user09_ecall_exit7:
    ecall

    # 如果错误实现把有效 exit 当成 ResumeUser，打印一个非常明确的失败行。
    li a7, 1
    li a0, 66              # B
    ecall
    li a7, 1
    li a0, 65              # A
    ecall
    li a7, 1
    li a0, 68              # D
    ecall
    li a7, 1
    li a0, 95              # _
    ecall
    li a7, 1
    li a0, 65              # A
    ecall
    li a7, 1
    li a0, 70              # F
    ecall
    li a7, 1
    li a0, 84              # T
    ecall
    li a7, 1
    li a0, 69              # E
    ecall
    li a7, 1
    li a0, 82              # R
    ecall
    li a7, 1
    li a0, 95              # _
    ecall
    li a7, 1
    li a0, 69              # E
    ecall
    li a7, 1
    li a0, 88              # X
    ecall
    li a7, 1
    li a0, 73              # I
    ecall
    li a7, 1
    li a0, 84              # T
    ecall
    li a7, 1
    li a0, 10
    ecall

user09_fail:
    j user09_fail
.globl user09_end
user09_end:

# ---------- 第 09 课 B：100 次顺序运行用的最短 exit(0) 程序 ----------
.balign 4
.globl user09_stress_start
user09_stress_start:
.globl user09_stress_entry
user09_stress_entry:
    li a7, 2
    li a0, 0
.globl user09_stress_ecall
user09_stress_ecall:
    ecall
.globl user09_stress_after_exit
user09_stress_after_exit:
    j user09_stress_after_exit
.globl user09_stress_end
user09_stress_end:

# ---------- 用户栈 ----------
.section .bss.user_stack, "aw", @nobits
.balign 16
.globl user_stack_start
user_stack_start:
    .space 16384
.globl user_stack_end
user_stack_end:

# ---------- 可信 trap 栈 ----------
.section .bss.user_trap_stack, "aw", @nobits
.balign 16
.globl user_trap_stack_start
user_trap_stack_start:
    .space 16384
.globl user_trap_stack_end
user_trap_stack_end:

# ---------- 用户运行期间恢复 kernel gp/tp 的可信副本 ----------
.section .bss.kernel_env, "aw", @nobits
.balign 8
.globl kernel_gp_saved
kernel_gp_saved:
    .space 8
.globl kernel_tp_saved
kernel_tp_saved:
    .space 8

# ---------- 第 09 课 run_user 的持久 KernelContext ----------
.section .bss.kernel_context, "aw", @nobits
.balign 16
.globl kernel_context
kernel_context:
    .space {kctx_size}

.section .text.user_enter, "ax"
.balign 4
.globl run_task
run_task:
    # a0 points to this task's fully initialized, persistent trap-stack frame.
    la t0, kernel_context
    sd ra, {kctx_ra}(t0)
    sd sp, {kctx_sp}(t0)
    sd gp, {kctx_gp}(t0)
    sd tp, {kctx_tp}(t0)
    sd s0, {kctx_s0}(t0)
    sd s1, {kctx_s1}(t0)
    sd s2, {kctx_s2}(t0)
    sd s3, {kctx_s3}(t0)
    sd s4, {kctx_s4}(t0)
    sd s5, {kctx_s5}(t0)
    sd s6, {kctx_s6}(t0)
    sd s7, {kctx_s7}(t0)
    sd s8, {kctx_s8}(t0)
    sd s9, {kctx_s9}(t0)
    sd s10, {kctx_s10}(t0)
    sd s11, {kctx_s11}(t0)

    # 用户 trap 期间 Rust 需要恢复当前内核 gp/tp；仍保留第 07 课的专门可信副本。
    la t0, kernel_gp_saved
    sd gp, 0(t0)
    la t0, kernel_tp_saved
    sd tp, 0(t0)

    mv sp, a0
    j restore_user_frame
