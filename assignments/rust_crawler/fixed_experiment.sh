#!/bin/bash

# ==============================================================================
# 配置参数
# ==============================================================================
BINARY="./target/release/rust_crawler"
OUTPUT_DIR="./experiment_outputs"
CSV_FILE="$OUTPUT_DIR/experiment_metrics.csv"
LOG_FILE="$OUTPUT_DIR/raw_experiment_log.txt"
ROUNDS=3         # 每组配置重复测试 3 轮
FIXED_REPEAT=100  # 禁用延迟后速度极快，建议先用 10-50 压测，数据不够再往上调

# 创建输出目录
mkdir -p "$OUTPUT_DIR"

# ==============================================================================
# 1. 前置环境检查与编译
# ==============================================================================
echo "=================================================================="
echo "         Rust 爬虫并发模型实验控制台 (禁用人工延迟版)"
echo "=================================================================="

# 检查并自动编译 release
if [ ! -f "$BINARY" ]; then
    echo "[*] 未检测到 Release 二进制文件，正在开始编译..."
    cargo build --release
    if [ $? -ne 0 ]; then
        echo "[!] 编译失败，请检查 Rust 环境。"
        exit 1
    fi
fi

# 解除系统文件描述符限制
ulimit -n 65535 2>/dev/null || echo "[!] 权限不足，无法修改 ulimit。"

# 写入 CSV 报表表头
echo "Experiment_Type,Model_Type,Concurrency,Load_Type,Round,Duration_Sec,RPS,Avg_Delay_Ms,P95_Delay_Ms,Max_Memory_MB,Success_Rate" > "$CSV_FILE"
echo "--- 实验开始时间: $(date) ---" > "$LOG_FILE"

# ==============================================================================
# 核心执行函数
# ==============================================================================
run_test_group() {
    local exp_type=$1   # 实验分类
    local conn=$2       # 并发数
    local load_type=$3  # 纯IO还是混合负载
    local extra_flags=$4 # 额外的命令参数

    echo -e "\n[Running] 实验: $exp_type | 并发: $conn | 负载: $load_type"

    for ((r=1; r<=ROUNDS; r++)); do
        echo -n "  -> 轮次 $r/$ROUNDS ... "
        
        # 记录每轮执行的原始日志
        echo -e "\n--- Exp: $exp_type, Conn: $conn, Load: $load_type, Round: $r ---" >> "$LOG_FILE"
        
        # 【修复点】在这里全局追加了 --no-delay 参数
        local cmd="$BINARY compare --concurrency $conn --repeat $FIXED_REPEAT --mock --no-delay $extra_flags"
        echo "CMD: $cmd" >> "$LOG_FILE"
        
        # 执行命令并捕获输出
        local output
        output=$(eval "$cmd" 2>&1)
        echo "$output" >> "$LOG_FILE"
        
        # 使用 awk 精准切分表格（列3=进程, 列4=线程, 列5=协程）
        # 1. 总耗时
        local d_proc=$(echo "$output" | awk -F'│' '/总耗时/{print $3}' | tr -d ' ')
        local d_thrd=$(echo "$output" | awk -F'│' '/总耗时/{print $4}' | tr -d ' ')
        local d_coro=$(echo "$output" | awk -F'│' '/总耗时/{print $5}' | tr -d ' ')

        # 2. 吞吐率
        local r_proc=$(echo "$output" | awk -F'│' '/吞吐率/{print $3}' | tr -d ' ')
        local r_thrd=$(echo "$output" | awk -F'│' '/吞吐率/{print $4}' | tr -d ' ')
        local r_coro=$(echo "$output" | awk -F'│' '/吞吐率/{print $5}' | tr -d ' ')

        # 3. 平均延迟
        local avg_proc=$(echo "$output" | awk -F'│' '/平均延迟/{print $3}' | tr -d ' ')
        local avg_thrd=$(echo "$output" | awk -F'│' '/平均延迟/{print $4}' | tr -d ' ')
        local avg_coro=$(echo "$output" | awk -F'│' '/平均延迟/{print $5}' | tr -d ' ')

        # 4. P95延迟
        local p95_proc=$(echo "$output" | awk -F'│' '/P95延迟/{print $3}' | tr -d ' ')
        local p95_thrd=$(echo "$output" | awk -F'│' '/P95延迟/{print $4}' | tr -d ' ')
        local p95_coro=$(echo "$output" | awk -F'│' '/P95延迟/{print $5}' | tr -d ' ')

        # 5. 成功率
        local s_proc=$(echo "$output" | awk -F'│' '/成功率/{print $3}' | tr -d ' ')
        local s_thrd=$(echo "$output" | awk -F'│' '/成功率/{print $4}' | tr -d ' ')
        local s_coro=$(echo "$output" | awk -F'│' '/成功率/{print $5}' | tr -d ' ')

        # 6. 峰值内存
        local m_proc=$(echo "$output" | awk -F'│' '/峰值内存/{print $3}' | tr -d ' ')
        local m_thrd=$(echo "$output" | awk -F'│' '/峰值内存/{print $4}' | tr -d ' ')
        local m_coro=$(echo "$output" | awk -F'│' '/峰值内存/{print $5}' | tr -d ' ')
        
        # 验证是否解析成功
        if [ -z "$r_proc" ]; then
            echo "失败 (解析未成功)"
            echo "$exp_type,All,$conn,$load_type,$r,FAIL,FAIL,FAIL,FAIL,FAIL,FAIL" >> "$CSV_FILE"
        else
            echo "完成 (协程RPS: $r_coro | 线程RPS: $r_thrd)"
            # 将完整指标按顺序写入 CSV
            echo "$exp_type,Process,$conn,$load_type,$r,$d_proc,$r_proc,$avg_proc,$p95_proc,$m_proc,$s_proc" >> "$CSV_FILE"
            echo "$exp_type,Thread,$conn,$load_type,$r,$d_thrd,$r_thrd,$avg_thrd,$p95_thrd,$m_thrd,$s_thrd" >> "$CSV_FILE"
            echo "$exp_type,Coroutine,$conn,$load_type,$r,$d_coro,$r_coro,$avg_coro,$p95_coro,$m_coro,$s_coro" >> "$CSV_FILE"
        fi
        
        sleep 1
    done
    
    # 组间冷却
    if [ $conn -gt 512 ]; then
        sleep 3
    else
        sleep 1
    fi
}

# ==============================================================================
# 2. 自动化执行计划
# ==============================================================================

# --------- 实验一：并发数梯度扫描 ---------
echo -e "\n=== 启动【实验一：并发数扫描（纯I/O）】 ==="
CONCURRENCY_GRADIENT=(8 16 32 64 128 256 512 1024 2048 3000)

for c in "${CONCURRENCY_GRADIENT[@]}"; do
    run_test_group "Exp1_Concurrency" "$c" "Pure_IO" "--pure-io"
done

# --------- 实验二：混合负载对比 ---------
echo -e "\n=== 启动【实验二：混合负载对比（固定并发16）】 ==="
run_test_group "Exp2_LoadCompare" "16" "CPU_Bound_HTML" ""
run_test_group "Exp2_LoadCompare" "16" "Pure_IO" "--pure-io"

# ==============================================================================
# 3. 实验收尾
# ==============================================================================
echo -e "\n=================================================================="
echo "🎉 实验全部执行完毕！"
echo "📊 结构化数据已保存至: $CSV_FILE"
echo "=================================================================="

# 预览生成的数据
echo -e "\n统计数据预览 (前 15 行):"
head -n 15 "$CSV_FILE" | column -s, -t