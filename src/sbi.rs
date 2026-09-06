//! SBI v0.2 BASE/TIME. a0/a1 are outputs; all other registers are preserved by SBI.
use core::arch::asm;
fn call(eid: usize, fid: usize, arg: usize) -> Result<usize, isize> {
    let error: isize;
    let value: usize;
    unsafe {
        asm!("ecall", in("a7") eid, in("a6") fid,
             inlateout("a0") arg => error, lateout("a1") value,
             options(nostack));
    }
    if error == 0 { Ok(value) } else { Err(error) }
}
pub fn init_time() {
    assert!(
        call(0x10, 3, 0x54494d45).expect("SBI BASE probe failed") != 0,
        "TIME extension unavailable"
    );
}
pub fn set_timer(deadline: usize) {
    call(0x54494d45, 0, deadline).expect("SBI TIME failed");
}
