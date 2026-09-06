# 第 07～08 课：最小 U-mode 入口、用户程序和两类静态栈资源。

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

    csrw sscratch, a3
    csrw sepc, a1
    csrw sstatus, a2

    # 保存 user_sp 到 t0；随后把用户可见整数寄存器清成已知初值。
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

    # 最后才切用户 sp；此前所有准备工作都使用可信 kernel stack。
    mv sp, t0
    mv t0, zero
    sret

# ---------- 第 07 课：一次 ecall 后停在内核 ----------
.section .text.user, "ax"
.balign 4
.globl user_start
user_start:
.globl user_entry
user_entry:
    li a7, 1
    li a0, 85              # ASCII 'U'

.globl user_ecall
user_ecall:
    ecall

.globl user_after_ecall
user_after_ecall:
    j user_after_ecall

.globl user_end
user_end:

# ---------- 第 08 课：多次 syscall，真正从内核恢复回用户 ----------
.balign 4
.globl user08_start
user08_start:
.globl user08_entry
user08_entry:
    # 用 callee-saved 寄存器和用户栈中的 sentinel 检查完整 TrapFrame 恢复。
    li s2, 0x2222333344445555
    li s3, 0x6666777788889999
    li t1, 0x1122334455667788
    sd t1, 0(sp)

    # 1) 未知 syscall：必须返回 -1，并从 ecall 后继续。
    li a7, 999
    li a0, 0
.globl user08_ecall_unknown
user08_ecall_unknown:
    ecall
    li t0, -1
    bne a0, t0, user08_fail

    # 2) putchar 参数超出 7-bit ASCII：必须返回 -2，不输出字符。
    li a7, 1
    li a0, 256
.globl user08_ecall_invalid
user08_ecall_invalid:
    ecall
    li t0, -2
    bne a0, t0, user08_fail

    # 两次 trap/sret 后，非返回值寄存器和用户栈内容必须仍然保持。
    li t0, 0x2222333344445555
    bne s2, t0, user08_fail
    li t0, 0x6666777788889999
    bne s3, t0, user08_fail
    ld t1, 0(sp)
    li t0, 0x1122334455667788
    bne t1, t0, user08_fail

    # 3) 三次合法 putchar。成功路径不做 kernel debug print，最终串口应连续出现 `OK\n`。
    li a7, 1
    li a0, 79              # 'O'
.globl user08_ecall_o
user08_ecall_o:
    ecall
    bnez a0, user08_fail

    li a7, 1
    li a0, 75              # 'K'
.globl user08_ecall_k
user08_ecall_k:
    ecall
    bnez a0, user08_fail

    li a7, 1
    li a0, 10              # newline
.globl user08_ecall_newline
user08_ecall_newline:
    ecall
    bnez a0, user08_fail

    # 成功 syscall 也必须完整恢复 s2/s3 和用户 stack sentinel。
    li t0, 0x2222333344445555
    bne s2, t0, user08_fail
    li t0, 0x6666777788889999
    bne s3, t0, user08_fail
    ld t1, 0(sp)
    li t0, 0x1122334455667788
    bne t1, t0, user08_fail

.globl user08_done
user08_done:
    # 第 09 课才实现 exit；本课完成后用户程序保持在 U-mode 自旋。
    j user08_done

user08_fail:
    # 如果返回值、sepc 或寄存器恢复有任何一项错误，尽量输出一个 `F` 作为负例证据。
    li a7, 1
    li a0, 70              # 'F'
    ecall
user08_fail_loop:
    j user08_fail_loop

.globl user08_end
user08_end:

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
