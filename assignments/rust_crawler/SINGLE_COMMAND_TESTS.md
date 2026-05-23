# 单命令快速测试指南

## 🚀 三种测试方式

### 方式1：快速扫描（推荐先执行）
```bash
./quick_experiment.sh
```
**特点**：
- ✅ 测试5个关键并发点（4, 16, 64, 256, 1024）
- ✅ 自动生成CSV文件
- ✅ 实时显示结果
- ⏱️ 耗时约5-10分钟

**输出**：`quick_experiment_[时间戳].csv`

---

### 方式2：完整实验（全面分析）
```bash
./run_structured_experiment.sh
```
**特点**：
- ✅ 测试13个并发点
- ✅ 纯I/O + CPU密集对比
- ✅ 生成结构化数据目录
- ✅ 自动生成分析报告
- ⏱️ 耗时约20-30分钟

**输出**：
```
experiment_results_[时间戳]/
├── concurrency_scan.csv         # 结构化数据
├── raw_outputs/                   # 原始输出
│   ├── concurrency_4_pure_io.txt
│   ├── concurrency_16_pure_io.txt
│   └── ...
├── experiment_report.txt         # 自动分析报告
└── analysis.R                     # R分析脚本
```

---

### 方式3：单次精细测试
```bash
# 测试特定并发点
./target/release/rust_crawler compare --concurrency 32 --pure-io --mock --repeat 500
```

---

## 📊 数据分析工具

### 自动分析
```bash
# 实验完成后，自动分析数据
python3 analyze_results.py quick_experiment_*.csv
```

**分析内容包括**：
1. 数据概览
2. 性能峰值识别
3. 扩展性分析
4. 内存效率分析
5. 协程优势分析
6. 性能拐点识别

### 手动查看CSV
```bash
# 查看CSV数据（对齐显示）
cat quick_experiment_*.csv | column -t -s ','

# 只看协程模型
cat quick_experiment_*.csv | cut -d',' -f1,7 | column -t

# 对比三个模型
cat quick_experiment_*.csv | awk -F',' '{printf "%4s | 进程: %8s | 线程: %8s | 协程: %8s\n", $1, $3, $5, $7}'
```

---

## 🎯 推荐执行流程

### Step 1: 快速验证（5分钟）
```bash
./quick_experiment.sh
```
快速了解三个模型在5个关键并发点的表现差异。

### Step 2: 数据分析（立即执行）
```bash
python3 analyze_results.py quick_experiment_*.csv
```
自动识别性能拐点、峰值和趋势。

### Step 3: 深入实验（如果需要）
```bash
# 基于Step 2的分析结果，补充测试特定并发点
./target/release/rust_crawler compare --concurrency 128 --pure-io --mock --repeat 500

# 或运行完整实验
./run_structured_experiment.sh
```

---

## 📈 关键观察点

执行测试时，请重点关注：

### 1. 线性扩展区（1-16并发）
- 三种模型吞吐量是否线性增长？
- 内存占用是否与并发数成正比？
- **预期**：协程模型内存占用最低

### 2. 拐点探测区（32-128并发）
- 线程模型何时开始增长放缓？
- 协程模型是否保持稳定增长？
- **预期**：线程模型在64-128附近出现拐点

### 3. 高并发压力区（256+并发）
- 线程模型是否崩溃或性能急剧下降？
- 协程模型是否保持100%成功率？
- **预期**：协程模型优势明显

---

## 💡 实验技巧

### 如果测试卡住
```bash
# Ctrl+C 中断当前测试
# 减少并发数或repeat次数
```

### 如果出现"Too many open files"
```bash
# 说明达到了系统文件描述符限制
# 这是预期的，证明线程模型撞到了系统极限
```

### 如果需要更精确的数据
```bash
# 增加repeat次数（减少随机误差）
--repeat 1000

# 或测试更多并发点
for c in {1..10}; do
  n=$((c * 100))
  ./target/release/rust_crawler compare --concurrency $n --pure-io --mock --repeat 500
done
```

---

## 🚀 开始第一个实验

### 最简单的方式：
```bash
cd /home/kikun/School/opencamp-project/assignments/rust_crawler
./quick_experiment.sh
```

### 完整的方式：
```bash
cd /home/kikun/School/opencamp-project/assignments/rust_crawler
./run_structured_experiment.sh
```

### 单次测试：
```bash
cd /home/kikun/School/opencamp-project/assignments/rust_crawler
./target/release/rust_crawler compare --concurrency 16 --pure-io --mock --repeat 500
```

选择您喜欢的方式开始实验吧！
