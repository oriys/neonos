#!/bin/sh
# 第 07 课 checkpoint：真实 sret 进入 U-mode，再由用户 ecall 回到可信 S-mode trap 栈。
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
    echo "lesson-07 checkpoint failed: $1" >&2
    cat "$log" >&2 || true
    exit 1
}

# 先检查最关键的安全边界没有被实现成“继续用 user sp”。
grep -Fq 'csrrw sp, sscratch, sp' src/trap.S \
    || fail "user trap entry does not swap away the user-controlled sp first"
grep -Fq 'csrw stvec, t0' src/trap.S \
    || fail "user trap entry never restores the kernel fault vector"

cargo build --features lesson07-user-mode

qemu-system-riscv64 \
    -machine virt \
    -bios default \
    -kernel "$kernel" \
    -nographic \
    -smp 1 >"$log" 2>&1 &
pid=$!

attempt=0
while [ "$attempt" -lt 80 ]; do
    if grep -Fq 'user_mode_evidence=true' "$log"; then
        break
    fi

    if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid" || true
        fail "QEMU exited before U-mode evidence was reported"
    fi

    attempt=$((attempt + 1))
    sleep 0.1
done

grep -Fq '[user setup] pid=3 program=user-ecall' "$log" \
    || fail "real user Process did not follow the two lesson-06 model PIDs"
grep -Fq 'size=16384' "$log" || fail "16 KiB user/trap stack evidence missing"
grep -Fq '[user setup] spp=0' "$log" || fail "kernel did not construct SPP=0 before sret"
grep -Fq '[user setup] process=Running' "$log" || fail "real Process was not Running before sret"
grep -Fq '[user enter]' "$log" || fail "S-mode never reached the user-entry boundary"

grep -Fq '[user trap]' "$log" || fail "user ecall did not return to the user trap handler"
grep -Fq 'origin_spp=0' "$log" || fail "saved sstatus does not prove the trap originated in U-mode"
grep -Fq 'scause=0x8' "$log" || fail "trap was not a U-mode ecall exception"
grep -Fq 'cause_code=8' "$log" || fail "decoded exception code is not U-mode ecall=8"
grep -Fq 'a0=85' "$log" || fail "saved a0 does not contain the user-written ASCII U value"
grep -Fq 'a7=1' "$log" || fail "saved a7 does not contain the user-written syscall number"
grep -Fq 'frame_in_trap_stack=true' "$log" || fail "TrapFrame is not inside the trusted trap stack"
grep -Fq 'saved_user_sp_ok=true' "$log" || fail "saved x2 is not a valid aligned user-stack address"
grep -Fq 'sepc_in_user_code=true' "$log" || fail "saved sepc is outside the linked user code"
grep -Fq 'ecall_match=true' "$log" || fail "saved sepc is not the exact user ecall label"
grep -Fq 'kernel_stvec_restored=true' "$log" || fail "S-mode handler did not restore the kernel fault vector"
grep -Fq 'user_mode_evidence=true' "$log" || fail "combined U-mode evidence failed"

clean_log=$(mktemp)
tr -d '\r' <"$log" >"$clean_log"
sepc=$(sed -n 's/^sepc=\(0x[0-9a-fA-F][0-9a-fA-F]*\)$/\1/p' "$clean_log" | head -n 1)
ecall=$(sed -n 's/^expected_ecall=\(0x[0-9a-fA-F][0-9a-fA-F]*\)$/\1/p' "$clean_log" | head -n 1)
rm -f "$clean_log"

[ -n "$sepc" ] || fail "user-trap sepc was not printed"
[ -n "$ecall" ] || fail "expected user ecall address was not printed"
[ "$sepc" = "$ecall" ] || fail "sepc differs from user ecall label: sepc=$sepc ecall=$ecall"

if grep -Fq '[panic]' "$log"; then
    fail "lesson 07 reached panic instead of the intended user-trap path"
fi

# 本课明确不返回用户；handler 应稳定停住。
sleep 0.2
kill -0 "$pid" 2>/dev/null || fail "kernel exited after the user trap report"

# 恢复默认构建，确保早期课次仍可单独运行。
cargo build

echo "lesson-07 checkpoint passed"
