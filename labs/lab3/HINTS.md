# Lab 3 提示

<details><summary>第一层：机制与策略分别在哪里？</summary>

模拟器可以先用整数 tick 逐步推进；用小输入手算核对。Ready 队列只负责存储顺序。SBI 封装负责固件调用，timer::arm 负责将期限与 STIE 接起来。
</details>

<details><summary>第二层：两个常见重复记账错误</summary>

yield 不能重置未用完的 slice，也不能清空累计 allotment；一次 slice 用完可以重置 slice，但 allotment 要继续累计。完成任务不再降级、入队或等待 boost。
</details>

<details><summary>第三层：为什么单项测试可能被前面的 TODO 卡住？</summary>

内核所有批次会先验证 Ready 队列，timer/preemption 必须先完成 push。timer 不触发时检查 SBI 参数是否绝对 deadline，以及 STIE 是否真的打开。STIE=32 超过 CSR 立即数五位范围，不能直接把 32 放进 csrsi 的立即数位置。
</details>
