// 这个模块只负责“把内核想输出的内容变成 UART 字节”。
//
// 第 02 课故意把打印路径拆成四层：
//
// print!/println!
//      ↓
// core::fmt::Arguments
//      ↓
// _print / Console::write_str
//      ↓
// write_text
//      ↓
// put_byte
//      ↓
// QEMU virt UART
//
// 这样以后 panic、trap、调度器等模块都不需要知道 UART 地址。

// `fmt` 是 Rust `core` 里的格式化模块。
// `Write` trait 规定了“一个对象只要会接收字符串片段，就可以接入格式化系统”。
use core::fmt::{self, Write};

// `write_volatile` 用来访问具有硬件副作用的 MMIO 地址。
// 它保证这次写入不会被编译器当作普通、可删除的内存写。
use core::ptr::write_volatile;

// 当前课程固定运行在 QEMU `virt` 机器上。
// 这个实验环境的主 UART MMIO base 是 0x1000_0000。
//
// 注意：这是“当前实验平台的设备地址”，不是所有 RISC-V 机器的通用规则。
const UART_BASE: usize = 0x1000_0000;

// 发送一个原始字节。
//
// 这是整个 console 模块里最底层、也是唯一真正接触 MMIO 的函数。
pub fn put_byte(byte: u8) {
    // 把整数地址转换成一个“可写 u8 的裸指针”。
    let uart = UART_BASE as *mut u8;

    // SAFETY:
    // - `UART_BASE` 来自当前 QEMU `virt` 教学平台的 UART MMIO 布局；
    // - 本函数只写 1 byte 到该设备寄存器；
    // - 这不是普通 RAM，所以必须使用 volatile 访问保留设备副作用。
    //
    // 当前阶段还没有实现 16550 的状态轮询、并发保护或完整驱动初始化；
    // 这里只保留第 01 课已经验证过的最小早期输出路径。
    unsafe { write_volatile(uart, byte) };
}

// 输出一段 Rust 字符串。
//
// `&str` 表示“借用一段有效 UTF-8 字符串”，函数不会取得它的所有权。
pub fn write_text(text: &str) {
    // UART 只理解字节，不理解 Rust 的“字符”概念。
    // `as_bytes()` 把 UTF-8 字符串视为最终编码后的字节切片。
    for &byte in text.as_bytes() {
        put_byte(byte);
    }
}

// `Console` 本身不需要保存任何字段。
// 它只是一个“我能够接收格式化文本”的类型标记。
pub struct Console;

// 让 `Console` 实现 `core::fmt::Write`。
//
// 从此以后，Rust 格式化系统只需要知道“Console 会 write_str”，
// 不需要知道 UART、MMIO 地址或 volatile 是什么。
impl Write for Console {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        write_text(text);

        // 当前最小 UART 后端没有可报告给 `fmt` 的 recoverable formatting error，
        // 所以写完以后返回 Ok(())。
        Ok(())
    }
}

// `_print` 是自定义宏和 `core::fmt` 之间的桥。
//
// `format_args!` 不会先分配一个 `String`；它构造的是一个轻量的
// `fmt::Arguments` 描述，然后由 `write_fmt` 把最终文本片段逐步交给 Console。
pub fn _print(args: fmt::Arguments<'_>) {
    let mut console = Console;

    // `Console::write_str` 当前总是返回 Ok，因此这里不 `unwrap()`。
    // 这样以后 panic 路径使用 console 时，不会因为打印错误又触发第二次 panic。
    let _ = console.write_fmt(args);
}

// `print!` 只负责把调用者提供的格式模板和参数打包成 `fmt::Arguments`，
// 真正输出仍由 `$crate::console::_print` 完成。
//
// `$crate` 表示“定义这个宏的 crate 根”，因此宏在别的模块展开时
// 不依赖调用位置的相对路径。
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        $crate::console::_print(format_args!($($arg)*));
    }};
}

// `println!` 在 `print!` 的基础上追加一个换行字节。
//
// 单独提供空参数分支，确保 `println!()` 也能工作。
#[macro_export]
macro_rules! println {
    () => {{
        $crate::print!("\n");
    }};
    ($($arg:tt)*) => {{
        $crate::console::_print(format_args!("{}\n", format_args!($($arg)*)));
    }};
}
