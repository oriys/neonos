# 第 46 课：先记日志，再修改真正的文件系统

状态：待开始。前置：[第 45 课](45-fsck.md)。分 A、B 两次，每次 45～60 分钟。

## 目标与阅读

继续阅读 OSTEP 第 42 章 journaling。实现一个固定容量的教学 redo log，让一次事务的“是否提交”由持久化的 commit 记录决定，而不是由内存里的函数返回值决定。

## A 次：定义日志格式

沿用第七阶段预留的文件系统块 5～20。创建 `docs/fs-log-format.md`，明确所有字段宽度、端序、校验方式和容量限制。

第一版日志可以采用：

```text
block 5   journal header / commit metadata
block 6..20   transaction payload blocks
```

header 至少记录：

- 日志魔数与版本。
- 事务序号。
- payload 数量。
- 每个 payload 对应的 home block 号。
- 状态或 commit 标记。
- 能发现撕裂/截断元数据的校验信息。

payload 保存将来要写到 home location 的完整 4 KiB 块新内容。一次事务修改块数超过日志容量时，必须在写任何持久日志前返回“事务过大”，不能默默拆成多个事务后仍声称整体原子。

宿主机增加只读日志解码器：能够区分 empty、uncommitted、committed、malformed 四种状态。

## B 次：实现提交协议

提交过程固定为：

```text
1. 在内存中准备本事务最终块内容
2. 写 payload blocks
3. 写未提交 header / descriptor
4. flush
5. 写 commit 状态
6. flush
7. 把 payload 安装到各自 home block
8. flush
9. 清空日志头
10. flush
```

实现时要写清具体采用“descriptor 与 commit 同一 header 的两次写入”还是单独 commit 记录；无论哪种，都必须保证恢复代码能够仅凭磁盘内容判断事务有没有完成 commit 持久化。

### 为什么要两次 flush

如果 commit 在日志内容真正持久之前就可见，恢复可能看到“已提交”却缺少完整 payload；因此日志内容先 flush，再使 commit 持久。

commit 持久后，即使 home blocks 只安装了一部分，恢复也应能够从完整日志重新覆盖所有 home blocks。

## 与文件数据的关系

本课程第一版采用有限的 ordered metadata journaling：新分配给文件的数据块先写入其最终位置并 flush，然后把位图、inode、目录等元数据的新块镜像放进日志事务。这样已提交的元数据不会指向尚未保证持久的新数据块。

这不是通用的数据日志：覆盖已有文件内容、多个用户操作合并、并发事务、rename 原子性等都不自动得到保证。每个加入日志保护的操作必须列出事务涉及哪些块和数据先行条件。

## 失败处理

- payload 写入失败：不写 commit，保留错误并进入只读/停止写入状态。
- commit flush 失败：事务状态未知，不继续假装成功安装 home blocks。
- home 安装失败：保留已提交日志，下一次启动交给恢复重放。
- 清日志失败：即使 home 已正确，保留 committed 日志也可以在下次启动安全地再次重放；因此恢复必须幂等。

## 验收

- [ ] 日志格式有独立文档和宿主机解码器。
- [ ] 超容量事务在任何持久修改前失败。
- [ ] commit 前崩溃留下的日志不会被解释为已提交事务。
- [ ] commit 后、home 安装前中止时，磁盘上仍有足够信息恢复所有目标块。
- [ ] 每次成功事务最后清空日志；失败路径不把未知状态标成成功。
- [ ] 能解释为什么“写日志”和“写 commit”之间需要持久化顺序。

下一课：[第 47 课：启动时恢复，并验证每个崩溃边界](47-recovery.md)。
