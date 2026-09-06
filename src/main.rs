// `#![...]` 是作用于整个 Rust 文件（crate）的属性。
#![no_std]
#![no_main]

// 第 02 课：可复用格式化输出。
mod console;

// 第 04 课：读取 linker symbols，打印并验证真实内存地图。
mod memory;

// 第 05 课：S-mode trap 入口、TrapFrame 与异常诊断。
mod trap;

// 第 06 课：Program / Process / ProcessState 最小模型。
mod process;

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

const LESSON04_BSS_PROBE_ENABLED: usize = cfg!(feature = "lesson04-bss-probe") as usize;
const LESSON04_SKIP_BSS_CLEAR: usize = cfg!(feature = "lesson04-skip-bss-clear") as usize;

global_asm!(
    r#"
    .section .text.entry
    .globl _start
_start:
    la sp, boot_stack_top

    li t2, {probe_enabled}
    beqz t2, 2f
    la t0, bss_probe
    li t1, 0x1122334455667788
    sd t1, 0(t0)

2:
    li t2, {skip_bss_clear}
    bnez t2, 4f

    la t0, sbss
    la t1, ebss
3:
    bgeu t0, t1, 4f
    sb zero, 0(t0)
    addi t0, t0, 1
    j 3b

4:
    call rust_main

1:
    wfi
    j 1b

    .section .bss.stack, "aw", @nobits
    .align 12
boot_stack:
    .space 65536
boot_stack_top:

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

    // 第 04 课：真实链接布局与 BSS 启动契约。
    memory::report_and_validate();

    // 第 05 课：默认只安装 stvec；故障 feature 会在下一行之后直接进入 trap 并停住。
    trap::init();

    #[cfg(feature = "lesson05-illegal-trap")]
    trap::trigger_lesson05();

    // 第 06 课仍然只是 Rust 内核里的模型实验，因此明确输出 `[model]`。
    // 它不会执行用户指令，也不会使用 sret。
    process::run_lesson06_model();

    // 第 03 课的故障注入回归仍然保留。
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
