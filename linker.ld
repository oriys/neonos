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
        KEEP(*(.text.entry))
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

    /*
     * 启动栈单独成为一个 NOLOAD 输出 section。
     * 它仍占运行时内存，但不属于普通 BSS 的主动清零范围。
     */
    . = ALIGN(4K);
    .boot_stack (NOLOAD) : {
        sboot_stack = .;
        KEEP(*(.bss.stack))
        eboot_stack = .;
    }

    /*
     * 普通 BSS 使用独立的 [sbss, ebss) 区间。
     * `_start` 会在进入 Rust 之前按字节把这个范围清零。
     */
    . = ALIGN(4K);
    .bss (NOLOAD) : {
        sbss = .;
        *(.bss .bss.*)
        *(COMMON)
        ebss = .;
    }

    /DISCARD/ : {
        *(.eh_frame)
    }
}
