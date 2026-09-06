#!/bin/sh
# 第 10 课 checkpoint：用户 fault 终止当前 Process，内核继续运行另一个正常用户程序。
set -eu

kernel=target/riscv64gc-unknown-none-elf/debug/neonos
log=$(mktemp)
clean_log=$(mktemp)
pid=

cleanup() {
    if [ -n "$pid" ]; then
        kill "$pid" 2>/dev/null || true
    fi
    rm -f "$log" "$clean_log"
}
trap cleanup EXIT INT TERM

fail() {
    echo "lesson-10 checkpoint failed: $1" >&2
    cat "$log" >&2 || true
    exit 1
}

# 先验证实现边界：Faulted 是结构化终态，用户 API 不直接碰 UART MMIO。
grep -Fq 'struct FaultInfo' src/process.rs || fail "FaultInfo is missing"
grep -Fq 'Faulted(FaultInfo)' src/process.rs || fail "Faulted process state is missing"
grep -Fq 'fn finish_fault' src/process.rs || fail "Running -> Faulted commit API is missing"
grep -Fq 'user_api_putchar:' src/user_api.S || fail "user putchar wrapper is missing"
grep -Fq 'user_api_exit:' src/user_api.S || fail "user exit wrapper is missing"
# 注释里可以解释 UART；真正禁止的是把 QEMU virt UART MMIO 地址写进用户 wrapper。
if grep -Eiq '0x0*10000000|0x1000_0000' src/user_api.S; then
    fail "user API wrapper contains the UART MMIO address"
fi
grep -Fq 'call rust_user_trap_dispatch' src/trap.S || fail "user trap entry does not use shared dispatcher"

cargo build --features lesson10-user-errors

qemu-system-riscv64 \
    -machine virt \
    -bios default \
    -kernel "$kernel" \
    -nographic \
    -smp 1 >"$log" 2>&1 &
pid=$!

attempt=0
while [ "$attempt" -lt 160 ]; do
    tr -d '\r' <"$log" >"$clean_log"
    if grep -Fq '[stage 02] complete' "$clean_log"; then
        break
    fi

    if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid" || true
        fail "QEMU exited before stage 02 completion"
    fi

    attempt=$((attempt + 1))
    sleep 0.1
done

tr -d '\r' <"$log" >"$clean_log"
grep -Fq '[stage 02] complete' "$clean_log" || fail "final stage marker missing"

grep -Eq '^\[user fault\] pid=3 cause=2 sepc=0x[0-9a-fA-F]+ stval=0x[0-9a-fA-F]+$' "$clean_log" \
    || fail "controlled U-mode illegal instruction was not classified as cause 2"
grep -Fq '[user fault] process=Faulted kernel_alive=true' "$clean_log" \
    || fail "fault did not commit Faulted after returning to management"

grep -Fxq 'AFTER_FAULT_OK' "$clean_log" || fail "post-fault user program did not complete its output"
grep -Fq '[user fault] pid=4 process=Exited(0)' "$clean_log" \
    || fail "post-fault user process did not exit normally"

if grep -Fq 'BAD_EXIT_RETURN' "$clean_log"; then
    fail "valid exit incorrectly resumed user code"
fi
if grep -Fq '[panic]' "$clean_log"; then
    fail "kernel panicked while isolating a user fault"
fi

sleep 0.2
kill -0 "$pid" 2>/dev/null || fail "kernel did not remain alive after stage 02"

cargo build

echo "lesson-10 checkpoint passed"
