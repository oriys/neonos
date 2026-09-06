use core::arch::asm;
// Quantum is deliberately expressed in platform ticks, not milliseconds.
// QEMU's actual timebase-frequency is recorded by tests/scheduling.sh from its DTB.
pub const T: usize = 10_000;
pub fn now() -> usize {
    let value;
    unsafe {
        asm!("rdtime {}", out(reg) value, options(nomem, nostack));
    }
    value
}
pub fn after(interval: usize) -> usize {
    assert!(interval > 0);
    now()
        .checked_add(interval)
        .expect("timer deadline overflow")
}
pub fn arm(deadline: usize) {
    crate::sbi::set_timer(deadline);
    unsafe {
        asm!("csrs sie, {}", in(reg) 32usize, options(nomem, nostack));
    }
}
pub fn stop() {
    unsafe {
        asm!("csrc sie, {}", in(reg) 32usize, options(nomem, nostack));
    }
    crate::sbi::set_timer(usize::MAX);
}
