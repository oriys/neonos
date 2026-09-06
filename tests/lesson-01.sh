#!/bin/sh
# 第 01 课 checkpoint：验证“宿主机 cargo run → QEMU/OpenSBI → _start → rust_main → UART → halt”这条最小启动链。
#
# 这个脚本故意不验证后续课程能力。它只检查第 01 课真正承诺的事实：
# 1. 仓库仍能构建 RISC-V 裸机 ELF；
# 2. linker.ld 仍把 _start 作为入口；
# 3. _start 仍先设置 sp，再 call rust_main；
# 4. QEMU 日志中先出现 OpenSBI，再出现 Hello kernel；
# 5. Hello kernel 出现以后 QEMU 仍然存活，说明内核没有“打印完自动退出”。
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

fail() {
    echo "lesson-01 checkpoint failed: $1" >&2
    cat "$log" >&2 || true
    exit 1
}

cargo build
file "$kernel" | grep -Eq 'ELF 64-bit LSB executable.*RISC-V' || fail "kernel is not a 64-bit little-endian RISC-V ELF"

# 这些静态检查不是为了代替运行，而是为了把本课要求学生指出的源码证据也纳入 checkpoint。
grep -Fq 'ENTRY(_start)' linker.ld || fail "linker entry is not _start"

sp_line=$(grep -n -m 1 -F 'la sp, boot_stack_top' src/main.rs | cut -d: -f1 || true)
call_line=$(grep -n -m 1 -F 'call rust_main' src/main.rs | cut -d: -f1 || true)

[ -n "$sp_line" ] || fail "_start no longer initializes sp from boot_stack_top"
[ -n "$call_line" ] || fail "_start no longer calls rust_main"
[ "$sp_line" -lt "$call_line" ] || fail "rust_main is called before the boot stack is installed"

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
        break
    fi

    if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid" || true
        fail "QEMU exited before Hello kernel appeared"
    fi

    attempt=$((attempt + 1))
    sleep 0.1
done

grep -Fq 'Hello kernel' "$log" || fail "Hello kernel did not appear within the startup deadline"
grep -Fq 'OpenSBI' "$log" || fail "OpenSBI banner was not observed"

opensbi_line=$(grep -n -m 1 -F 'OpenSBI' "$log" | cut -d: -f1)
hello_line=$(grep -n -m 1 -F 'Hello kernel' "$log" | cut -d: -f1)
[ "$opensbi_line" -lt "$hello_line" ] || fail "Hello kernel appeared before the observed OpenSBI startup banner"

# 给 QEMU 一个短窗口；如果内核打印后错误返回/退出，这里应能抓到。
sleep 0.2
kill -0 "$pid" 2>/dev/null || fail "QEMU stopped after Hello kernel; lesson 01 expects the kernel to remain in halt()"

echo "lesson-01 checkpoint passed"
