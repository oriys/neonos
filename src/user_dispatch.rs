// 所有 U-mode trap 共用同一个汇编入口；这里仅按课程 checkpoint 选择 Rust 处理器。

use crate::trap::TrapFrame;

#[unsafe(no_mangle)]
pub extern "C" fn rust_user_trap_dispatch(frame: *mut TrapFrame) -> usize {
    let frame = unsafe { &mut *frame };

    #[cfg(feature = "lesson10-user-errors")]
    return crate::user10::handle_user_trap(frame);

    #[cfg(any(
        feature = "lesson07-user-mode",
        feature = "lesson08-syscalls",
        feature = "lesson09-exit"
    ))]
    return crate::user::rust_user_trap_handler(frame as *mut TrapFrame);

    panic!("unexpected user trap without an active user-mode checkpoint");
}
