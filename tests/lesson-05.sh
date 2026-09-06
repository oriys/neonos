#!/bin/sh
# 第 05 课 checkpoint：验证 stvec、TrapFrame 和一次真实 S-mode 非法指令异常。
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
    echo "lesson-05 checkpoint failed: $1" >&2
    cat "$log" >&2 || true
    exit 1
}

start_qemu() {
    : >"$log"
    qemu-system-riscv64 \
        -machine virt \
        -bios default \
        -kernel "$kernel" \
        -nographic \
        -smp 1 >"$log" 2>&1 &
    pid=$!
}

wait_for() {
    marker=$1
    attempt=0
    while [ "$attempt" -lt 50 ]; do
        if grep -Fq "$marker" "$log"; then
            return 0
        fi
        if ! kill -0 "$pid" 2>/dev/null; then
            wait "$pid" || true
            return 1
        fi
        attempt=$((attempt + 1))
        sleep 0.1
    done
    return 1
}

stop_qemu() {
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    pid=
}

# 先检查教材强调的保存顺序：原始 t0 必须在 t0 被借作临时寄存器之前落入 frame。
save_t0_line=$(grep -n -m 1 -F 'sd t0, {x5}(sp)' src/trap.S | cut -d: -f1 || true)
reuse_t0_line=$(grep -n -m 1 -F 'addi t0, sp, {tf_size}' src/trap.S | cut -d: -f1 || true)
[ -n "$save_t0_line" ] || fail "trap entry does not save original t0"
[ -n "$reuse_t0_line" ] || fail "trap entry does not reconstruct original sp"
[ "$save_t0_line" -lt "$reuse_t0_line" ] || fail "t0 is clobbered before its original value is saved"

# B 次：默认构建只安装入口，不主动触发异常。
cargo build
start_qemu
wait_for 'trap ready' || fail "default boot never reached trap ready"
grep -Fq 'memory layout ok' "$log" || fail "lesson 04 regression failed before trap initialization"
if grep -Fq '[trap]' "$log"; then
    fail "default boot unexpectedly entered trap handler"
fi
kill -0 "$pid" 2>/dev/null || fail "default kernel stopped after installing stvec"
stop_qemu

# C 次：显式开启受控非法指令。
cargo build --features lesson05-illegal-trap
start_qemu
wait_for '[trap]' || fail "controlled illegal instruction did not reach the kernel trap handler"

# 去掉 QEMU 串口常见的 CR，便于做精确字段比较。
clean_log=$(mktemp)
tr -d '\r' <"$log" >"$clean_log"

trap_count=$(grep -Fc '[trap]' "$clean_log" || true)
[ "$trap_count" -eq 1 ] || {
    rm -f "$clean_log"
    fail "trap handler was entered more than once"
}

grep -Fq 'scause=0x2' "$clean_log" || {
    rm -f "$clean_log"
    fail "scause is not illegal-instruction exception code 2"
}
grep -Fq 'interrupt=false' "$clean_log" || {
    rm -f "$clean_log"
    fail "controlled trap was reported as an interrupt"
}
grep -Fq 'cause_code=2' "$clean_log" || {
    rm -f "$clean_log"
    fail "decoded exception code is not 2"
}
grep -Fq 'trigger_match=true' "$clean_log" || {
    rm -f "$clean_log"
    fail "saved sepc does not match the trigger label"
}

trigger=$(sed -n 's/^trigger at \(0x[0-9a-fA-F][0-9a-fA-F]*\)$/\1/p' "$clean_log" | head -n 1)
sepc=$(sed -n 's/^sepc=\(0x[0-9a-fA-F][0-9a-fA-F]*\)$/\1/p' "$clean_log" | head -n 1)

[ -n "$trigger" ] || {
    rm -f "$clean_log"
    fail "trigger address was not printed"
}
[ -n "$sepc" ] || {
    rm -f "$clean_log"
    fail "sepc was not printed"
}
[ "$trigger" = "$sepc" ] || {
    rm -f "$clean_log"
    fail "sepc differs from trigger label: trigger=$trigger sepc=$sepc"
}

grep -Eq '^original_sp=0x[0-9a-fA-F]+$' "$clean_log" || {
    rm -f "$clean_log"
    fail "original trap-time sp was not captured"
}

rm -f "$clean_log"

# 本课 handler 不恢复、不 sret；报告后应稳定停在内核中。
sleep 0.2
kill -0 "$pid" 2>/dev/null || fail "kernel exited instead of halting after the trap report"
stop_qemu

# 恢复默认构建，证明仓库不会长期处在主动触发异常的配置。
cargo build

echo "lesson-05 checkpoint passed"
