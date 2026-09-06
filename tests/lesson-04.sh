#!/bin/sh
# 第 04 课 checkpoint：验证 linker 内存地图、启动栈隔离和普通 BSS 主动清零。
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
    echo "lesson-04 checkpoint failed: $1" >&2
    cat "$log" >&2 || true
    exit 1
}

run_case() {
    features=$1
    expected_probe=$2
    label=$3

    cargo build --features "$features"
    : >"$log"

    qemu-system-riscv64 \
        -machine virt \
        -bios default \
        -kernel "$kernel" \
        -nographic \
        -smp 1 >"$log" 2>&1 &
    pid=$!

    attempt=0
    while [ "$attempt" -lt 50 ]; do
        if grep -Fq 'memory layout ok' "$log"; then
            break
        fi

        if ! kill -0 "$pid" 2>/dev/null; then
            wait "$pid" || true
            fail "$label: QEMU exited before memory layout validation completed"
        fi

        attempt=$((attempt + 1))
        sleep 0.1
    done

    grep -Fq '.text' "$log" || fail "$label: .text range missing"
    grep -Fq '.rodata' "$log" || fail "$label: .rodata range missing"
    grep -Fq '.data' "$log" || fail "$label: .data range missing"
    grep -Fq '.boot_stack' "$log" || fail "$label: .boot_stack range missing"
    grep -Fq '.bss' "$log" || fail "$label: .bss range missing"
    grep -Fq 'size=65536' "$log" || fail "$label: 64 KiB boot stack invariant missing"
    grep -Fq '_start' "$log" || fail "$label: _start address missing"
    grep -Fq 'sp' "$log" || fail "$label: current sp missing"
    grep -Fq "bss_probe   $expected_probe" "$log" || fail "$label: unexpected BSS probe value"
    grep -Fq 'memory layout ok' "$log" || fail "$label: runtime layout invariants failed"

    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    pid=
}

# 正例：先把 probe 写成明确非零，再执行正常 BSS 清零；Rust 必须读到 0。
run_case 'lesson04-bss-probe' '0x0000000000000000' 'clear-enabled'

# 负例：同样先写非零，但故意跳过清零；Rust 必须看到原模式。
run_case 'lesson04-bss-probe,lesson04-skip-bss-clear' '0x1122334455667788' 'clear-skipped'

# 最后确认默认 feature 集仍能构建；仓库不会长期停在故意跳过清零的状态。
cargo build

echo "lesson-04 checkpoint passed"
