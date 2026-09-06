#!/bin/sh
# 第 03 课 checkpoint：用 feature 触发一次可控 Rust panic，验证 panic handler 的真实诊断路径。
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
    echo "lesson-03 checkpoint failed: $1" >&2
    cat "$log" >&2 || true
    exit 1
}

# 只在这次构建打开故障注入；默认内核仍保持正常启动。
cargo build --features lesson03-panic

qemu-system-riscv64 \
    -machine virt \
    -bios default \
    -kernel "$kernel" \
    -nographic \
    -smp 1 >"$log" 2>&1 &
pid=$!

attempt=0
while [ "$attempt" -lt 50 ]; do
    if grep -Fq '[panic] lesson 03 deliberate failure' "$log"; then
        break
    fi

    if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid" || true
        fail "QEMU exited before panic diagnostics appeared"
    fi

    attempt=$((attempt + 1))
    sleep 0.1
done

grep -Fq 'Hello kernel' "$log" || fail "deliberate panic did not occur after normal startup output"
grep -Fq '[panic] lesson 03 deliberate failure' "$log" || fail "panic message is missing"
grep -Eq 'at src/main\.rs:[0-9]+:[0-9]+' "$log" || fail "panic source location is missing or malformed"

if grep -Fq 'SHOULD_NOT_REACH' "$log"; then
    fail "control returned after panic"
fi

# panic handler 最后进入 halt，所以模拟器应该仍然存活，而不是像普通进程那样退出。
sleep 0.2
kill -0 "$pid" 2>/dev/null || fail "kernel exited instead of halting after panic"

echo "lesson-03 checkpoint passed"
