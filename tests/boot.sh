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
