# 并发模型性能特征实验指南

## 实验目标

通过系统性实验，分析三种并发模型（进程/线程/协程）在不同情况下的性能特征和变化趋势。

---

## 📋 实验一：并发数扫描实验（核心实验）

### 目标

找到三种模型的性能拐点和扩展性特征

### 实验设计

| 实验组               | 并发数范围          | 负载类型 | 预期发现           |
| -------------------- | ------------------- | -------- | ------------------ |
| **A1: 线性扩展区**   | 1, 2, 4, 8, 16      | 纯I/O    | 线性扩展，无锁竞争 |
| **A2: 拐点探测区**   | 16, 32, 64, 128     | 纯I/O    | 线程模型拐点       |
| **A3: 高并发压力区** | 128, 256, 512, 1024 | 纯I/O    | 协程优势           |
| **A4: 极限压力区**   | 1024, 2048, 3072    | 纯I/O    | 系统极限           |

### 测试命令

#### A1: 线性扩展区测试

```bash
# 并发数 1, 2, 4, 8, 16 - 纯I/O模式
./target/release/rust_crawler compare --concurrency 1 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 2 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 4 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 8 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 16 --pure-io --mock --repeat 100
```

**观察重点**：

- 吞吐量是否线性增长
- 内存占用是否与并发数成正比
- 三种模型在低并发下的差异

#### A2: 拐点探测区测试

```bash
# 并发数 16, 32, 64, 128 - 纯I/O模式
./target/release/rust_crawler compare --concurrency 16 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 32 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 64 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 128 --pure-io --mock --repeat 100
```

**观察重点**：

- 线程模型何时开始出现性能下降（Thrashing）
- 协程模型是否保持稳定增长
- 进程模型的线性内存增长

#### A3: 高并发压力区测试

```bash
# 并发数 128, 256, 512, 1024 - 纯I/O模式
./target/release/rust_crawler compare --concurrency 128 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 256 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 512 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 1024 --pure-io --mock --repeat 100
```

**观察重点**：

- 线程模型是否崩溃（ulimit -n 限制）
- 协程模型在千并发下的表现
- 进程模型的内存压力

#### A4: 极限压力区测试

```bash
# 并发数 1024, 2048, 3072 - 纯I/O模式
./target/release/rust_crawler compare --concurrency 1024 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 2048 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 3072 --pure-io --mock --repeat 100
```

**观察重点**：

- 系统资源极限（文件描述符、内存）
- 哪种模型最先达到瓶颈
- 成功率和错误率变化

---

## 📋 实验二：CPU密集 vs I/O密集对比

### 目标

分析不同负载类型对并发模型的影响

### 实验设计

| 实验组            | 负载类型         | 固定并发数 | 预期发现                |
| ----------------- | ---------------- | ---------- | ----------------------- |
| **B1: CPU密集型** | HTML解析（默认） | 16         | 线程≈协程（Amdahl定律） |
| **B2: I/O密集型** | --pure-io        | 16         | 协程显著优于线程        |

### 测试命令

#### B1: CPU密集型测试（混合负载）

```bash
# 固定并发数 16，包含HTML解析
./target/release/rust_crawler compare --concurrency 16 --mock --repeat 100
```

**观察重点**：

- 线程与协程的性能差异（预期差异很小）
- 平均延迟和P95延迟
- CPU密集型任务的瓶颈特征

#### B2: I/O密集型测试（纯网络）

```bash
# 固定并发数 16，跳过HTML解析
./target/release/rust_crawler compare --concurrency 16 --pure-io --mock --repeat 100
```

**观察重点**：

- 协程模型的I/O优势是否明显
- 协程 vs 线程的吞吐量差异
- 内存占用的差异

---

## 📋 实验三：单模型深度分析

### 目标

分别测试每种模型的详细性能特征

### 测试命令

#### 进程模型单独测试

```bash
# 测试不同并发数下的进程模型
./target/release/rust_crawler run --crawler process --concurrency 4 --pure-io --mock --repeat 100
./target/release/rust_crawler run --crawler process --concurrency 16 --pure-io --mock --repeat 100
./target/release/rust_crawler run --crawler process --concurrency 64 --pure-io --mock --repeat 100
./target/release/rust_crawler run --crawler process --concurrency 160 --pure-io --mock --repeat 100
```

#### 线程模型单独测试

```bash
# 测试不同并发数下的线程模型
./target/release/rust_crawler run --crawler thread --concurrency 4 --pure-io --mock --repeat 500
./target/release/rust_crawler run --crawler thread --concurrency 16 --pure-io --mock --repeat 100
./target/release/rust_crawler run --crawler thread --concurrency 64 --pure-io --mock --repeat 100
./target/release/rust_crawler run --crawler thread --concurrency 256 --pure-io --mock --repeat 100
./target/release/rust_crawler run --crawler thread --concurrency 512 --pure-io --mock --repeat 100
```

#### 协程模型单独测试

```bash
# 测试不同并发数下的协程模型
./target/release/rust_crawler run --crawler async --concurrency 4 --pure-io --mock --repeat 500
./target/release/rust_crawler run --crawler async --concurrency 16 --pure-io --mock --repeat 500
./target/release/rust_crawler run --crawler async --concurrency 64 --pure-io --mock --repeat 500
./target/release/rust_crawler run --crawler async --concurrency 256 --pure-io --mock --repeat 100
./target/release/rust_crawler run --crawler async --concurrency 512 --pure-io --mock --repeat 100
./target/release/rust_crawler run --crawler async --concurrency 1024 --pure-io --mock --repeat 100
./target/release/rust_crawler run --crawler async --concurrency 2048 --pure-io --mock --repeat 100
./target/release/rust_crawler run --crawler async --concurrency 3072 --pure-io --mock --repeat 100
```

---

## 📊 数据记录表格模板

### 表1：并发数扫描实验（纯I/O）

| 并发数 | 进程-吞吐量(RPS) | 进程-内存(MB) | 线程-吞吐量(RPS) | 线程-内存(MB) | 协程-吞吐量(RPS) | 协程-内存(MB) |
| ------ | ---------------- | ------------- | ---------------- | ------------- | ---------------- | ------------- |
| 1      |                  |               |                  |               |                  |               |
| 2      |                  |               |                  |               |                  |               |
| 4      |                  |               |                  |               |                  |               |
| 8      |                  |               |                  |               |                  |               |
| 16     |                  |               |                  |               |                  |               |
| 32     |                  |               |                  |               |                  |               |
| 64     |                  |               |                  |               |                  |               |
| 128    |                  |               |                  |               |                  |               |
| 256    |                  |               |                  |               |                  |               |
| 512    |                  |               |                  |               |                  |               |
| 1024   |                  |               |                  |               |                  |               |
| 2048   |                  |               |                  |               |                  |               |
| 3072   |                  |               |                  |               |                  |               |

### 表2：CPU密集 vs I/O密集对比（并发=16）

| 负载类型             | 进程-耗时(s) | 进程-内存(MB) | 线程-耗时(s) | 线程-内存(MB) | 协程-耗时(s) | 协程-内存(MB) |
| -------------------- | ------------ | ------------- | ------------ | ------------- | ------------ | ------------- |
| CPU密集（含解析）    |              |               |              |               |              |               |
| I/O密集（--pure-io） |              |               |              |               |              |               |

---

## 🚀 快速执行脚本

### 方式1：逐个手动执行（推荐用于详细观察）

```bash
# 创建结果记录文件
echo "=== 并发数扫描实验 ===" > experiment_results.txt

# 执行并发数扫描（记录关键数据）
for c in 1 2 4 8 16 32 64 128 256 512 1024; do
    echo "并发数: $c" >> experiment_results.txt
    ./target/release/rust_crawler compare --concurrency $c --pure-io --mock --repeat 500 2>&1 | grep -E "(总耗时|吞吐率|内存增量)" >> experiment_results.txt
    echo "---" >> experiment_results.txt
done
```

### 方式2：批量自动执行

```bash
# 创建完整实验脚本
cat > run_all_experiments.sh << 'EOF'
#!/bin/bash
echo "开始执行完整实验套件..."

# 实验一：并发数扫描
echo "=== 实验一：并发数扫描 ===" > full_results.txt
for c in 1 2 4 8 16 32 64 128 256 512 1024 2048 3072; do
    echo "并发数: $c" >> full_results.txt
    date >> full_results.txt
    ./target/release/rust_crawler compare --concurrency $c --pure-io --mock --repeat 500 2>&1 | tee -a full_results.txt
    echo "=======================================" >> full_results.txt
    sleep 2  # 避免过热
done

# 实验二：负载类型对比
echo "=== 实验二：负载类型对比 ===" >> full_results.txt
echo "CPU密集型（含HTML解析）：" >> full_results.txt
./target/release/rust_crawler compare --concurrency 16 --mock --repeat 500 2>&1 | tee -a full_results.txt

echo "I/O密集型（--pure-io）：" >> full_results.txt
./target/release/rust_crawler compare --concurrency 16 --pure-io --mock --repeat 500 2>&1 | tee -a full_results.txt

echo "实验完成！结果已保存至 full_results.txt"
EOF

chmod +x run_all_experiments.sh
./run_all_experiments.sh
```

---

## 📈 分析维度

执行完实验后，请重点关注以下维度：

### 1. 扩展性分析

- **线性扩展区**（1-16并发）：哪些模型保持线性增长？
- **拐点识别**：线程模型在哪个并发数开始性能下降？
- **极限性能**：协程模型在最高并发下的表现如何？

### 2. 内存效率分析

- **内存增长率**：进程模型是否严格线性增长（13.35 MB × N）？
- **内存稳定性**：协程模型的内存增长是否平缓？
- **内存爆发点**：线程模型在哪个并发数出现内存激增？

### 3. 吞吐量-延迟权衡

- **低并发**（1-16）：三种模型的延迟差异
- **中并发**（16-128）：吞吐量的相对变化
- **高并发**（128+）：成功率和错误率的变化

### 4. CPU vs I/O 影响分析

- **CPU密集型**：线程 vs 协程的性能差异是否缩小？
- **I/O密集型**：协程的I/O优势是否明显？

---

## 🔍 建议的实验执行顺序

### 阶段1：快速扫描（30分钟）

```bash
# 快速执行几个关键并发点
./target/release/rust_crawler compare --concurrency 4 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 16 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 64 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 256 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 1024 --pure-io --mock --repeat 100
```

### 阶段2：精细分析（根据阶段1结果）

- 如果发现某个范围有性能拐点，补充测试该范围的更多并发点
- 如果发现某个性能特征异常，单独测试该模型

### 阶段3：对比验证

- 执行CPU密集 vs I/O密集对比实验
- 验证实验结论的一致性

---

## 📝 实验注意事项

1. **系统资源监控**

   ```bash
   # 另开终端监控系统资源
   watch -n 1 'ps aux | grep rust_crawler | wc -l'  # 进程数
   watch -n 1 'free -h'  # 内存使用
   watch -n 1 'ulimit -n'  # 文件描述符限制
   ```

2. **避免系统干扰**
   - 关闭不必要的后台程序
   - 确保系统有足够的可用内存
   - 使用 Mock 模式避免网络波动

3. **数据一致性**
   - 每次测试间隔至少 2 秒（让系统冷却）
   - 相同配置测试多次取平均值（如果需要高精度）

4. **错误处理**
   - 如果出现 "Too many open files" 错误，说明达到系统限制
   - 如果出现内存错误，说明达到系统内存极限

---

## 📊 预期发现与理论验证

根据理论分析，实验应该验证以下预期：

### 理论预期1：线性扩展区（1-16并发）

- 三种模型吞吐量都应该线性增长
- 协程模型在内存占用上应该有优势

### 理论预期2：线程模型拐点（64-256并发）

- 线程模型吞吐量增长应该变缓
- 可能出现上下文切换开销增大
- 可能出现文件描述符限制问题

### 理论预期3：协程模型优势（256+并发）

- 协程模型应该保持吞吐量增长
- 内存增长应该相对平缓
- 成功率应该维持100%

### 理论预期4：进程模型线性特征

- 内存占用应该严格线性增长
- 吞吐量应该相对稳定
- 创建开销应该是最大的瓶颈

---

## 🚀 开始实验

### 推荐的起点（从简单到复杂）

**第1步：验证环境**

```bash
# 先测试一个简单案例，确保环境正常
./target/release/rust_crawler compare --concurrency 4 --pure-io --mock --repeat 100
```

**第2步：执行核心实验**

```bash
# 执行并发数 4, 16, 64, 256, 1024 的核心测试
./target/release/rust_crawler compare --concurrency 4 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 16 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 64 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 256 --pure-io --mock --repeat 100
./target/release/rust_crawler compare --concurrency 1024 --pure-io --mock --repeat 100
```

**第3步：根据结果深入分析**

- 如果发现有趣的特征，补充测试更多并发点
- 对比CPU密集 vs I/O密集
- 单独测试特定模型

---

## 💡 数据解读提示

实验完成后，在解读数据时请记住：

1. **相对比较比绝对值更重要**：关注三种模型的相对差异趋势
2. **拐点位置最关键**：找出性能突变的并发数点
3. **内存增长率是扩展性指标**：进程模型的线性内存增长是可预测的
4. **成功率和吞吐量同样重要**：高吞吐量但低成功率没有实际意义

---

**祝实验顺利！记得记录每个测试的关键数据点。**
