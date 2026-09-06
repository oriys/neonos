/* 最终生成的是 RISC-V ELF。 */
OUTPUT_ARCH(riscv)

ENTRY(_start)

SECTIONS
{
    . = 0x80200000;

    .text : ALIGN(4K) {
        stext = .;
        KEEP(*(.text.entry))
        KEEP(*(.text.trap))
        KEEP(*(.text.user_enter))
        KEEP(*(.text.user))
        KEEP(*(.text.user10))
        KEEP(*(.text.user_scheduler))
        KEEP(*(.text.user_api))
        KEEP(*(.text.model_program))
        *(.text .text.*)
        etext = .;
    }

    .rodata : ALIGN(4K) {
        srodata = .;
        *(.rodata .rodata.*)
        erodata = .;
    }

    .data : ALIGN(4K) {
        sdata = .;
        *(.data .data.*)
        edata = .;
    }

    . = ALIGN(4K);
    .boot_stack (NOLOAD) : {
        sboot_stack = .;
        KEEP(*(.bss.stack))
        eboot_stack = .;
    }

    . = ALIGN(4K);
    .bss (NOLOAD) : {
        sbss = .;

        KEEP(*(.bss.user_stack))
        KEEP(*(.bss.user_trap_stack))
        KEEP(*(.bss.kernel_env))

        /* 第 09 课：run_user 跨整次用户运行保存的可信内核调用现场。 */
        KEEP(*(.bss.kernel_context))

        KEEP(*(.bss.probe))

        *(.bss .bss.*)
        *(COMMON)
        ebss = .;
    }

    /DISCARD/ : {
        *(.eh_frame)
    }
}
