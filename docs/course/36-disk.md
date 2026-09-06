# 第 36 课：先准备一块绝对安全的实验磁盘

状态：待开始。前置：[第六阶段](stage-06.md) 验收完成。预计 45～60 分钟。

## 第一原则：本课程永远不对宿主机真实磁盘做实验

本阶段会实现“写磁盘”。所以在学驱动之前，先建立不可违反的安全边界：

> **neonos 只允许写一个新创建的普通 raw 镜像文件。绝不把 `/dev/...`、宿主机系统盘、已有用户文件当实验目标。**

阅读 OSTEP 第 36～38 章的设备/磁盘概念。本课不写 kernel driver。

---

## A. 创建专用 probe image

在项目专用的、被 `.gitignore` 排除的输出目录创建：

```text
16 MiB raw file
```

例如工具逻辑：

```text
if path already exists:
    refuse unless explicit test-only recreate flag
if path is not a regular file:
    refuse
create exactly 16 MiB
```

不要为了方便写：

```sh
dd ... of=$ANY_USER_PATH
```

而没有路径保护。

建议以后由：

```text
tools/mkdisk.py / test helper
```

统一创建，脚本检查：

- 目标位于项目 test-output 目录；
- 是 regular file；
- 不跟随危险 symlink；
- 默认不覆盖。

### 16 MiB 的几何

```text
16 MiB = 16 * 1024 * 1024 bytes
       = 16,777,216 bytes
```

VirtIO block request 的 `sector` 单位按 512-byte sector 计：

```text
16 MiB / 512 = 32,768 sectors
```

后面的 neonos 文件系统选择：

```text
fs block = 4096 bytes = 8 sectors
```

所以：

```text
16 MiB / 4096 = 4096 fs blocks
```

这三个单位从现在开始永远写清：

```text
byte
512-byte device sector
4096-byte filesystem block
```

不要都叫“block”。

---

## B. 写一个宿主机可核对的 probe

选择：

```text
sector 64
```

byte offset：

```text
64 * 512 = 32768
```

写固定 pattern，例如：

```text
neonos disk probe
```

并记录：

- 精确 offset；
- 精确 bytes；
- 长度；
- 一个简单 checksum/hex dump。

用独立读取方式从宿主机再次读取 offset 32768，确认 pattern 正确。

不要让“写 probe 的函数”自己调用同一个 decode 函数后宣布成功；至少用另一个读取/hex 工具核对。

### 和未来 fs block 对照

```text
fs block 8
→ byte offset 8*4096 = 32768
→ sectors 64..71
```

因此 probe sector 64 恰好位于 fs block 8 开头。

**probe image 和第 40 课 filesystem image 分开。** 第 40 课会重新创建/格式化另一个受保护 test image，不让早期 marker 混成文件系统数据。

---

## C. 给 QEMU 单独挂载这个文件

默认 `cargo run` 先保持现有“无磁盘/普通启动”入口不变。

另建 storage test 启动方式，显式：

```text
raw backend file
→ virtio-blk-device
```

启动参数要显式声明：

```text
format=raw
```

避免 QEMU 猜格式。

一个典型结构是：

```text
-drive file=<probe.img>,format=raw,if=none,id=vd0,...
-device virtio-blk-device,drive=vd0
```

实际 cache 参数在第 38 课持久化实验时再固定并记录。

同一个 image 同一时间只允许一个 QEMU writer。宿主机要修改/查看会与 guest 写入冲突的内容时，先关闭 QEMU。

---

## D. 不猜 VirtIO MMIO 地址，读设备树

QEMU `virt` 可以导出它生成的 DTB。保存当前实验配置对应的 DTB，并记录：

```text
UART node
virtio-mmio nodes
RAM
interrupt controller
```

重要：

```text
存在一个 virtio-mmio slot
≠ 那个 slot 当前一定挂着 block device
```

第 37 课必须读取每个候选设备的：

```text
MagicValue
Version
DeviceID
```

确认 `DeviceID` 真的是 block device 后再使用。

把本次 QEMU 命令和 DTB hash/关键信息写进学习记录，避免以后 QEMU 参数变了还用旧地址。

---

## 安全检查清单

开始任何 guest write 前必须能回答：

```text
这个 backend 是什么文件？
它是不是项目新建 regular file？
大小是不是预期 16 MiB？
QEMU 是否显式 format=raw？
是否只有这个 QEMU 实例在写？
```

如果有一个答案不确定，停止 storage 实验。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| sector 64 marker 出现在奇怪位置 | 是否把 sector 当 4 KiB block |
| fs block 8 算成 sector 8 | 一个 fs block 是 8 个 512-byte sectors |
| QEMU 警告 raw probing | 是否显式 `format=raw` |
| DTB 有多个 virtio slots | 不能靠 slot 顺序猜 block device，要读 DeviceID |
| 重新跑测试 probe 消失 | helper 是否默认每次重建 image；读写阶段要明确是否 recreate |
| 害怕误写宿主机盘 | 测试 helper 是否只接受受保护 regular-file 路径 |

## 验收

### 安全/几何

- [ ] 能从 16 MiB 算出 32768 sectors / 4096 fs blocks。
- [ ] sector 64 = byte 32768 = fs block 8 start。
- [ ] probe image 是新建 regular file，不是设备节点/用户现有文件。
- [ ] 默认不覆盖已有 image。

### QEMU/平台

- [ ] storage 启动入口显式 `format=raw`。
- [ ] 默认无盘 boot test 不被破坏。
- [ ] 当前实验 DTB 已导出并记录 virtio-mmio 候选信息。
- [ ] 能解释“MMIO slot”和“真正 block device”为什么不是同义词。

## 下一课为什么自然出现

现在宿主机文件里已经有一个精确 marker，QEMU 也挂上了 block backend。

下一步 kernel 要完成第一条真正设备事务：

```text
发现 virtio block
→ 协商 feature
→ 建 virtqueue
→ 提交 READ sector 64
→ 等待 used ring
→ 读出同一个 marker
```

进入 [第 37 课：第一次从设备读到数据](37-block-read.md)。