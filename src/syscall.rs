// 第 08～09 课的用户 syscall 分发层。
//
// ABI：a7=number，a0=arg0/result。
// 第 09 课第一次出现“处理成功但不返回用户”的 syscall，因此分发结果不能再只是一个整数。

pub const SYSCALL_PUTCHAR: usize = 1;
pub const SYSCALL_EXIT: usize = 2;

pub const ERR_UNKNOWN_SYSCALL: isize = -1;
pub const ERR_INVALID_ARGUMENT: isize = -2;

#[derive(Clone, Copy)]
pub enum SyscallOutcome {
    // 普通 syscall：把结果写回 a0，推进 sepc，然后恢复用户。
    Return(isize),

    // 有效 exit：记录 code 后终止本次用户运行；不为了返回用户去推进 sepc。
    Exit(u8),
}

pub fn dispatch(number: usize, arg0: usize) -> SyscallOutcome {
    match number {
        SYSCALL_PUTCHAR => {
            if arg0 > 0x7f {
                SyscallOutcome::Return(ERR_INVALID_ARGUMENT)
            } else {
                crate::console::put_byte(arg0 as u8);
                SyscallOutcome::Return(0)
            }
        }
        SYSCALL_EXIT => {
            if arg0 > u8::MAX as usize {
                SyscallOutcome::Return(ERR_INVALID_ARGUMENT)
            } else {
                SyscallOutcome::Exit(arg0 as u8)
            }
        }
        _ => SyscallOutcome::Return(ERR_UNKNOWN_SYSCALL),
    }
}
