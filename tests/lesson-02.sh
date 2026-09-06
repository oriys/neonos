#!/bin/sh
# 第 02 课 checkpoint：验证 console 分层和 Rust core::fmt 格式化输出。
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
    echo "lesson-02 checkpoint failed: $1" >&2
    cat "$log" >&2 || true
    exit 1
}

cargo build

# 主流程不应再直接拥有 UART MMIO 实现。
if grep -Fq 'use core::ptr::write_volatile' src/main.rs; then
    fail "main.rs still imports write_volatile directly"
fi
if grep -Fq 'const UART:' src/main.rs; then
    fail "main.rs still owns the UART address"
fi

# console 模块必须真正包含本课四个层次，而不是只改输出文本。
grep -Fq 'pub fn put_byte' src/console.rs || fail "console::put_byte is missing"
grep -Fq 'pub fn write_text' src/console.rs || fail "console::write_text is missing"
grep -Fq 'impl Write for Console' src/console.rs || fail "Console does not implement core::fmt::Write"
grep -Fq 'pub fn _print' src/console.rs || fail "console::_print is missing"

qemu-system-riscv64 \
    -machine virt \
    -bios default \
    -kernel "$kernel" \
    -nographic \
    -smp 1 >"$log" 2>&1 &
pid=$!

attempt=0
while [ "$attempt" -lt 50 ]; do
    if grep -Fq 'left right' "$log"; then
        break
    fi

    if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid" || true
        fail "QEMU exited before the console demonstration completed"
    fi

    attempt=$((attempt + 1))
    sleep 0.1
done

# 逐项验证格式化结果。
grep -Fq 'Hello kernel' "$log" || fail "Hello kernel is missing"
grep -Fq 'count=42' "$log" || fail "decimal formatting failed"
grep -Fq 'addr=0x80200000' "$log" || fail "hex address formatting failed"
grep -Fq 'left right' "$log" || fail "print!/println! line composition failed"

# 再验证这些输出的相对顺序，以及无参数 println!() 确实产生了空行。
awk '
{
    sub(/\r$/, "", $0)
}
/Hello kernel/ && state == 0 { state = 1; next }
/count=42/ && state == 1 { state = 2; next }
/addr=0x80200000/ && state == 2 { state = 3; next }
state == 3 && $0 == "" { state = 4; next }
/left right/ && state == 4 { state = 5; next }
END { exit state == 5 ? 0 : 1 }
' "$log" || fail "console output order or blank-line behavior is wrong"

kill -0 "$pid" 2>/dev/null || fail "kernel did not remain alive after console output"

echo "lesson-02 checkpoint passed"
