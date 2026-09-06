// 第 08 课：第一个真正“处理完再返回用户”的系统调用分发层。
//
// ABI 约定：
//   a7 = syscall number
//   a0 = 第一个参数；返回时也写回 a0
//
// 本课只实现：
//   1 = putchar(byte 0..127) -> 0
//
// 错误：
//   -1 = unknown syscall
//   -2 = invalid argument

pub const SYSCALL_PUTCHAR: usize = 1;
pub const ERR_UNKNOWN_SYSCALL: isize = -1;
pub const ERR_INVALID_ARGUMENT: isize = -2;

pub fn dispatch(number: usize, arg0: usize) -> isize {
    match number {
        SYSCALL_PUTCHAR => {
            // 第一版只接受 7-bit ASCII，避免现在就引入 UTF-8/终端编码层。
            if arg0 > 0x7f {
                ERR_INVALID_ARGUMENT
            } else {
                crate::console::put_byte(arg0 as u8);
                0
            }
        }
        _ => ERR_UNKNOWN_SYSCALL,
    }
}
