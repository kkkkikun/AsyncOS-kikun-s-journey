# Async Flow Priority Study

这个仓库记录了我对Rust用户空间线程、无栈协程、Future执行器以及它们可能应用于 Vegar/ArceOS I/O调度的研究。

## Goals

1. 跟踪绿色线程和 Rust Futures 中的执行流状态转换。
2. 实现一个优先级感知的 Future 执行器。
3. 分析优先级感知的唤醒机制如何缓解 ArceOS I/O 饥饿问题。
4. 提供报告和可复现的测试日志。
