// `#![...]` 是作用于整个 Rust 文件（crate）的属性。
#![no_std]
#![no_main]

mod console;
mod memory;
mod trap;
mod process;

// 第 07 课第一次加入真实 U-mode 代码、用户栈和用户来源 trap 入口。
mod user;

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
    crate::println!("Hello kernel");
    crate::println!("count={}", 42);
    crate::println!("addr={:#x}", 0x8020_0000usize);
    crate::println!();
    crate::print!("left");
    crate::println!(" right");

    memory::report_and_validate();
    trap::init();

    // 第 05 课受控非法指令会直接进入内核 trap 并停止，不污染后续课次。
    #[cfg(feature = "lesson05-illegal-trap")]
    trap::trigger_lesson05();

    // 第 06 课模型仍先执行，并返回下一个未使用 PID。
    let _next_pid = process::run_lesson06_model();

    // 第 03 课 panic 回归必须仍能单独运行，所以它优先于第 07 课的“不返回”实验路径。
    #[cfg(feature = "lesson03-panic")]
    {
        lesson03_deliberate_panic();
        crate::println!("SHOULD_NOT_REACH");
    }

    // 默认构建不主动切换特权级；第 07 课 checkpoint 显式开启 feature。
    // 一旦成功 sret，用户 ecall 会进入可信 trap 栈并由 handler 停住，因此这里不会返回。
    #[cfg(feature = "lesson07-user-mode")]
    user::run_lesson07(_next_pid);

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
