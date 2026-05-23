#!/bin/bash
# 并发模型性能实验脚本 - 结构化数据版本
# 用法: ./run_structured_experiment.sh

set -e  # 遇到错误立即退出

# 创建结果目录
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULT_DIR="experiment_results_${TIMESTAMP}"
mkdir -p "$RESULT_DIR"

echo "=== 并发模型性能实验 ==="
echo "结果将保存至: $RESULT_DIR"
echo "开始时间: $(date)"
echo ""

# 创建CSV文件头
CSV_FILE="$RESULT_DIR/concurrency_scan.csv"
echo "并发数,模型,总耗时_s,吞吐量_RPS,平均延迟_ms,P95延迟_ms,峰值内存_MB,成功率_%" > "$CSV_FILE"

# 创建原始输出目录
RAW_DIR="$RESULT_DIR/raw_outputs"
mkdir -p "$RAW_DIR"

# 定义并发数列表
CONCURRENCY_LIST=(1 2 4 8 16 32 64 128 256 512 1024 2048 3072)

# 定义负载类型
LOAD_TYPES=("pure_io" "cpu_intensive")

# 辅助函数：提取数据
extract_metrics() {
    local output_file="$1"
    local concurrency="$2"
    local load_type="$3"

    # 提取总耗时
    local total_time=$(grep "总耗时" "$output_file" | grep -oP '\d+\.\d+(?=s)' || echo "N/A")

    # 提取吞吐量
    local throughput=$(grep "吞吐率" "$output_file" | grep -oP '\d+\.\d+(?= RPS)' || echo "N/A")

    # 提取平均延迟
    local avg_latency=$(grep "平均延迟" "$output_file" | grep -oP '\d+(?= ms)' || echo "N/A")

    # 提取P95延迟
    local p95_latency=$(grep "P95延迟" "$output_file" | grep -oP '\d+(?= ms)' || echo "N/A")

    # 提取峰值内存
    local peak_memory=$(grep "峰值内存" "$output_file" | grep -oP '\d+\.\d+(?= MB)' || echo "N/A")

    # 提取成功率（从PerformanceStats中）
    local success_rate=$(grep "成功率" "$output_file" | tail -1 | grep -oP '\d+\.\d+(?= %)' || echo "100.0")

    echo "$total_time,$throughput,$avg_latency,$p95_latency,$peak_memory,$success_rate"
}

# 实验一：并发数扫描（纯I/O）
echo "=== 实验一：并发数扫描（纯I/O）==="
echo "测试并发数: ${CONCURRENCY_LIST[@]}"
echo ""

for c in "${CONCURRENCY_LIST[@]}"; do
    echo "[测试] 并发数: $c (纯I/O)"

    # 执行测试并保存原始输出
    local output_file="$RAW_DIR/concurrency_${c}_pure_io.txt"
    ./target/release/rust_crawler compare \
        --concurrency "$c" \
        --pure-io \
        --mock \
        --repeat 500 \
        2>&1 | tee "$output_file"

    # 提取进程模型数据
    local metrics=$(extract_metrics "$output_file" "$c" "pure_io")
    echo "$c,进程,$metrics" >> "$CSV_FILE"

    # 提取线程模型数据
    metrics=$(extract_metrics "$output_file" "$c" "pure_io")
    echo "$c,线程,$metrics" >> "$CSV_FILE"

    # 提取协程模型数据
    metrics=$(extract_metrics "$output_file" "$c" "pure_io")
    echo "$c,协程,$metrics" >> "$CSV_FILE"

    echo "  ✓ 完成"
    sleep 1  # 让系统冷却
done

echo ""
echo "=== 实验二：负载类型对比（并发=16）==="

# CPU密集型测试
echo "[测试] CPU密集型（含HTML解析）"
local cpu_output="$RAW_DIR/cpu_intensive.txt"
./target/release/rust_crawler compare \
    --concurrency 16 \
    --mock \
    --repeat 500 \
    2>&1 | tee "$cpu_output"

echo "  ✓ 完成"

# I/O密集型测试
echo "[测试] I/O密集型（--pure-io）"
local io_output="$RAW_DIR/io_intensive.txt"
./target/release/rust_crawler compare \
    --concurrency 16 \
    --pure-io \
    --mock \
    --repeat 500 \
    2>&1 | tee "$io_output"

echo "  ✓ 完成"

# 生成汇总报告
echo ""
echo "=== 生成实验报告 ==="

REPORT_FILE="$RESULT_DIR/experiment_report.txt"
cat > "$REPORT_FILE" << EOF
======================================
并发模型性能实验报告
======================================
实验时间: $(date)
结果目录: $RESULT_DIR

一、实验设计
--------------------------------------
1. 并发数扫描: ${CONCURRENCY_LIST[@]} 个并发点
2. 负载类型: 纯I/O + CPU密集 vs I/O密集对比
3. 重复次数: 每个配置 500 次请求
4. 测试模式: Mock模式（本地离线测试）

二、关键发现
--------------------------------------
EOF

# 分析关键数据
echo "正在分析数据..." | tee -a "$REPORT_FILE"

# 找出最高吞吐量
echo "" | tee -a "$REPORT_FILE"
echo "最高吞吐量配置:" | tee -a "$REPORT_FILE"
awk -F',' '$3 > max {max=$3; line=$0} END {print line}' "$CSV_FILE" | tee -a "$REPORT_FILE"

# 统计协程模型在不同并发数下的性能趋势
echo "" | tee -a "$REPORT_FILE"
echo "协程模型性能趋势:" | tee -a "$REPORT_FILE"
echo "并发数,吞吐量(RPS)" | tee -a "$REPORT_FILE"
awk -F',' '$3 == "协程" {print $1","$3}' "$CSV_FILE" | tee -a "$REPORT_FILE"

# 内存使用对比（在并发=256时）
echo "" | tee -a "$REPORT_FILE"
echo "内存使用对比 (并发=256):" | tee -a "$REPORT_FILE"
awk -F',' '$1 == "256" {print $2","$6}' "$CSV_FILE" | tee -a "$REPORT_FILE"

echo "" | tee -a "$REPORT_FILE"
echo "详细数据请查看: $CSV_FILE" | tee -a "$REPORT_FILE"
echo "原始输出请查看: $RAW_DIR/" | tee -a "$REPORT_FILE"

# 生成R分析脚本（可选）
cat > "$RESULT_DIR/analysis.R" << 'R_EOF'
# R分析脚本（如果安装了R）
library(ggplot2)
library(dplyr)
library(tidyr)

# 读取数据
data <- read.csv("concurrency_scan.csv")

# 绘制吞吐量对比图
p1 <- ggplot(data, aes(x=并发数, y=吞吐量_RPS, color=模型)) +
    geom_line() +
    geom_point() +
    labs(title="三种并发模型的吞吐量对比",
         x="并发数",
         y="吞吐量 (RPS)") +
    theme_minimal()
ggsave("throughput_comparison.png", p1, width=10, height=6)

# 绘制内存使用对比图
p2 <- ggplot(data, aes(x=并发数, y=峰值内存_MB, color=模型)) +
    geom_line() +
    geom_point() +
    labs(title="三种并发模型的内存使用对比",
         x="并发数",
         y="峰值内存 (MB)") +
    theme_minimal()
ggsave("memory_comparison.png", p2, width=10, height=6)

print("图表已生成: throughput_comparison.png, memory_comparison.png")
R_EOF

echo ""
echo "======================================"
echo "实验完成！"
echo "======================================"
echo "结果保存位置: $RESULT_DIR"
echo "  - 结构化数据: $CSV_FILE"
echo "  - 原始输出: $RAW_DIR"
echo "  - 实验报告: $REPORT_FILE"
echo "  - R分析脚本: $RESULT_DIR/analysis.R"
echo ""
echo "完成时间: $(date)"
echo ""

# 快速预览结果
echo "=== 快速预览 ==="
echo ""
echo "关键指标对比（部分）:"
head -n 4 "$CSV_FILE" | column -t -s ','
echo "..."
tail -n 3 "$CSV_FILE" | column -t -s ','

echo ""
echo "提示: 使用以下命令查看完整数据"
echo "  cat $CSV_FILE | column -t -s ','"
echo "  less $REPORT_FILE"
