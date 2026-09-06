# 第 37 课：第一次从 VirtIO 块设备读到数据

状态：待开始。前置：[第 36 课](36-disk.md) 验收完成。分 A、B、C 三次，每次 45～60 分钟；第一次调 virtqueue 卡住时可以继续拆分。

## 本课的唯一终点

从第 36 课那个**宿主机已经写好 marker 的 raw probe image** 中，真正通过 guest kernel 驱动读取：

```text
sector 64
→ 512 bytes
→ 得到 `neonos disk probe...`
```

本课只实现：

- modern virtio-mmio；
- split virtqueue；
- 单队列；
- 最多一个 in-flight request；
- polling used ring；
- READ。

暂时不实现：

- 外部中断/PLIC；
- 多请求并发；
- indirect descriptor；
- EVENT_IDX；
- 写入/flush；
- block cache。

设备协议以 [VirtIO 1.2 规范](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html) 为准。

---

# A 次：先把“设备真的准备好”做对

## 1. 发现设备，而不是猜某个 MMIO slot

从第 36 课记录的 DTB/virtio-mmio 候选开始，对每个候选读取：

```text
MagicValue
Version
DeviceID
VendorID
```

只接受：

```text
MagicValue == 0x74726976   # "virt"
Version    == 2            # modern virtio-mmio
DeviceID   == 2            # block device
```

如果 slot 空闲，`DeviceID` 可能是 0；这不是“坏磁盘”，只是这里没有已连接设备。

找到多个 block device 时，不靠“第一个就是测试盘”猜测；课程第一版可以要求测试 QEMU 只挂一个 block backend，并把选中的 MMIO base 打印出来。

## 2. 先读容量，固定单位

VirtIO block 的 `capacity` 按：

```text
512-byte sectors
```

计数。

16 MiB probe image 预期：

```text
capacity = 32768 sectors
```

本课第一条 read 固定：

```text
sector = 64
len    = 512 bytes
```

范围条件：

```text
sector + len/512 <= capacity
```

用 checked arithmetic；不要把文件系统 4096-byte block number 直接塞进 VirtIO `sector` 字段。

---

## 3. 把 VirtIO device status 当状态机，而不是几个随便 OR 的 bit

modern driver 初始化顺序：

```text
reset: status = 0
  ↓ 确认设备读回 0
ACKNOWLEDGE
  ↓
DRIVER
  ↓
feature negotiation
  ↓
FEATURES_OK
  ↓ 重新读取，确认设备没有清掉 FEATURES_OK
配置 virtqueue / device-specific state
  ↓
DRIVER_OK
```

课程实现每一步都打印一个短的状态摘要，失败立即停止，不继续“试试看”。

### 如果某一步不可恢复地失败

例如：

- modern `Version` 不符合；
- 必需 feature 缺失；
- `FEATURES_OK` 被设备拒绝；
- queue size 不够；

就：

```text
设置 FAILED（若已经进入 driver status 流程）
→ 停止这个 device path
```

重新尝试必须先做一次完整 device reset，再重新初始化；不能在半初始化状态上继续堆设置。

---

## 4. feature negotiation：只接受自己真正理解的能力

modern VirtIO 驱动必须处理/接受：

```text
VIRTIO_F_VERSION_1
```

如果设备没有提供这项，当前课程 modern driver 直接报告环境不符合，不偷偷退回 legacy register layout。

第 37 课不需要接受：

```text
INDIRECT_DESC
EVENT_IDX
多队列等可选 feature
```

没有实现就把对应 driver feature bit 留 0。

同时读取并记录 block device 是否提供：

```text
VIRTIO_BLK_F_FLUSH
```

但**本课不把“设备 offer FLUSH”当成已经实现持久化**。第 38 课才真正实现并验证 flush request。

feature selector 的 32-bit bank、高/低位拼接要明确；不要只读低 32 位后误以为 `VERSION_1` 不存在。

---

## 5. 用简单但容易证明的 virtqueue 布局

第一版 queue size 可以固定为：

```text
8 descriptors
```

前提：

```text
QueueNumMax >= 8
```

若当前设备连 8 都不支持，教学环境直接报告不支持；不为第一版同时引入动态 queue-size 算法。

一个 block request 需要 3 个 descriptor：

```text
0 request header
1 data buffer
2 status byte
```

### 为了减少“对齐 + 跨页”变量

第一版可以各用完整 zeroed frame 保存：

```text
descriptor table
available ring
used ring
request scratch/buffer
```

queue size=8 时它们都远小于 4 KiB，但一页一块让：

- 物理地址稳定；
- 对齐简单；
- 不会被 heap move；
- 设备访问期间不会被释放。

这些 frame 都由 driver 明确拥有，直到 device reset/driver teardown。

## 6. 写 queue address 时交的是物理地址

CPU/Rust 可以通过 kernel identity mapping 访问：

```text
某个 VA/指针
```

但 VirtIO MMIO queue address register 需要的是：

```text
descriptor/avail/used 的 physical address
```

必须从 frame ownership 中取得真正 PA，再拆低/高 32-bit 写入 modern virtio-mmio registers。

不能把：

```text
&desc as *const _ as usize
```

在未来非恒等映射下直接当设备 PA。

---

## 7. 配置 queue 完成后，最后才 `DRIVER_OK`

顺序至少：

```text
QueueSel
→ 读 QueueNumMax
→ 写 QueueNum=8
→ 写 DescLow/High
→ 写 Driver/AvailLow/High
→ 写 Device/UsedLow/High
→ QueueReady=1
→ 读回必要状态
→ DRIVER_OK
```

这正是之前课程里最重要的一处修正：

> `DRIVER_OK` 不能在 virtqueue/device-specific setup 之前提前设置。

### A 次验收

- [ ] 设备是从 DTB/MMIO 实际发现，不靠 slot 序号猜。
- [ ] Magic/Version/DeviceID/capacity 都与当前测试环境一致。
- [ ] `VIRTIO_F_VERSION_1` 通过完整 64-bit feature 流程确认。
- [ ] unsupported feature 没被 driver 接受。
- [ ] queue size/addresses/Ready 全部完成以后才 DRIVER_OK。
- [ ] queue frame ownership/物理地址可以画清楚。
- [ ] 此时还没发 READ request。

---

# B 次：提交第一条 512-byte READ

## 1. request header 的三个字段先写清

VirtIO block request header 概念上：

```text
type     = VIRTIO_BLK_T_IN   # guest read
reserved = 0
sector   = 64
```

注意名字：

```text
IN = data flows into guest buffer
```

不是“guest 输入给设备”。

`sector=64` 的含义固定是 512-byte sector 64。

## 2. status byte 先放一个“不可能误认成功”的 sentinel

例如：

```text
status = 0xff
```

设备完成后，本课接受：

```text
0 = OK
1 = IOERR
2 = UNSUPP
```

任何其他值都作为协议/设备错误报告，不把旧 buffer 内容当成功数据。

## 3. descriptor chain 的读写方向必须从“设备视角”理解

链：

```text
D0: request header
    device READS it
    WRITE flag = 0
    NEXT → D1

D1: 512-byte data buffer
    device WRITES it
    WRITE flag = 1
    NEXT → D2

D2: 1-byte status
    device WRITES it
    WRITE flag = 1
    no NEXT
```

如果把 D1 WRITE flag 写反，设备会把它当成“guest 提供给设备的数据”，READ request 就不会按预期工作。

header/data/status 在 request 完成前都属于**设备仍可能访问的稳定内存**，不能复用/释放。

---

## 4. 发布顺序：volatile MMIO 和 memory ordering 是两回事

在通知设备前，CPU 必须先完成：

```text
request header/status/data init
→ descriptor chain
→ avail.ring[slot] = head_descriptor
→ avail.idx 增加
→ notify device
```

其中 descriptor/ring 是普通共享内存，MMIO notify 是设备寄存器。

需要按 VirtIO/平台要求放置发布方向的 memory fence，确保设备看到 `avail.idx` 时，前面的 descriptor/header 内容已经可见。

不要写：

```text
“我用了 write_volatile，所以普通内存顺序也自动正确”
```

volatile 解决“这次 MMIO 访问不能被优化掉”，不替代 CPU↔device 的内存顺序协议。

## 5. `avail.idx` / `used.idx` 是 wrapping index

使用：

```text
u16 wrapping_add
```

不要写：

```text
if used.idx > last_used_idx
```

因为 65535→0 后这种比较会失效。

单 in-flight 第一版只需：

```text
while used.idx == last_used_idx:
    poll
```

一旦不相等，就按：

```text
last_used_idx % queue_size
```

读取对应 used element，再：

```text
last_used_idx = last_used_idx.wrapping_add(1)
```

## 6. used ring 变化后再做 acquire/read barrier

观察到 device 更新 `used.idx` 后，在读取：

```text
used.elem
status byte
data buffer
```

之前执行对应的消费/可见性同步，保证这些 device writes 对 CPU 可见。

然后验证：

```text
used.elem.id == 本次 head descriptor id
status == OK
```

只有这两项都成立，才比较 512-byte data buffer 中的 marker。

## 7. 不能只 grep 一段字符串就算 read 正确

至少检查：

```text
sector = 64
bytes_read = 512
prefix bytes 与宿主机 probe 完全一致
checksum / 已知 pattern 一致
```

并用另一个 sector（例如 65）确认不会永远返回“上一次 buffer 里残留的 marker”。

### B 次验收

- [ ] descriptor 方向按设备视角正确。
- [ ] status 从 0xff 变成 OK，而不是旧值。
- [ ] used element id 对得上本次 request。
- [ ] avail/used index 使用 wrapping 语义。
- [ ] memory fence 与 MMIO volatile 的职责能区分。
- [ ] sector 64 的真实 bytes 与宿主机 probe 一致。

---

# C 次：边界、重复复用和 timeout

## 1. 统一 block-driver 范围 API

第一版可以只支持：

```text
read_sectors(start_sector, kernel_buffer)
```

要求：

```text
buffer.len > 0
buffer.len % 512 == 0
num_sectors = len/512
start_sector.checked_add(num_sectors) <= capacity
```

本课实际测试仍以单 sector 为主。

错误：

- zero/non-sector-multiple length → invalid argument；
- overflow/out-of-capacity → range error；
- device status IOERR/UNSUPP → device error。

不要让“接近 disk end”的加法整数回绕后变成 sector 0。

## 2. 顺序提交超过 queue size 的请求

虽然 queue 有 8 descriptors，本课一次只允许一个 request。

连续做例如 100 次：

```text
submit
→ wait completion
→ reclaim descriptors
→ submit next
```

验证：

```text
head descriptors 可以重复使用
avail_idx/used_idx 持续 wrapping
free descriptor count 每次完成后回到基线
```

“连续请求数 > queue length”用来证明 descriptor 不会每次永久少 3 个。

另外用一个纯宿主机小模型，把 index 人工放到：

```text
65534, 65535, 0, 1
```

验证 ring slot `% queue_size` 和 wrapping 比较规则。

## 3. timeout 的安全规则比“返回错误”更重要

如果轮询超过测试上限：

```text
RequestTimeout
```

此时**不能**立刻：

```text
free data buffer
reuse descriptors
reuse header/status memory
```

因为设备可能只是慢，仍然拥有这些 DMA-visible addresses。

### 安全 recovery

第一版做：

```text
标记 device path Quiescing/Failed
→ status = 0 请求 device reset
→ 轮询确认 device status 读回 0
→ 只有确认 reset 完成以后，才认为旧 request 不再会访问这些 buffer
→ 重新初始化整个 virtio device/queue
```

如果无法确认 reset：

```text
停止这个 block device path
保留/冻结旧 request memory
不再发新请求
```

“宁可暂时泄漏几页实验内存，也不能把仍可能被设备 DMA 的内存交给别的对象”。

## 4. reset 后不是直接接着用旧 queue

完成 reset 后：

```text
device status/feature/queue state 被重新初始化
```

所以要重新走 A 次完整初始化流程，再发新的 probe read。

不能：

```text
status=0
→ 立刻认为原 QueueReady/feature negotiation 仍有效
```

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| QueueReady 但 request 永不完成 | physical queue address、descriptor flags、notify、feature/status 顺序 |
| marker 偶尔是旧数据 | status/used-id/fence 是否真的验证，buffer 是否未重新初始化 |
| 第 9/20/100 次 request 失败 | descriptors/index 是否每次 completion 后正确回收 |
| index 65535 附近逻辑错 | 是否用了普通大小比较而不是 u16 wrapping |
| timeout 后其他内存被设备改 | 是否没 reset 就复用了设备仍可能访问的 buffer |
| reset 后新 request 直接挂 | 是否忘了重新 negotiate/configure queue/DRIVER_OK |

## 最终验收

### 初始化

- [ ] modern status/feature/queue 顺序符合 VirtIO。
- [ ] 失败状态不会继续进入 DRIVER_OK。
- [ ] MMIO register 和 shared queue memory 的顺序职责明确。

### READ

- [ ] 512-byte sector 64 marker 与宿主机完全一致。
- [ ] status/used-id/descriptor direction 全部检查。
- [ ] 读另一个 sector 不会返回旧 marker 假阳性。

### 生命周期

- [ ] 100 次顺序 read descriptors 无泄漏。
- [ ] wrapping index 模型通过。
- [ ] 越界/状态错误有明确结果。
- [ ] timeout 不提前复用设备-owned memory。
- [ ] reset 必须确认 status=0，并完整重新初始化后才能恢复。

## 下一课为什么自然出现

现在 guest 已经能从磁盘读取真实数据，但“写 request 完成”不等于“数据在下一次 QEMU 启动后还存在”。

下一课专门把三个层次拆开：

```text
WRITE request completed
→ READ-back in same run
→ FLUSH completed
→ reboot same image read-back
```

进入 [第 38 课：写完了，真的保存了吗](38-block-write.md)。