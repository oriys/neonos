// 第 04 课把“链接脚本里的地址”变成 Rust 可以观察和验证的内存地图。
//
// 这里不做分页，也不分配内存；只回答：当前 ELF 的主要区域实际在哪里。

use core::arch::asm;
use core::ptr::{addr_of, read_volatile};

// 这些名字不是普通 Rust 变量，而是 `linker.ld` 或启动汇编导出的符号。
// 对边界符号，我们真正关心的是“符号所在地址”，不是去读取那个地址里的 u8 值。
unsafe extern "C" {
    static stext: u8;
    static etext: u8;
    static srodata: u8;
    static erodata: u8;
    static sdata: u8;
    static edata: u8;
    static sboot_stack: u8;
    static eboot_stack: u8;
    static sbss: u8;
    static ebss: u8;

    // `_start` 是汇编入口标签；同样只取它的地址。
    static _start: u8;

    // `bss_probe` 与边界符号不同：它真的是普通 BSS 中预留的 8-byte 存储。
    static bss_probe: u64;
}

// 统一使用半开区间 [start, end)。
#[derive(Clone, Copy)]
struct Region {
    start: usize,
    end: usize,
}

impl Region {
    fn size(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    fn is_well_formed(self) -> bool {
        self.start <= self.end
    }

    fn contains(self, address: usize) -> bool {
        self.start <= address && address < self.end
    }
}

#[derive(Clone, Copy)]
struct Layout {
    text: Region,
    rodata: Region,
    data: Region,
    boot_stack: Region,
    bss: Region,
}

// 只取得符号地址，不解引用符号所在内存。
fn layout() -> Layout {
    // SAFETY: 这些 extern 符号都由当前 linker.ld 定义；这里只用 `addr_of!` 取得地址。
    unsafe {
        Layout {
            text: Region {
                start: addr_of!(stext) as usize,
                end: addr_of!(etext) as usize,
            },
            rodata: Region {
                start: addr_of!(srodata) as usize,
                end: addr_of!(erodata) as usize,
            },
            data: Region {
                start: addr_of!(sdata) as usize,
                end: addr_of!(edata) as usize,
            },
            boot_stack: Region {
                start: addr_of!(sboot_stack) as usize,
                end: addr_of!(eboot_stack) as usize,
            },
            bss: Region {
                start: addr_of!(sbss) as usize,
                end: addr_of!(ebss) as usize,
            },
        }
    }
}

fn start_address() -> usize {
    // SAFETY: `_start` 由启动汇编定义；这里只取地址。
    unsafe { addr_of!(_start) as usize }
}

fn current_sp() -> usize {
    let value: usize;

    // SAFETY: `mv dst, sp` 只读取当前栈指针，不访问内存，也不修改 sp。
    unsafe {
        asm!(
            "mv {value}, sp",
            value = out(reg) value,
            options(nomem, nostack)
        );
    }

    value
}

fn bss_probe_value() -> u64 {
    // SAFETY:
    // - `bss_probe` 由启动汇编在普通 BSS 中按 8-byte 对齐预留；
    // - 这里只做一次 volatile 读取，用来观察 Rust 运行前启动代码留下的真实结果；
    // - 不创建 `&mut` 或长期引用。
    unsafe { read_volatile(addr_of!(bss_probe)) }
}

fn print_region(name: &str, region: Region) {
    crate::println!(
        "{:<11} [{:#x}, {:#x}) size={}",
        name,
        region.start,
        region.end,
        region.size()
    );
}

fn overlaps(left: Region, right: Region) -> bool {
    left.start < right.end && right.start < left.end
}

// 打印真实地图，并把教材中的“应该如此”变成运行时不变量。
pub fn report_and_validate() {
    let layout = layout();
    let entry = start_address();
    let sp = current_sp();
    let probe = bss_probe_value();

    print_region(".text", layout.text);
    print_region(".rodata", layout.rodata);
    print_region(".data", layout.data);
    print_region(".boot_stack", layout.boot_stack);
    print_region(".bss", layout.bss);
    crate::println!("_start      {:#x}", entry);
    crate::println!("sp          {:#x}", sp);
    crate::println!("bss_probe   {:#018x}", probe);

    if !layout.text.is_well_formed()
        || !layout.rodata.is_well_formed()
        || !layout.data.is_well_formed()
        || !layout.boot_stack.is_well_formed()
        || !layout.bss.is_well_formed()
    {
        panic!("memory layout contains an inverted [start,end) range");
    }

    // 当前链接脚本按 text → rodata → data → boot_stack → bss 排列。
    // 中间允许因为 4 KiB 对齐产生空洞，但不允许逆序或重叠。
    if layout.text.end > layout.rodata.start
        || layout.rodata.end > layout.data.start
        || layout.data.end > layout.boot_stack.start
        || layout.boot_stack.end > layout.bss.start
    {
        panic!("kernel memory regions overlap or are out of order");
    }

    if layout.boot_stack.size() != 65_536 {
        panic!(
            "boot stack size mismatch: expected 65536, got {}",
            layout.boot_stack.size()
        );
    }

    if overlaps(layout.boot_stack, layout.bss) {
        panic!("boot stack overlaps ordinary BSS");
    }

    if !layout.text.contains(entry) {
        panic!("_start is outside the .text range");
    }

    // `sp` 是栈中“当前正在使用的位置”，允许等于栈顶边界，也允许因 Rust 调用帧下降。
    if sp < layout.boot_stack.start || sp > layout.boot_stack.end {
        panic!("current sp is outside the boot stack range");
    }

    // 正常启动以及“先写 probe 再清零”的正例都必须看到 0。
    // 只有 lesson04-skip-bss-clear 负例显式关闭这条启动契约。
    if !cfg!(feature = "lesson04-skip-bss-clear") && probe != 0 {
        panic!("ordinary BSS was not zeroed before Rust: probe={:#x}", probe);
    }

    crate::println!("memory layout ok");
}
