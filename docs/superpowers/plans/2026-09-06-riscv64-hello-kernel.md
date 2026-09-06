# RISC-V Hello Kernel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a RISC-V 64-bit ELF with Cargo, boot it through OpenSBI in QEMU, set the kernel stack in `_start`, enter Rust, and print `Hello kernel`.

**Architecture:** A single `no_std`, `no_main` Rust binary embeds its assembly entry point. Cargo selects the bare-metal RISC-V target and linker script; QEMU's `virt` machine and bundled OpenSBI load and enter the ELF.

**Tech Stack:** Rust nightly, `riscv64gc-unknown-none-elf`, LLVM linker, QEMU `virt`, OpenSBI, POSIX shell

**Spec:** `docs/superpowers/specs/2026-09-06-riscv64-hello-kernel-design.md`

## Global Constraints

- The ELF load address is `0x80200000` and its entry symbol is `_start`.
- The first version runs exactly one hart.
- Guest output goes directly to the QEMU `virt` UART at `0x10000000`.
- No bootloader crate, allocator, build script, or third-party Rust dependency is added.
- Multi-hart coordination, memory management, interrupts, additional drivers, and shutdown support remain out of scope.

---

### Task 1: Bootable kernel and end-to-end smoke test

**Files:**
- Create: `tests/boot.sh`
- Create: `.gitignore`
- Create: `Cargo.toml`
- Generated: `Cargo.lock`
- Create: `.cargo/config.toml`
- Create: `linker.ld`
- Create: `src/main.rs`

**Interfaces:**
- Consumes: QEMU `virt` memory map, bundled OpenSBI, and the built-in `riscv64gc-unknown-none-elf` Rust target.
- Produces: `target/riscv64gc-unknown-none-elf/debug/neonos`, with ELF entry symbol `_start`; `pub extern "C" fn rust_main() -> !`; `cargo run` as the QEMU launch command.

- [x] **Step 1: Install missing host prerequisites**

Run:

```bash
rustup target add riscv64gc-unknown-none-elf
brew install qemu
```

Expected: `rustup target list --installed` contains `riscv64gc-unknown-none-elf`, and `qemu-system-riscv64 --version` exits successfully.

- [x] **Step 2: Write the failing end-to-end test**

Create `tests/boot.sh`:

```sh
#!/bin/sh
set -eu

kernel=target/riscv64gc-unknown-none-elf/debug/neonos
log=$(mktemp)
pid=

cleanup() {
    if [ -n "$pid" ]; then
        kill "$pid" 2>/dev/null || true
    fi
    rm -f "$log"
}
trap cleanup EXIT INT TERM

cargo build
file "$kernel" | grep -Eq 'ELF 64-bit LSB executable.*RISC-V'

qemu-system-riscv64 \
    -machine virt \
    -bios default \
    -kernel "$kernel" \
    -nographic \
    -smp 1 >"$log" 2>&1 &
pid=$!

attempt=0
while [ "$attempt" -lt 50 ]; do
    if grep -Fq 'Hello kernel' "$log"; then
        exit 0
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid" || true
        cat "$log"
        exit 1
    fi
    attempt=$((attempt + 1))
    sleep 0.1
done

cat "$log"
exit 1
```

Then run:

```bash
chmod +x tests/boot.sh
./tests/boot.sh
```

Expected: FAIL because `Cargo.toml` does not exist. This catches a missing or broken build/boot/output chain; changing the entry, stack initialization, Rust call, UART address, or message prevents the expected guest output.

- [x] **Step 3: Add the minimal Cargo project configuration**

Create `.gitignore`:

```gitignore
/target
```

Create `Cargo.toml`:

```toml
[package]
name = "neonos"
version = "0.1.0"
edition = "2024"

[profile.dev]
panic = "abort"

[profile.release]
panic = "abort"
```

Create `.cargo/config.toml`:

```toml
[build]
target = "riscv64gc-unknown-none-elf"

[target.riscv64gc-unknown-none-elf]
rustflags = ["-C", "link-arg=-Tlinker.ld"]
runner = [
    "qemu-system-riscv64",
    "-machine", "virt",
    "-bios", "default",
    "-nographic",
    "-smp", "1",
    "-kernel",
]
```

- [x] **Step 4: Add the linker layout**

Create `linker.ld`:

```ld
OUTPUT_ARCH(riscv)
ENTRY(_start)

SECTIONS
{
    . = 0x80200000;

    .text : ALIGN(4K) {
        KEEP(*(.text.entry))
        *(.text .text.*)
    }

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4K) {
        *(.data .data.*)
    }

    .bss : ALIGN(4K) {
        *(.bss.stack)
        *(.bss .bss.*)
        *(COMMON)
    }

    /DISCARD/ : {
        *(.eh_frame)
    }
}
```

- [x] **Step 5: Add `_start`, the kernel stack, and Rust UART output**

Create `src/main.rs`:

```rust
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
```

- [x] **Step 6: Verify the test turns green**

Run:

```bash
./tests/boot.sh
```

Expected: exit status 0 after the RISC-V ELF boots and emits `Hello kernel`.

Run the user-facing commands separately:

```bash
cargo build
file target/riscv64gc-unknown-none-elf/debug/neonos
cargo run
```

Expected: the build succeeds; `file` identifies a 64-bit little-endian RISC-V ELF; OpenSBI reports the next address as `0x80200000`; the guest prints `Hello kernel`. Exit QEMU with `Ctrl-A X`.

- [x] **Step 7: Commit**

```bash
git add Cargo.toml .cargo/config.toml linker.ld src/main.rs tests/boot.sh
git commit -m "feat: boot riscv64 hello kernel"
```
