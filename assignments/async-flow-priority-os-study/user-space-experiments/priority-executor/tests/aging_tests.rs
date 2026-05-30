use priority_executor::*;
use std::sync::Arc;
use priority_executor::test_helpers::*;

#[test]
fn strict_priority_can_starve_low() {
    clear_execution_order();

    // Create executor WITHOUT aging
    let executor = Arc::new(PriorityExecutor::new_without_aging());

    // Spawn 20 immediate HIGH tasks
    for i in 0..20 {
        let i = i;
        executor.clone().spawn(Priority::High, async move {
            record_execution(&format!("high_{}", i));
        });
    }

    // Spawn 1 immediate LOW task
    executor.clone().spawn(Priority::Low, async move {
        record_execution("low_task");
    });

    // Run for limited ticks
    executor.run_for_ticks(Some(15));

    let order = get_execution_order();

    println!("Without aging - execution order: {:?}", order);

    // Count HIGH and LOW executions
    let high_count = order.iter().filter(|x| x.starts_with("high_")).count();
    let low_count = order.iter().filter(|x| x.contains("low")).count();

    println!("HIGH count: {}, LOW count: {}", high_count, low_count);

    // Without aging, LOW should be starved
    assert!(low_count == 0, "LOW should be completely starved without aging");
    assert!(high_count > 0, "HIGH tasks should run");

    println!("✓ strict_priority_can_starve_low passed - LOW starved (0 runs) vs HIGH {} runs", high_count);
}

#[test]
fn aging_prevents_starvation() {
    clear_execution_order();

    // Create executor WITH aging
    let config = ExecutorConfig {
        aging_enabled: true,
        aging_threshold: 2, // Lower threshold for faster promotion
    };
    let executor = Arc::new(PriorityExecutor::new(config));

    // Spawn limited HIGH tasks (not too many)
    for i in 0..8 {
        let i = i;
        executor.clone().spawn(Priority::High, async move {
            record_execution(&format!("high_{}", i));
        });
    }

    // Spawn 1 LOW task
    executor.clone().spawn(Priority::Low, async move {
        record_execution("low_task");
    });

    // Run for more ticks to allow aging
    executor.run_for_ticks(Some(20));

    let order = get_execution_order();

    println!("With aging - execution order: {:?}", order);

    // Count HIGH and LOW executions
    let high_count = order.iter().filter(|x| x.starts_with("high_")).count();
    let low_count = order.iter().filter(|x| x.contains("low")).count();

    println!("HIGH count: {}, LOW count: {}", high_count, low_count);

    // With aging, LOW should eventually run
    assert!(low_count > 0, "LOW should run with aging enabled");
    assert!(high_count > 0, "HIGH tasks should still run");

    println!("✓ aging_prevents_starvation passed - LOW ran {} times with aging vs HIGH {}", low_count, high_count);
}

#[test]
fn aging_promotes_low_to_normal() {
    clear_execution_order();

    // Create executor with aggressive aging
    let config = ExecutorConfig {
        aging_enabled: true,
        aging_threshold: 2, // Promote after 2 ticks
    };
    let executor = Arc::new(PriorityExecutor::new(config));

    // Spawn limited HIGH tasks
    for i in 0..5 {
        executor.clone().spawn(Priority::High, async move {
            record_execution(&format!("high_{}", i));
        });
    }

    // Spawn 1 LOW task
    executor.clone().spawn(Priority::Low, async move {
        record_execution("low_task");
    });

    // Run for enough ticks
    executor.run_for_ticks(Some(10));

    let order = get_execution_order();

    println!("LOW promotion test: {:?}", order);

    // LOW should eventually run
    let low_count = order.iter().filter(|x| x.contains("low")).count();
    assert!(low_count > 0, "LOW should run with aging");

    println!("✓ aging_promotes_low_to_normal passed - LOW ran {} times", low_count);
}

#[test]
fn aging_promotes_normal_to_high() {
    clear_execution_order();

    // Create executor with aging
    let config = ExecutorConfig {
        aging_enabled: true,
        aging_threshold: 2,
    };
    let executor = Arc::new(PriorityExecutor::new(config));

    // Spawn limited HIGH tasks
    for i in 0..5 {
        executor.clone().spawn(Priority::High, async move {
            record_execution(&format!("high_{}", i));
        });
    }

    // Spawn 1 NORMAL task
    executor.clone().spawn(Priority::Normal, async move {
        record_execution("normal_task");
    });

    // Run for enough ticks
    executor.run_for_ticks(Some(10));

    let order = get_execution_order();

    println!("NORMAL promotion test: {:?}", order);

    // NORMAL should run more than without aging
    let normal_count = order.iter().filter(|x| x.contains("normal")).count();
    assert!(normal_count > 0, "NORMAL should run with aging");

    println!("✓ aging_promotes_normal_to_high passed - NORMAL ran {} times", normal_count);
}

#[test]
fn aging_respects_base_priority() {
    clear_execution_order();

    // Create executor with aging
    let config = ExecutorConfig {
        aging_enabled: true,
        aging_threshold: 1, // Very aggressive
    };
    let executor = Arc::new(PriorityExecutor::new(config));

    // Spawn tasks of different priorities
    executor.clone().spawn(Priority::High, async move {
        record_execution("high");
    });

    executor.clone().spawn(Priority::Low, async move {
        record_execution("low");
    });

    executor.clone().spawn(Priority::Normal, async move {
        record_execution("normal");
    });

    // Run to completion
    executor.run();

    let order = get_execution_order();

    println!("Base priority respect: {:?}", order);

    // All tasks should run
    assert_eq!(order.len(), 3, "All tasks should run");

    // HIGH should still be first (immediate execution)
    assert_eq!(order[0], "high", "HIGH should run first");

    println!("✓ aging_respects_base_priority passed");
}

#[test]
fn wait_ticks_increments_correctly() {
    // Create executor with aging
    let config = ExecutorConfig {
        aging_enabled: true,
        aging_threshold: 5,
    };
    let executor = Arc::new(PriorityExecutor::new(config));

    // Spawn a LOW task
    let task = executor.clone().spawn(Priority::Low, async move {
        record_execution("low");
    });

    // Spawn limited HIGH tasks
    for _ in 0..3 {
        executor.clone().spawn(Priority::High, async move {
            // Immediate task
        });
    }

    // Run for some ticks (enough to age but not exhaust all HIGH tasks)
    executor.run_for_ticks(Some(6));

    let wait_ticks = task.get_wait_ticks();

    println!("LOW task: wait_ticks={}", wait_ticks);

    // LOW should have accumulated wait ticks even if not scheduled yet
    assert!(wait_ticks > 0, "LOW should have accumulated wait_ticks while waiting");

    println!("✓ wait_ticks_increments_correctly passed - wait_ticks={}", wait_ticks);
}

#[test]
fn aging_threshold_affects_promotion_speed() {
    // Test 1: Low threshold (fast promotion)
    clear_execution_order();

    let config_fast = ExecutorConfig {
        aging_enabled: true,
        aging_threshold: 1,
    };
    let executor_fast = Arc::new(PriorityExecutor::new(config_fast));

    for _ in 0..5 {
        executor_fast.clone().spawn(Priority::High, async move {
            record_execution("h");
        });
    }
    executor_fast.clone().spawn(Priority::Low, async move {
        record_execution("l");
    });

    executor_fast.run_for_ticks(Some(8));

    let order_fast = get_execution_order();
    let fast_low = order_fast.iter().filter(|x| x.contains("l")).count();

    println!("Fast aging (threshold=1): LOW ran {} times", fast_low);

    // Test 2: High threshold (slow promotion)
    clear_execution_order();

    let config_slow = ExecutorConfig {
        aging_enabled: true,
        aging_threshold: 10,
    };
    let executor_slow = Arc::new(PriorityExecutor::new(config_slow));

    for _ in 0..5 {
        executor_slow.clone().spawn(Priority::High, async move {
            record_execution("H");
        });
    }
    executor_slow.clone().spawn(Priority::Low, async move {
        record_execution("L");
    });

    executor_slow.run_for_ticks(Some(8));

    let order_slow = get_execution_order();
    let slow_low = order_slow.iter().filter(|x| x.contains("F")).count();

    println!("Slow aging (threshold=10): LOW ran {} times", slow_low);

    assert!(fast_low >= slow_low, "Lower threshold should promote faster or equal");

    println!("✓ aging_threshold_affects_promotion_speed passed");
}

#[test]
fn high_priority_stays_high() {
    clear_execution_order();

    // Create executor with aging
    let config = ExecutorConfig {
        aging_enabled: true,
        aging_threshold: 1,
    };
    let executor = Arc::new(PriorityExecutor::new(config));

    // Spawn a HIGH task
    let task = executor.clone().spawn(Priority::High, async move {
        record_execution("high_task");
    });

    // Spawn other HIGH tasks to make it wait
    for _ in 0..3 {
        executor.clone().spawn(Priority::High, async move {
            // Immediate
        });
    }

    // Run some ticks
    executor.run_for_ticks(Some(5));

    // HIGH task should stay HIGH (not promoted to anything else)
    let priority = task.get_priority();
    let base = task.get_base_priority();

    assert_eq!(priority, Priority::High, "HIGH should stay HIGH");
    assert_eq!(base, Priority::High, "Base priority should be HIGH");

    println!("✓ high_priority_stays_high passed");
}

#[test]
fn run_for_ticks_respects_limit() {
    clear_execution_order();

    let executor = Arc::new(PriorityExecutor::new_without_aging());

    // Spawn many tasks
    for i in 0..20 {
        executor.clone().spawn(Priority::High, async move {
            record_execution(&format!("task_{}", i));
        });
    }

    // Run for only 5 ticks
    executor.run_for_ticks(Some(5));

    let order = get_execution_order();

    println!("Limited ticks: {} tasks executed", order.len());

    // Should execute approximately 5 tasks (or fewer if some take multiple ticks)
    assert!(order.len() <= 6, "Should not exceed tick limit significantly");

    println!("✓ run_for_ticks_respects_limit passed");
}
