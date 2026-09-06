# 第 07 课：最小 U-mode 程序、第一次 sret 入口，以及两类静态栈资源。

# ---------- 从 S-mode 进入 U-mode ----------
.section .text.user_enter, "ax"
.balign 4
.globl enter_user_mode
enter_user_mode:
    # C ABI 参数：
    #   a0 = user_sp
    #   a1 = user_entry / sepc
    #   a2 = trusted sstatus for sret
    #   a3 = trusted trap_stack_top / sscratch

    # 用户代码可以改 gp/tp，所以在离开内核前把当前可信值存进 kernel-owned BSS。
    la t0, kernel_gp_saved
    sd gp, 0(t0)
    la t0, kernel_tp_saved
    sd tp, 0(t0)

    # 用户运行期间 stvec 必须指向会先切可信栈的 user_trap_entry。
    la t0, user_trap_entry
    csrw stvec, t0

    # sscratch 保存可信 trap stack top；用户自己不能通过普通指令修改这个 CSR。
    csrw sscratch, a3

    # sret 会从 sepc 取下一条 PC，并根据 sstatus.SPP 决定返回特权级。
    csrw sepc, a1
    csrw sstatus, a2

    # 保存 user_sp 到 t0；下面会把其他用户可见整数寄存器清成已知初值。
    mv t0, a0

    mv ra, zero
    mv gp, zero
    mv tp, zero
    # t0 暂时保留 user_sp
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

    # 最后才切用户 sp；此前所有准备工作都使用可信 kernel stack。
    mv sp, t0
    mv t0, zero

    # 这不是普通 ret：CPU 按 sepc/sstatus 完成 S -> U 特权级返回。
    sret

# ---------- 受控用户程序 ----------
.section .text.user, "ax"
.balign 4
.globl user_start
user_start:
.globl user_entry
user_entry:
    # 第 08 课才会真正把 a7=1 解释成 putchar syscall。
    # 本课只利用这些值证明用户寄存器确实执行到了 ecall 前。
    li a7, 1
    li a0, 85              # ASCII 'U'

.globl user_ecall
user_ecall:
    ecall

    # 第 07 课内核不会返回用户。如果这里真的执行，停在明确失败循环。
.globl user_after_ecall
user_after_ecall:
    j user_after_ecall

.globl user_end
user_end:

# ---------- 用户栈：U-mode 的 sp ----------
.section .bss.user_stack, "aw", @nobits
.balign 16
.globl user_stack_start
user_stack_start:
    .space 16384
.globl user_stack_end
user_stack_end:

# ---------- 可信 trap 栈：用户 trap 后 S-mode 立即切换到这里 ----------
.section .bss.user_trap_stack, "aw", @nobits
.balign 16
.globl user_trap_stack_start
user_trap_stack_start:
    .space 16384
.globl user_trap_stack_end
user_trap_stack_end:

# ---------- 进入用户态前保存的可信 kernel ABI 环境 ----------
.section .bss.kernel_env, "aw", @nobits
.balign 8
.globl kernel_gp_saved
kernel_gp_saved:
    .space 8
.globl kernel_tp_saved
kernel_tp_saved:
    .space 8
