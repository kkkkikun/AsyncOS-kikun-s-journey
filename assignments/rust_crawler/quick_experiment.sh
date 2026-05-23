#!/bin/bash
# 快速实验脚本 - 5个关键并发点
# 用法: ./quick_experiment.sh

set -e

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULT_FILE="quick_experiment_${TIMESTAMP}.csv"

echo "=== 快并发模型实验 ==="
echo "结果保存至: $RESULT_FILE"
echo ""

# CSV表头
echo "并发数,进程_吞吐量,进程_内存,线程_吞吐量,线程_内存,协程_吞吐量,协程_内存" > "$RESULT_FILE"

# 测试5个关键并发点
CONCURRENCIES=(4 16 64 256 1024)

for c in "${CONCURRENCIES[@]}"; do
    echo "[测试] 并发数: $c"

    # 执行测试，捕获输出
    output=$(./target/release/rust_crawler compare \
        --concurrency "$c" \
        --pure-io \
        --mock \
        --repeat 100 \
        2>&1)

    # 提取进程模型数据
    process_rps=$(echo "$output" | grep "进程爬虫" | grep -oP '进程.*?\|\K\d+\.\d+(?= RPS)' || echo "N/A")
    process_mem=$(echo "$output" | grep "进程爬虫" | grep -oP '进程.*?\|\K\d+\.\d+(?= MB)' || echo "N/A")

    # 提取线程模型数据
    thread_rps=$(echo "$output" | grep "线程爬虫" | grep -oP '线程.*?\|\K\d+\.\d+(?= RPS)' || echo "N/A")
    thread_mem=$(echo "$output" | grep "线程爬虫" | grep -oP '线程.*?\|\K\d+\.\d+(?= MB)' || echo "N/A")

    # 提取协程模型数据
    async_rps=$(echo "$output" | grep "协程爬虫" | grep -oP '协程.*?\|\K\d+\.\d+(?= RPS)' || echo "N/A")
    async_mem=$(echo "$output" | grep "协程爬虫" | grep -oP '协程.*?\|\K\d+\.\d+(?= MB)' || echo "N/A")

    # 写入CSV
    echo "$c,$process_rps,$process_mem,$thread_rps,$thread_mem,$async_rps,$async_mem" >> "$RESULT_FILE"

    # 实时显示结果
    printf "%-4s | 进程: %8s RPS / %6s MB | 线程: %8s RPS / %6s MB | 协程: %8s RPS / %6s MB\n" \
        "$c" "$process_rps" "$process_mem" "$thread_rps" "$thread_mem" "$async_rps" "$async_mem"

    sleep 1
done

echo ""
echo "=== 实验完成 ==="
echo "详细数据已保存至: $RESULT_FILE"
echo ""
echo "查看数据:"
echo "  cat $RESULT_FILE | column -t -s ','"
