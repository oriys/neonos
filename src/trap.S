# 第 05 课：S-mode trap 的最小汇编入口。
#
# 这段代码故意只负责 CPU 刚进入 trap 后 Rust 还不能安全做的事情：
# 1. 在当前可信 kernel stack 上分配 TrapFrame；
# 2. 保存 trap 发生瞬间的 32 个整数寄存器槽位；
# 3. 保存 sstatus/sepc/scause/stval；
# 4. 把完整 frame 指针按 C ABI 放进 a0；
# 5. 调用不会返回的 rust_trap_handler。
#
# 本课还不恢复寄存器，也不执行 sret。

.section .text.trap, "ax"
.balign 4
.globl trap_entry
trap_entry:
    # TrapFrame 是 288 bytes。进入 trap 时当前 sp 按 ABI 为 16-byte 对齐；
    # 288 = 18 * 16，因此分配 frame 后仍保持 16-byte 对齐。
    addi sp, sp, -{tf_size}

    # 最容易写错的顺序：我们接下来要借 t0 计算“trap 前的旧 sp”，
    # 因此必须先把 trap 发生时原始 t0 保存下来。
    sd t0, {x5}(sp)

    # 现在 t0 才可以被安全当作临时寄存器。
    # 当前 sp 已经下降 tf_size，所以旧 sp = sp + tf_size。
    addi t0, sp, {tf_size}
    sd t0, {x2}(sp)

    # x0 硬件上永远读取为 0；仍保留一个槽位，让“寄存器编号=数组下标”。
    sd zero, {x0}(sp)

    # 保存剩余整数寄存器。x2 已保存旧 sp，x5 已保存原始 t0，因此跳过它们。
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

    # 所有整数寄存器已经安全落盘后，复用 t0 读取关键 CSR。
    csrr t0, sstatus
    sd t0, {sstatus}(sp)
    csrr t0, sepc
    sd t0, {sepc}(sp)
    csrr t0, scause
    sd t0, {scause}(sp)
    csrr t0, stval
    sd t0, {stval}(sp)

    # C ABI 第一个参数放在 a0。这里把完整 TrapFrame 的起始地址交给 Rust。
    mv a0, sp
    call rust_trap_handler

    # Rust handler 的签名是 -> !，理论上不会回来。
    # 如果未来实现错误真的返回，仍留一个安全兜底，不继续执行未知内存。
1:
    wfi
    j 1b

# 受控非法指令触发函数只给第 05 课 test feature 使用。
# Rust 普通 call 会把返回地址放入 ra，但第一条指令就触发异常，因此不会 ret。
.balign 4
.globl trigger_illegal_instruction
trigger_illegal_instruction:
.globl trap_trigger_label
trap_trigger_label:
    # 当前目标启用了 C 扩展；全 0 的最低 16-bit 编码是非法指令。
    # 用 4 bytes 零只是为了让触发位置在反汇编/符号中更明显；本课不尝试跳过它返回。
    .4byte 0
    ret
