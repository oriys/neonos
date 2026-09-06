# 第 10 课：用户 fault 与 fault 后继续运行另一个正常 Process。

.section .text.user10, "ax"

# ---------- A：受控用户非法指令 ----------
.balign 4
.globl user10_fault_start
user10_fault_start:
.globl user10_fault_entry
user10_fault_entry:
.globl user10_fault_instruction
user10_fault_instruction:
    .4byte 0
user10_fault_after:
    j user10_fault_after
.globl user10_fault_end
user10_fault_end:

# ---------- B：fault 后启动的正常用户程序 ----------
.balign 4
.globl user10_normal_start
user10_normal_start:
.globl user10_normal_entry
user10_normal_entry:
    call user10_normal_main
    call user_api_exit

    # 合法 exit(0) 如果错误返回，输出 BAD_EXIT_RETURN 并停住。
    li a0, 66
    call user_api_putchar
    li a0, 65
    call user_api_putchar
    li a0, 68
    call user_api_putchar
    li a0, 95
    call user_api_putchar
    li a0, 69
    call user_api_putchar
    li a0, 88
    call user_api_putchar
    li a0, 73
    call user_api_putchar
    li a0, 84
    call user_api_putchar
    li a0, 95
    call user_api_putchar
    li a0, 82
    call user_api_putchar
    li a0, 69
    call user_api_putchar
    li a0, 84
    call user_api_putchar
    li a0, 85
    call user_api_putchar
    li a0, 82
    call user_api_putchar
    li a0, 78
    call user_api_putchar
    li a0, 10
    call user_api_putchar
1:
    j 1b

user10_normal_main:
    # 用户 main 只依赖 user_api，不知道 UART 地址和 trap 实现。
    li a0, 65
    call user_api_putchar
    li a0, 70
    call user_api_putchar
    li a0, 84
    call user_api_putchar
    li a0, 69
    call user_api_putchar
    li a0, 82
    call user_api_putchar
    li a0, 95
    call user_api_putchar
    li a0, 70
    call user_api_putchar
    li a0, 65
    call user_api_putchar
    li a0, 85
    call user_api_putchar
    li a0, 76
    call user_api_putchar
    li a0, 84
    call user_api_putchar
    li a0, 95
    call user_api_putchar
    li a0, 79
    call user_api_putchar
    li a0, 75
    call user_api_putchar
    li a0, 10
    call user_api_putchar

    li a0, 0
    ret

.globl user10_normal_end
user10_normal_end:
