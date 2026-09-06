// `#![...]` 是作用于整个 Rust 文件（crate）的属性。
#![no_std]
#![no_main]

// 第 02 课：可复用格式化输出。
mod console;

// 第 04 课：读取 linker symbols，打印并验证真实内存地图。
mod memory;

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

// 这两个编译期常量只服务于第 04 课的自动实验：
// - 默认都是 0，不改变正常启动语义；
// - probe 模式会在清零前故意污染 8-byte BSS；
// - skip 模式会故意跳过清零，构造负例。
const LESSON04_BSS_PROBE_ENABLED: usize = cfg!(feature = "lesson04-bss-probe") as usize;
const LESSON04_SKIP_BSS_CLEAR: usize = cfg!(feature = "lesson04-skip-bss-clear") as usize;

// 最早期启动代码必须在调用普通 Rust 函数前完成两个最低运行条件：
// 1. 准备一个不会被后续 BSS 清零覆盖的启动栈；
// 2. 主动把普通 [sbss, ebss) 清零。
global_asm!(
    r#"
    .section .text.entry
    .globl _start
_start:
    # 先建立启动栈。linker.ld 已把它从普通 BSS 独立成 .boot_stack。
    la sp, boot_stack_top

    # 第 04 课正/负对照探针。
    # 默认 probe_enabled=0，因此正常构建不会故意写脏 BSS。
    li t2, {probe_enabled}
    beqz t2, 2f
    la t0, bss_probe
    li t1, 0x1122334455667788
    sd t1, 0(t0)

2:
    # 负例 feature 仅用于证明“如果不清零，probe 的非零值会保留下来”。
    li t2, {skip_bss_clear}
    bnez t2, 4f

    # 按字节清普通 BSS 半开区间 [sbss, ebss)。
    # 循环只使用临时寄存器，不依赖 Rust 调用栈。
    la t0, sbss
    la t1, ebss
3:
    bgeu t0, t1, 4f
    sb zero, 0(t0)
    addi t0, t0, 1
    j 3b

4:
    # 到这里以后，Rust 可以依赖普通 BSS 已满足零初始化契约。
    call rust_main

1:
    wfi
    j 1b

    # 启动栈输入 section；linker.ld 会把它单独收进 .boot_stack (NOLOAD)。
    .section .bss.stack, "aw", @nobits
    .align 12
boot_stack:
    .space 65536
boot_stack_top:

    # 第 04 课 BSS 探针是真正的 8-byte 普通 BSS。
    # linker.ld 的 .bss wildcard 会收集它，因此它必须落在 [sbss, ebss) 内。
    .section .bss.probe, "aw", @nobits
    .balign 8
    .globl bss_probe
bss_probe:
    .space 8
"#,
    probe_enabled = const LESSON04_BSS_PROBE_ENABLED,
    skip_bss_clear = const LESSON04_SKIP_BSS_CLEAR,
);

#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> ! {
    // 第 02 课输出回归。
    crate::println!("Hello kernel");
    crate::println!("count={}", 42);
    crate::println!("addr={:#x}", 0x8020_0000usize);
    crate::println!();
    crate::print!("left");
    crate::println!(" right");

    // 第 04 课：用真实 linker symbols 和当前 sp 验证内存布局。
    memory::report_and_validate();

    // 第 03 课故障注入仍然保留，保证新增启动初始化没有破坏 panic 路径。
    #[cfg(feature = "lesson03-panic")]
    {
        lesson03_deliberate_panic();
        crate::println!("SHOULD_NOT_REACH");
    }

    halt()
}

#[cfg(feature = "lesson03-panic")]
fn lesson03_deliberate_panic() {
    panic!("lesson 03 deliberate failure");
}

fn halt() -> ! {
    loop {
        unsafe { asm!("wfi") };
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    crate::println!("[panic] {}", info.message());

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

    halt()
}
