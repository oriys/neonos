# 第 10 课开始建立最小用户 ABI wrapper。
#
# 这些函数运行在 U-mode，只负责把普通函数参数变成 neonos syscall ABI：
#   a0 = arg0 / return value
#   a7 = syscall number
# 然后执行 ecall。它们不是内核函数，也不直接访问 UART/CSR。

.section .text.user_api, "ax"
.balign 4
.globl user_api_start
user_api_start:

.globl user_api_putchar
user_api_putchar:
    li a7, 1
.globl user_api_putchar_ecall
user_api_putchar_ecall:
    ecall
    ret

.globl user_api_exit
user_api_exit:
    li a7, 2
.globl user_api_exit_ecall
user_api_exit_ecall:
    ecall
    ret

.globl user_api_end
user_api_end:
