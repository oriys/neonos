/* 最终生成的是 RISC-V ELF。 */
OUTPUT_ARCH(riscv)

/* ELF 入口仍然是最早期汇编符号 `_start`。 */
ENTRY(_start)

SECTIONS
{
    /* 当前 QEMU + OpenSBI 教学启动约定把内核放在 0x80200000 一带。 */
    . = 0x80200000;

    /* 代码区域使用半开区间 [stext, etext)。 */
    .text : ALIGN(4K) {
        stext = .;

        /* 各课的关键入口明确收集，不依赖后面的宽泛通配恰好命中。 */
        KEEP(*(.text.entry))
        KEEP(*(.text.trap))
        KEEP(*(.text.user_enter))
        KEEP(*(.text.user))
        KEEP(*(.text.model_program))

        *(.text .text.*)
        etext = .;
    }

    /* 只读数据区域 [srodata, erodata)。 */
    .rodata : ALIGN(4K) {
        srodata = .;
        *(.rodata .rodata.*)
        erodata = .;
    }

    /* 有初值的可写数据区域 [sdata, edata)。 */
    .data : ALIGN(4K) {
        sdata = .;
        *(.data .data.*)
        edata = .;
    }

    /* 启动/管理栈独立于普通 BSS 清零范围。 */
    . = ALIGN(4K);
    .boot_stack (NOLOAD) : {
        sboot_stack = .;
        KEEP(*(.bss.stack))
        eboot_stack = .;
    }

    /* 普通 BSS 会在进入 Rust 前由 `_start` 主动清零。 */
    . = ALIGN(4K);
    .bss (NOLOAD) : {
        sbss = .;

        /* 第 07 课第一次真正需要的两类静态运行栈和可信 kernel env。 */
        KEEP(*(.bss.user_stack))
        KEEP(*(.bss.user_trap_stack))
        KEEP(*(.bss.kernel_env))

        /* 第 04 课的清零探针也保持在普通 BSS。 */
        KEEP(*(.bss.probe))

        *(.bss .bss.*)
        *(COMMON)
        ebss = .;
    }

    /DISCARD/ : {
        *(.eh_frame)
    }
}
