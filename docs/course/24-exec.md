# 第 24 课：进程不变，换一个程序运行

状态：待开始。前置：[第四阶段](stage-04.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## `exec` 最容易被误解成“创建新进程”

本课先建立最重要的语义：

```text
exec 前：PID 7 正在运行 program A
exec 后：PID 7 仍是同一个 Process，但地址空间和用户程序变成 B
```

所以 `exec`：

- **不创建新 PID**；
- 不改变父子关系；
- 成功后不会回到旧程序的 `ecall` 后面；
- 失败时旧程序必须原样继续。

回看 OSTEP 第 5 章。当前只加载内核内置的受控 program catalog，不实现路径查找、通用 ELF、argv/envp。

---

## A 次：先构建一个“候选映像”，完全不动当前进程

### program catalog 必须由内核拥有

在计划中的：

```text
src/program.rs
```

建立固定目录：

```text
program_id
name
blob_start/blob_end
entry_offset
code/data/bss 布局
```

用户只能传：

```text
program_id
```

不能传一个任意 kernel pointer 让内核把那片内存当程序描述。

未知 ID：

```text
-6
```

### `build_image` 的输出必须是完整候选

概念接口：

```text
build_image(program_id) -> Result<CandidateImage, ExecError>
```

`CandidateImage` 至少拥有：

```text
new AddressSpace
new user TrapFrame/context
new entry
new user stack state
program identity
```

它还**没有**成为当前 Process 的正式映像。

### 构建顺序

1. 查内核 catalog；
2. checked 计算 code/data/bss/stack 地址和长度；
3. 验证区域不重叠、不越过课程 user layout；
4. 验证 entry offset 落在 executable code 内；
5. 分配新的 root/table/user frames；
6. 建 kernel borrowed mappings；
7. 建 user RX/RW mappings；
8. 把可重定位 blob 复制到新 code frame；
9. 初始化 data/bss/stack；
10. `fence.i`；
11. 构造全新的用户寄存器现场；
12. software walk 重新核对权限/入口/stack；
13. 全部成功才返回 `CandidateImage`。

### 新用户现场不能继承旧寄存器垃圾

成功 exec 后用户应该像“刚开始这个程序”：

```text
sepc/entry = new entry
sp = new stack top
通用寄存器 = 课程定义的干净初值
```

不要复制旧程序的：

```text
旧 sepc
旧 a0/a7
旧 stack contents
旧 user data
```

除非课程以后明确新增某种 exec 参数 ABI。

### 失败原子性

无论：

- unknown program；
- frame 不足；
- page-table frame 不足；
- layout/entry 非法；
- metadata 容量不足；

都必须：

```text
释放 Candidate 本次拥有的所有 frame
当前 Process.address_space 不变
当前 user TrapFrame 不变
当前 PID/state 不变
```

然后返回错误：

```text
unknown program → -6
resource shortage → -4
invalid image/layout → 使用阶段明确错误码
```

用户旧程序应能在失败分支继续输出：

```text
exec failed; still running
```

### A 次验收

- [ ] candidate 能在不修改 current process 的情况下完整构建。
- [ ] entry/layout/权限都经过 software walk 验证。
- [ ] 失败后 free-frame count 回到调用前。
- [ ] 旧程序现场和地址空间完全没变。

---

## B 次：成功 exec 的“提交点”必须清楚

### 为什么不能边建新映像边拆旧映像

错误流程：

```text
先 free 旧 user pages
→ 再尝试建新 root
→ 中途 OutOfFrames
→ 旧程序也没了
```

正确模型：

```text
OLD image（一直可运行）
+
完整 CANDIDATE
  ↓ commit
NEW image
  ↓
旧 image 才允许销毁
```

这和数据库/文件系统后面的“两阶段提交思维”很像：先准备，再提交。

### 成功 exec 不走普通 `ResumeUser`

当前正在 user `exec` syscall 的 trap 路径上。

如果成功后仍按第 08 课：

```text
sepc += 4
restore old TrapFrame
sret
```

就会回旧程序，这是错误的。

Rust trap/syscall 层应返回明确动作，例如：

```text
ExecReady(candidate)
```

然后正常返回汇编/管理路径，在可信 kernel stack 上完成提交。

### 提交时保留什么

同一个 Process 继续存在，所以保留：

```text
PID / generation
parent/children relation
Process slot identity
scheduler priority/usage（本课程选择保持）
以后第 27 课加入的 fd table
```

替换：

```text
program identity
AddressSpace
user context
user stack contents
```

本阶段还没有 fd；但现在先写下未来规则：

> exec 默认保留已打开 fd，供 shell 的 `fork → dup2 → exec` 工作；暂不实现 close-on-exec。

### 提交前先离开旧 user root 的所有权危险区

在安全 kernel management/trap stack 上：

1. candidate 已完全构建；
2. 切到永久 kernel root，或切到一个明确不会被本次销毁的安全 root；
3. `sfence.vma`；
4. 当前 hart 不再使用 old user root；
5. 把 Process 的 active image 原子替换成 candidate；
6. 销毁 old AddressSpace 的 Owned frames；
7. dispatch 同一个 PID 的新 user context；
8. 切 new root + `sfence.vma`；
9. `sret` 到 new entry。

也可以设计成先切 candidate root 再销毁 old，只要能严格证明当前执行的所有 kernel VA 在 candidate root 中存在且 old root 不再被硬件使用。课程建议通过 permanent kernel root 过渡，生命周期更容易推理。

### old trap stack 不属于旧 user image

第 21 课的可信 trap stack/调度资源属于 Process/线程的 kernel 运行资源，并通过 kernel mapping 存在，不要把它和 user AddressSpace 一起释放。

否则 exec 正在 trap stack 上执行时会自毁脚下的栈。

### 成功路径验证

A 程序：

```text
打印 A-before
exec(B)
打印 A-after-EXEC   ← 必须永远不可达
```

B 程序入口：

```text
打印 B-entry
验证 user sp 在新 stack
验证旧 data marker 不存在
exit
```

预期：

```text
A-before
B-entry
```

PID 日志前后相同。

### 反复 exec 资源压力

设计 A↔B 受控替换多轮（测试管理器可以限制次数，避免无限 ping-pong）：

```text
A exec B
B exec A
...
```

观察：

- 每次 PID 不变；
- old image 都被销毁；
- frame count 在允许的瞬时 candidate 双份占用后回到稳定基线；
- 不会每 exec 永久少几页。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| exec 成功后出现 A-after | 是否恢复了旧 TrapFrame/sepc |
| 失败后旧程序崩了 | 是否 candidate 完成前修改/释放 old image |
| exec 时 kernel page fault | 是否释放了当前 trap stack/kernel mapping，或 safe root 切换错误 |
| PID 改了 | 是否错误把 exec 实现成 create process |
| 多轮 exec frame 持续减少 | old AddressSpace destroy / candidate rollback 是否完整 |
| B 入口看到 A 的用户数据 | 新 user context/stack/data 是否真的重新初始化 |

## 最终验收

### 失败路径

- [ ] unknown ID / resource failure 保留 old image。
- [ ] 用户收到明确错误并能继续旧代码。
- [ ] candidate 所有 Owned frames 完整回滚。

### 成功路径

- [ ] PID/父关系不变。
- [ ] old `ecall` 后路径不可达。
- [ ] 新程序从自己的 entry/stack/context 开始。
- [ ] old AddressSpace 在安全 root 上销毁。
- [ ] Process kernel trap/scheduler 资源没有被误释放。
- [ ] 重复 exec 无长期 frame 泄漏。

### 理解验收

不看正文回答：

1. exec 为什么不是新建进程？
2. 为什么必须先完整 build candidate，再 commit？
3. 成功 exec 为什么不能 `sepc += 4` 后恢复旧现场？
4. PID、AddressSpace、user context 哪些保留，哪些替换？
5. 为什么销毁 old root 前要先切到安全 root？
6. 为什么 trap stack 不能随 old user image 一起 free？

## 下一课为什么自然出现

现在一个 Process 能换程序，但还没有“父进程等待另一个 Process 完成”的生命周期关系。

下一课先不急着实现 `fork`，先用测试夹具建立 parent/child 和 Zombie，学会：

> **子进程结束以后，为什么不能立刻把所有身份信息都删除？**

进入 [第 25 课：孩子结束了，父进程怎样知道](25-wait.md)。把 exec 成功/失败/资源曲线写进 [进度记录](progress.md)。