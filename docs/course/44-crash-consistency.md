# 第 44 课：为什么“写成功了”仍可能留下坏文件系统

状态：待开始。前置：[第七阶段](stage-07.md) 总验收完成。分 A、B 两次，每次 45～60 分钟。

## 目标与阅读

阅读 OSTEP 第 42 章中 crash consistency 的问题部分。目标不是马上修复，而是先把一次文件系统更新拆成多个持久写入，并稳定重现“只完成了一半”的状态。

本课故意测试 **Stage 7 的无日志写入路径**；第 46～47 课才按照 [第八阶段统一协议](stage-08-protocol.md) 加入 full-block redo journal。不要把本课的坏写入顺序直接当成最终日志协议。

本课只使用专用测试镜像副本。QEMU 被终止代表课程中的受控崩溃点，不等同于真实硬件任意断电模型。

---

## A 次：先写出不变量

以“把空文件扩展到一个数据块”为最小案例。一次成功操作至少涉及：

```text
数据块内容
块位图
inode 的 direct block pointer
inode.size
```

在没有 journal 的第七阶段实现中，这些变化最终都要写回磁盘，但它们不可能在同一个不可分割的设备动作里同时完成。

先写出完成状态必须满足的不变量：

1. inode 引用的数据块必须在合法 data region。
2. 被 inode 引用的数据块必须在 block bitmap 中标记 allocated。
3. 一个普通 data block 不能同时被两个互不共享的 inode 当作私有块。
4. inode.size 所覆盖的非 sparse file logical blocks 必须有对应 direct pointer。
5. 未被任何 inode/directory/system-reserved region 使用的 block 不应永久标为 allocated。

再讨论两种危险顺序：

```text
先持久化 bitmap，再持久化 inode
→ 中途 crash 可能产生“allocated 但无人引用”的 leak block

先持久化 inode，再持久化 bitmap
→ 中途 crash 可能产生“inode 引用但 bitmap 仍 free”的 block
```

不要把“换个写入顺序”误认为已经解决全部一致性问题；多块更新通常仍存在某个坏中间状态。

### 数据内容也属于一致性讨论

即使 inode/bitmap 最终结构上自洽，如果 inode 已经 durable 指向一个“新分配但内容尚未 durable”的 data block，也可能得到结构 clean、内容却不是用户期望值的状态。

因此从这一课开始区分：

```text
structural consistency
semantic file content
```

第 45 课 fsck 主要检查前者；第 47 课 recovery test 还会单独检查 old/new exact content oracle。

---

## B 次：做确定性的故障注入

1. 给无日志 file-extension path 增加仅测试使用的故障点编号，不通过 random sleep 猜时机。
2. 测试脚本从同一份已知正确 baseline image 复制出新的工作副本。
3. 在 bitmap 已 writeback **且 FLUSH 成功**、inode 尚未 writeback 之前终止 QEMU，重启时不要重新 mkfs。
4. 用宿主机 offline decoder/fsck 查看 bitmap 与 inode，记录真实矛盾。
5. 再做相反顺序的独立实验：让 inode update **FLUSH durable**、bitmap 尚未 durable，观察另一类矛盾。
6. 每次实验后丢弃工作副本，从 baseline 重新复制，避免前一个坏镜像影响下一 case。

这里的关键是：

> **只有显式 FLUSH 成功以后，教材才把那一组写入当作可证明的 durable checkpoint。**

如果只在 WRITE request completed 后、FLUSH 前 kill QEMU：

```text
不能可靠断言“这次 write 一定已持久”
也不能可靠断言“一定没持久”
```

这种点可以做观察实验，但只能重启后读取实际磁盘 bytes 再解释，不能作为确定性 old/new 教材答案。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 每次损坏结果不同 | 是否在没有 durable evidence 的 write/flush 中间点 kill |
| 重启后 filesystem 又正常了 | test script 是否偷偷重新 mkfs/覆盖 working image |
| 看见内容错就都叫 metadata corruption | data/inode/bitmap 分别检查实际状态 |
| 所有写入顺序看起来都安全 | 是否只测一个 disk block，没有覆盖 multi-block update |
| fsck clean 就说 operation 正确 | structural clean 不等于事务 semantic old/new 正确 |

## 验收与复盘

- [ ] 能解释为什么多个 persistent block update 不能靠普通 memory critical section 获得 crash atomicity。
- [ ] 写出至少 5 条本课程 filesystem structural invariants。
- [ ] 稳定复现“bitmap allocated 但 inode 不引用”或“inode 引用但 bitmap free”至少一种矛盾。
- [ ] 故障实验从同一 baseline 的独立副本开始，并使用明确 FLUSH-completed checkpoint。
- [ ] 能区分 runtime failure rollback、structural consistency、semantic file content 和 crash recovery。
- [ ] 知道本课测试的是 Stage 7 无 journal path，不是第 46 课最终 transaction protocol。

下一课：[第 45 课：让检查器指出磁盘哪里不一致](45-fsck.md)。