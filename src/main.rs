// `#![...]` 是作用于整个 Rust 文件（crate）的属性。
// `no_std` 表示这个程序不使用 Rust 标准库 `std`。
// 普通程序可以依赖操作系统提供文件、线程、网络等能力；内核启动时没有这些东西，
// 所以这里只使用更底层、几乎不依赖操作系统的 `core` 库。
#![no_std]

// `no_main` 告诉 Rust：不要生成普通应用程序的 `main` 启动流程。
// 这个项目不是被 macOS/Linux 启动的普通程序，而是一个内核，
// 所以我们会在下面自己提供真正的入口 `_start`。
#![no_main]

// `mod console;` 告诉 Rust：把 `src/console.rs` 作为当前 crate 的 `console` 模块编译进来。
// 从第 02 课开始，UART 地址、volatile 写入和格式化输出都收口在这个模块里。
mod console;

// 从 `core::arch` 导入两个和汇编有关的工具：
// - `asm!`：在 Rust 函数中插入少量汇编指令；
// - `global_asm!`：在整个程序中加入一段全局汇编代码。
use core::arch::{asm, global_asm};

// `PanicInfo` 保存 Rust 发生 panic 时的信息。
// 第 03 课会真正读取其中的消息和源码位置。
use core::panic::PanicInfo;

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

// Rust 2024 edition 要求把 `no_mangle` 这种可能影响链接安全性的属性写成 `unsafe(...)`。
// `no_mangle` 的作用是禁止 Rust 修改函数符号名，确保汇编中的 `call rust_main`
// 真正能找到一个就叫 `rust_main` 的符号。
#[unsafe(no_mangle)]
// `pub`：这个函数符号需要能被外部（这里是汇编代码）看到。
// `extern "C"`：使用稳定、明确的 C ABI 调用约定，方便汇编调用。
// `-> !`：`!` 叫 never type，表示这个函数永远不会正常返回。
pub extern "C" fn rust_main() -> ! {
    // 主流程只表达“输出什么”，不再接触 UART/MMIO 细节。
    crate::println!("Hello kernel");
    crate::println!("count={}", 42);
    crate::println!("addr={:#x}", 0x8020_0000usize);
    crate::println!();
    crate::print!("left");
    crate::println!(" right");

    // 默认构建不会进入这个分支。
    // 第 03 课的自动测试会显式开启 `lesson03-panic` feature，
    // 从而复现一次可控的软件 panic，而不需要测试脚本临时修改源码。
    #[cfg(feature = "lesson03-panic")]
    {
        lesson03_deliberate_panic();

        // `lesson03_deliberate_panic()` 在运行时一定 panic，因此这行绝不能出现。
        // 保留它是为了让测试能证明 panic 没有错误地返回到原控制流。
        crate::println!("SHOULD_NOT_REACH");
    }

    // 正常构建或 panic 测试完成诊断以后，内核都不会返回宿主机普通 main。
    halt()
}

// 这个辅助函数只在第 03 课故障注入构建中存在。
// 故意不把返回类型写成 `!`，这样编译器不会仅凭函数签名替测试证明后续代码不可达；
// “SHOULD_NOT_REACH 没出现”仍由真实运行行为证明。
#[cfg(feature = "lesson03-panic")]
fn lesson03_deliberate_panic() {
    panic!("lesson 03 deliberate failure");
}

// `halt` 是一个永不返回的辅助函数。
fn halt() -> ! {
    loop {
        // 在循环中执行 RISC-V 的 `wfi` 指令，让 CPU 等待中断。
        unsafe { asm!("wfi") };
    }
}

// `#[panic_handler]` 告诉 Rust：如果程序发生 panic，就调用下面这个函数。
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // 先输出固定前缀和 Rust 提供的 panic message。
    // 这里不使用 unwrap/expect，避免“报告错误的代码”再次触发 panic。
    crate::println!("[panic] {}", info.message());

    // `location()` 返回 Option：编译器通常能提供源码位置，但 API 不保证永远有。
    if let Some(location) = info.location() {
        crate::println!(
            "at {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    } else {
        crate::println!("location unavailable");
    }

    // panic 是不可恢复路径：留下诊断后稳定停住，不回 rust_main 继续执行。
    halt()
}
