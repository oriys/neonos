# Lab 1 提示

先尝试解释失败日志，再看下一层提示。

<details><summary>第一层：应该看哪些已有代码？</summary>

Console 已实现 fmt::Write，宏已经构造 fmt::Arguments；你的任务是接通中间层。BSS 循环的 t0/t1 分别是当前地址和结束地址。trap_entry_address() 提供入口地址。
</details>

<details><summary>第二层：容易漏掉的契约</summary>

格式化输出使用 Write 的 write_fmt，不需要 String。panic 的 message 与 location 是不同信息。stvec 的低两位选择模式，Direct 要求入口低两位为零。普通 BSS 不包含启动栈。
</details>

<details><summary>第三层：失败如何定位？</summary>

没有任何输出时先修 console，不要先调 trap。probe 负例也变零时，检查是否忽略 skip 配置。trap 不出现时检查 sie、SIE、stvec 的写入与读回；出现但 sepc 不匹配时，读 TrapFrame 保存路径而不是伪造输出。
</details>
