// `#![...]` 是作用于整个 Rust 文件（crate）的属性。
// `no_std` 表示这个程序不使用 Rust 标准库 `std`。
// 普通程序可以依赖操作系统提供文件、线程、网络等能力；内核启动时没有这些东西，
// 所以这里只使用更底层、几乎不依赖操作系统的 `core` 库。
#![no_std]

// `no_main` 告诉 Rust：不要生成普通应用程序的 `main` 启动流程。
// 这个项目不是被 macOS/Linux 启动的普通程序，而是一个内核，
// 所以我们会在下面自己提供真正的入口 `_start`。
#![no_main]

// 从 `core::arch` 导入两个和汇编有关的工具：
// - `asm!`：在 Rust 函数中插入少量汇编指令；
// - `global_asm!`：在整个程序中加入一段全局汇编代码。
use core::arch::{asm, global_asm};

// `PanicInfo` 保存 Rust 发生 panic 时的信息。
// 因为我们没有标准库，所以必须自己实现 panic 之后该怎么办。
use core::panic::PanicInfo;

// `write_volatile` 用来执行“不能被编译器随便优化掉”的内存写入。
// 后面我们会通过内存地址直接操作 UART 硬件，这类写入具有硬件副作用，
// 因此不能把它当成普通内存写入。
use core::ptr::write_volatile;

// `global_asm!` 把下面这整段 RISC-V 汇编直接加入最终内核。
// `r#" ... "#` 是 Rust 的 raw string（原始字符串），这样里面的双引号不需要转义。
global_asm!(
    r#"
    # `.section` 表示切换到一个 ELF section（段）。
    # `.text.entry` 是我们专门给最早启动代码取的名字。
    .section .text.entry

    # `.globl` 把 `_start` 声明成全局符号，链接器才能把它作为程序入口找到。
    .globl _start

# `_start:` 是一个汇编标签，代表“内核从这里开始执行”。
# `linker.ld` 中的 `ENTRY(_start)` 会把 ELF 的入口设置到这里。
_start:
    # `la` = load address，把 `boot_stack_top` 这个地址装进 `sp` 寄存器。
    # `sp` = stack pointer（栈指针）。Rust 函数正常运行前必须先有可用的栈。
    la sp, boot_stack_top

    # `call` 调用下面用 Rust 写的 `rust_main` 函数。
    # 到这里以后，我们就从最小的汇编启动代码进入 Rust 世界了。
    call rust_main

# `1:` 是 RISC-V/GNU 汇编里的“数字局部标签”。
# 它只用于附近的跳转，不需要起一个全局名字。
1:
    # `wfi` = Wait For Interrupt，意思是“等待中断”。
    # 当前内核没有别的任务可做，所以让 CPU 等待，而不是一直做无意义计算。
    wfi

    # `j` = jump（跳转）。`1b` 中的 `b` 表示 backward，
    # 即跳回前面最近的 `1:` 标签，于是形成永久循环。
    j 1b

    # 接下来不再定义机器指令，而是给启动栈预留内存。
    # `.bss.stack`：给启动栈单独取的 section 名字；
    # `a` = alloc，运行时需要为它安排内存；
    # `w` = writable，这块内存允许写；
    # `@nobits` = ELF 文件本身不用真的保存这几万字节，只需要记录运行时要留空间。
    .section .bss.stack, "aw", @nobits

    # `.align 12` 表示按 2^12 = 4096 字节，也就是 4 KiB 边界对齐。
    # 4 KiB 也是操作系统里非常常见的页大小。
    .align 12

# `boot_stack` 是这块启动栈低地址端的标签。
boot_stack:
    # `.space 65536` 从当前位置开始预留 65536 字节。
    # 65536 字节 = 64 KiB，这就是当前内核的启动栈大小。
    .space 65536

# `boot_stack_top` 是预留完 64 KiB 后的位置，也就是启动栈的高地址端。
# RISC-V 的栈通常从高地址向低地址增长，所以最开始让 `sp` 指向这里。
boot_stack_top:
"#
);

// QEMU 的 `virt` 虚拟机器把 UART（串口设备）映射在物理地址 `0x1000_0000`。
// `*mut u8` 表示“指向一个可写 8 位整数的裸指针”。
// 这里不是普通内存：往这个地址写一个字节，实际上就是给串口发送一个字符。
const UART: *mut u8 = 0x1000_0000 as *mut u8;

// Rust 2024 edition 要求把 `no_mangle` 这种可能影响链接安全性的属性写成 `unsafe(...)`。
// `no_mangle` 的作用是禁止 Rust 修改函数符号名，确保汇编中的 `call rust_main`
// 真正能找到一个就叫 `rust_main` 的符号。
#[unsafe(no_mangle)]
// `pub`：这个函数符号需要能被外部（这里是汇编代码）看到。
// `extern "C"`：使用稳定、明确的 C ABI 调用约定，方便汇编调用。
// `-> !`：`!` 叫 never type，表示这个函数永远不会正常返回。
pub extern "C" fn rust_main() -> ! {
    // `b"..."` 创建的是“字节字符串”，其中每个字符最终都是一个 u8 字节。
    // `for` 会把 `Hello kernel\n` 中的字节一个一个取出来。
    for byte in b"Hello kernel\n" {
        // `byte` 在这里是 `&u8`（对字节的引用），所以用 `*byte` 取出真正的 u8 值。
        // 写裸指针可能破坏内存安全，因此 Rust 要求放进 `unsafe` 块。
        // `write_volatile` 保证这次写入真的发生；写到 UART 后，QEMU 终端就会出现字符。
        unsafe { write_volatile(UART, *byte) };
    }

    // 所有字符打印完后进入 `halt()`，当前内核没有任何其他工作，所以不再返回。
    halt()
}

// `halt` 是一个永不返回的辅助函数。
// 以后无论“正常没事做”还是 panic，都可以复用这里的等待逻辑。
fn halt() -> ! {
    // `loop` 是 Rust 的无限循环。
    loop {
        // 在循环中执行 RISC-V 的 `wfi` 指令，让 CPU 等待中断。
        // 内联汇编属于底层操作，因此需要 `unsafe`。
        unsafe { asm!("wfi") };
    }
}

// `#[panic_handler]` 告诉 Rust：如果程序发生 panic，就调用下面这个函数。
// `no_std` 环境没有标准库替我们打印错误或退出进程，所以内核必须自己定义处理方式。
#[panic_handler]
// `_info` 中包含 panic 位置等信息。
// 参数名前加 `_` 表示“我现在故意没有使用这个参数”，避免编译器给出未使用警告。
// 返回 `!`，因为发生 panic 后这个函数也永远不会正常返回。
fn panic(_info: &PanicInfo) -> ! {
    // 当前最小实现不打印 panic 信息，只让 CPU 停在等待循环中。
    halt()
}
