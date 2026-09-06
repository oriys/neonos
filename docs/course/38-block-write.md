# 第 38 课：写完了，真的保存了吗

状态：待开始。前置：[第 37 课](37-block-read.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 这课最重要的三个层次

不要把下面三句话混成一句“写盘成功”：

```text
1. WRITE request completed
2. 同一次 QEMU 运行里重新 READ 得到新数据
3. FLUSH 完成并关闭/重启同一镜像后仍能 READ 得到新数据
```

它们证明的事情不同。

本课继续使用第 36 课的专用 probe image，不碰 filesystem image。

---

# A 次：WRITE request + 同次 READ-back

## 1. 先增加 WRITE，不改 queue 生命周期模型

仍然：

```text
modern virtio-mmio
split queue
one in-flight request
polling completion
```

WRITE 复用第 37 课同一套 request ownership、index wrapping、timeout/reset 规则。

### request header

概念上：

```text
type     = VIRTIO_BLK_T_OUT
reserved = 0
sector   = target sector
```

`OUT` 是从设备视角：

```text
guest buffer → device
```

### descriptor 方向

```text
D0 header: device reads, WRITE flag=0
D1 data  : device reads, WRITE flag=0
D2 status: device writes, WRITE flag=1
```

和 READ 最大区别就是 D1 的方向。

如果把 WRITE data descriptor 错标成 device-writable，设备看到的请求语义就不对。

## 2. 选择不会覆盖课程 probe 的独立测试区域

不要把 sector 64 的原始 `neonos disk probe` 直接覆盖掉。

例如另外选：

```text
sector 80
sector 81
```

先由宿主机记录它们的初始 bytes，再让 guest 写入两个不同 pattern：

```text
sector 80 → 0x11/known pattern A
sector 81 → 0x22/known pattern B
```

这样可以抓：

- sector number 与 byte offset 混淆；
- 总写同一 sector；
- request buffer 复用错误。

## 3. data buffer 在 completion 前必须保持稳定

WRITE 提交后：

```text
header/data/status
```

在 used completion 之前仍可能被 device 读取/写入。

所以：

- 不修改 WRITE data buffer；
- 不释放 frame；
- 不把同一 scratch 给另一 request；
- timeout 后仍遵守第 37 课 reset 规则。

## 4. 先验证 device completion

used ring 到达后检查：

```text
used.elem.id
status == OK
```

只有这一步成功，才能说：

> **设备报告这条 WRITE request 已完成。**

还不能说：

> “断电后一定存在”。

## 5. READ-back 必须用另一个 buffer

WRITE 完成后提交一个新的 READ：

```text
sector 80
→ read into buffer B
```

不要直接比较原 WRITE buffer A；A 本来就存着你准备写的 bytes，完全不能证明设备读回。

验证：

```text
buffer B != A 的同一片内存
B bytes == pattern A
```

再读 sector 81 验证 pattern B。

### A 次验收

- [ ] WRITE 的 descriptor direction 正确。
- [ ] status/used-id 真实完成。
- [ ] readback 使用不同 kernel buffer。
- [ ] 两个相邻 sector 的 pattern 不串写。
- [ ] 同次 READ-back 成功只被描述为“当前运行路径读回一致”，没有提前宣称 durable。

---

# B 次：FLUSH + 同一镜像重启验证

## 1. 先确认设备真正支持 FLUSH

读取 negotiated/offered feature：

```text
VIRTIO_BLK_F_FLUSH
```

如果当前第 37 课初始化时没有接受它，就在本课 driver 初始化中把它加入**已实现且设备提供**的 feature set，重新完整初始化设备。

如果设备不提供：

```text
报告 "flush unsupported in this environment"
→ 本课持久化验收不能通过
```

不能把一个空 `flush()` 函数返回 `Ok(())` 来冒充能力。

## 2. FLUSH request 本身也是一条真实 VirtIO request

概念 header：

```text
type     = VIRTIO_BLK_T_FLUSH
reserved = 0
sector   = 0   # 本课程固定清零未使用字段
```

链只需要：

```text
header → status
```

没有 data descriptor。

header 的 unused/reserved 字段保持确定的 0，避免把旧 request scratch 垃圾带入协议。

等待 used completion，并检查：

```text
status == OK
```

只有这时才能进入下一层证据。

## 3. 记录 QEMU block backend cache 配置

实验必须把完整 QEMU storage 参数记录下来。

特别禁止把：

```text
cache=unsafe
```

用于本课 durability 验收，因为这种模式会忽略 guest flush 请求，和我们正在验证的语义冲突。

选择一个支持/尊重 guest flush 的明确配置，并把：

- QEMU 版本；
- `-drive` / blockdev 参数；
- cache 模式；
- image 绝对路径；

写进测试日志。

课程结论只针对这套记录过的 QEMU/backend 配置，不泛化成真实 SSD/HDD 断电保证。

## 4. 两次启动必须使用**同一个文件**

### 第一次 QEMU

```text
启动 existing probe image
→ guest WRITE 新 marker 到 sector 80
→ device completion
→ guest FLUSH
→ flush completion
→ 输出 WRITE_RUN_DONE
→ 正常结束 QEMU
```

测试脚本记录 image 的：

```text
path
size
```

可以再记录 hash/mtime 作为辅助，但不要在第二次启动前改写测试区域。

### 第二次 QEMU

**禁止：**

```text
重新 mkdisk
重新写初始 probe
复制一个干净模板覆盖原 image
```

直接：

```text
用同一路径启动
→ 只 READ sector 80
→ 验证 persisted marker
→ 输出 REBOOT_READ_OK
```

这样才真正证明“跨 QEMU 生命周期仍可读到”。

## 5. 记录三层证据

学习记录明确分三栏：

```text
WRITE completion:       OK / error
FLUSH completion:       OK / unsupported / error
second-boot READ-back:  exact bytes/checksum
```

不要只写：

```text
“磁盘测试通过”
```

失去了最重要的层次。

---

## 6. 写/flush 状态不确定时不要继续做复杂元数据更新

如果：

```text
WRITE status IOERR
FLUSH IOERR
timeout/reset failure
```

block layer 把设备状态标成：

```text
WriteUncertain / Failed
```

本课策略：

- 停止新的 WRITE/FLUSH；
- 保留诊断；
- 允许的话只做明确安全的读取调查；
- 不把内存中的旧值“写回去”然后声称已经把磁盘回滚。

已经发给真实设备的写入，失败后可能处于课程无法完全判断的状态。

这个原则会一直延续到文件系统：

> runtime 内存 rollback 和“磁盘上的已发生写入”不是一回事。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| WRITE completion OK，但 same-run read 还是旧值 | descriptor 方向/sector/used/status/fence 是否正确 |
| same-run read OK，重启后旧值 | FLUSH 是否真的协商/提交/成功，backend cache 配置是否尊重 flush |
| 第二次启动 marker 又变成初始值 | 测试脚本是否偷偷 recreate image |
| 只读 backend 仍报告写成功 | 是否检查 VirtIO status，而不是只看到 used.idx 变化 |
| flush 函数总是成功但无 request | 是否写了 stub/no-op |
| failure 后继续写更多 metadata | device 是否已经进入 WriteUncertain/Failed 状态 |

## 最终验收

### WRITE

- [ ] 两个不同 sector 的 WRITE + 独立 READ-back 正确。
- [ ] readback buffer 与 write buffer 不是同一片内存。
- [ ] device status error 能被真实传播。

### FLUSH / persistence

- [ ] `VIRTIO_BLK_F_FLUSH` 真实协商。
- [ ] FLUSH 是真实 request，status=OK。
- [ ] QEMU backend cache 配置已记录且不使用忽略 guest flush 的 unsafe 模式。
- [ ] 第二次 QEMU 使用同一个 image，没有 recreate。
- [ ] second-boot READ 得到第一次 guest 写入的 exact bytes。

### 理解验收

不看正文回答：

1. WRITE request completion、same-run readback、flush completion、reboot readback 分别证明什么？
2. 为什么 readback 必须用另一个 buffer？
3. FLUSH request 为什么没有普通 data descriptor？
4. 为什么 `cache=unsafe` 不适合这次实验？
5. 为什么设备 I/O error 后不能靠“恢复内存旧值”声称磁盘也回滚了？
6. 为什么本实验仍不能等价于真实机器突然断电测试？

## 下一课为什么自然出现

现在每次文件系统读取都可以直接打到设备，但重复读取同一 4 KiB block 会非常浪费，而且文件系统需要在内存中修改 metadata 后统一写回。

下一课加一层：

```text
filesystem block cache
→ cache hit
→ dirty
→ writeback
→ global flush state
```

进入 [第 39 课：把常用磁盘块留在内存里](39-cache.md)。