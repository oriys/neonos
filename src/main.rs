#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use core::ptr::write_volatile;

global_asm!(
    r#"
    .section .text.entry
    .globl _start
_start:
    la sp, boot_stack_top
    call rust_main
1:
    wfi
    j 1b

    .section .bss.stack, "aw", @nobits
    .align 12
boot_stack:
    .space 65536
boot_stack_top:
"#
);

const UART: *mut u8 = 0x1000_0000 as *mut u8;

#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> ! {
    for byte in b"Hello kernel\n" {
        unsafe { write_volatile(UART, *byte) };
    }
    halt()
}

fn halt() -> ! {
    loop {
        unsafe { asm!("wfi") };
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    halt()
}
