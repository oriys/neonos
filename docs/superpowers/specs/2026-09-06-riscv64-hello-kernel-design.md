# RISC-V Hello Kernel Design

## Goal

`cargo build` produces a RISC-V 64-bit ELF which QEMU boots through its bundled OpenSBI firmware. OpenSBI transfers control to `_start`; `_start` installs a kernel stack and calls Rust; Rust writes `Hello kernel` to the QEMU `virt` machine UART.

## Architecture

The project is one `no_std`, `no_main` Rust binary for the built-in `riscv64gc-unknown-none-elf` target. Cargo target configuration supplies the linker script and a QEMU runner, so `cargo build` builds the ELF and `cargo run` boots that exact artifact.

QEMU uses `-machine virt -bios default -kernel <elf> -nographic -smp 1`. Its bundled OpenSBI firmware loads the kernel at `0x80200000` and jumps to the ELF entry point.

## Components

- `Cargo.toml`: defines the single kernel binary and aborting panic profiles.
- `.cargo/config.toml`: selects the RISC-V target, linker script, and QEMU runner.
- `linker.ld`: places the image at `0x80200000`, declares `ENTRY(_start)`, and lays out code, read-only data, data, BSS, and the stack.
- `src/main.rs`: embeds the small `_start` assembly routine, owns a fixed kernel stack, exposes `rust_main`, writes bytes to the `virt` UART at `0x10000000`, and then waits forever.
- `tests/boot.sh`: builds the ELF, checks its architecture, boots it, and fails unless `Hello kernel` appears before a short timeout.

No bootloader crate, allocator, build script, or third-party Rust dependency is needed.

## Boot Flow

1. Cargo compiles and links the kernel as a RISC-V ELF.
2. QEMU starts its default OpenSBI firmware and supplies the ELF as the kernel.
3. OpenSBI enters `_start` at supervisor privilege.
4. `_start` loads the linker-provided stack-top address into `sp`.
5. `_start` calls the C-ABI `rust_main` function.
6. `rust_main` writes `Hello kernel\n` to the memory-mapped UART and enters a `wfi` loop.

The first version intentionally runs one hart only. Multi-hart coordination, memory management, interrupts, drivers beyond UART, and shutdown support are outside this goal.

## Failure Handling

The panic handler enters the same non-returning wait loop. Build and boot failures are surfaced by Cargo, QEMU, or the smoke test; the kernel has no recoverable runtime operations yet.

## Verification

The persistent smoke test is written and observed failing before implementation. After implementation it must prove all of the following in one run:

- `cargo build` exits successfully.
- the output is a 64-bit little-endian RISC-V ELF;
- QEMU starts it through default OpenSBI;
- the guest emits `Hello kernel`.

Final verification reruns the smoke test from the committed project state.
