/* `OUTPUT_ARCH(riscv)` 告诉链接器：最终生成的是 RISC-V 架构的程序。 */
OUTPUT_ARCH(riscv)

/* `ENTRY(_start)` 指定 ELF 程序入口。
 * QEMU/OpenSBI 加载这个内核后，会从 `_start` 对应的地址开始执行。
 */
ENTRY(_start)

/* `SECTIONS` 用来描述最终 ELF 中各个 section（段）应该怎样排列。 */
SECTIONS
{
    /* `.` 是链接器里的“当前位置计数器”。
     * 把它设成 `0x80200000`，表示后面的内核内容从这个地址开始布局。
     * 在当前 QEMU + OpenSBI 启动方式里，内核就是从这里加载和执行的。
     */
    . = 0x80200000;

    /* 定义最终 ELF 的 `.text` 段，也就是主要存放机器指令的区域。
     * `ALIGN(4K)` 表示让它从 4 KiB 对齐的地址开始。
     */
    .text : ALIGN(4K) {
        /* `*(.text.entry)` 表示收集所有输入目标文件里的 `.text.entry` 段。
         * `KEEP(...)` 表示即使链接器认为它“好像没被引用”，也绝不能删除。
         * `_start` 就放在这里，所以它必须保留。
         */
        KEEP(*(.text.entry))

        /* 收集普通 `.text` 和所有 `.text.xxx` 形式的代码段。
         * Rust 编译出来的大部分函数机器码最终都会进入这里。
         */
        *(.text .text.*)
    }

    /* `.rodata` = read-only data，只读数据。
     * 比如字符串常量、只读常量等通常会放在这里。
     */
    .rodata : ALIGN(4K) {
        /* 收集 `.rodata` 和所有 `.rodata.xxx` 段。 */
        *(.rodata .rodata.*)
    }

    /* `.data` 用来存放“已经有初始值、并且运行时允许修改”的全局/静态数据。 */
    .data : ALIGN(4K) {
        /* 收集 `.data` 和所有 `.data.xxx` 段。 */
        *(.data .data.*)
    }

    /* `.bss` 用来存放“运行时需要内存，但 ELF 文件里通常不用保存实际字节”的数据。
     * 常见例子是未初始化的全局变量，以及我们手工预留的启动栈。
     */
    .bss : ALIGN(4K) {
        /* 先把启动汇编中定义的 `.bss.stack` 放进来。 */
        *(.bss.stack)

        /* 再收集普通 `.bss` 和所有 `.bss.xxx` 段。 */
        *(.bss .bss.*)

        /* `COMMON` 是一些工具链可能产生的传统“未初始化全局变量”区域。
         * 把它也并入 `.bss`，可以避免遗漏这类数据。
         */
        *(COMMON)
    }

    /* `/DISCARD/` 是一个特殊区域：放进这里的 section 会被最终 ELF 丢弃。 */
    /DISCARD/ : {
        /* `.eh_frame` 通常用于异常展开、栈回溯等运行时信息。
         * 当前这个极简内核使用 `panic = "abort"`，也不做这类异常展开，
         * 所以把它丢掉，保持内核镜像更简单。
         */
        *(.eh_frame)
    }
}
