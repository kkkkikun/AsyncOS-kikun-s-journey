use priority_executor::*;
use std::sync::Arc;
use priority_executor::test_helpers::*;

#[test]
fn high_priority_runs_first() {
    clear_execution_order();

    let executor = Arc::new(PriorityExecutor::new_without_aging());

    // Spawn three tasks with different priorities
    executor.clone().spawn(Priority::Low, async move {
        record_execution("low");
    });

    executor.clone().spawn(Priority::High, async move {
        record_execution("high");
    });

    executor.clone().spawn(Priority::Normal, async move {
        record_execution("normal");
    });

    // Run the executor
    executor.run();

    let order = get_execution_order();

    // Verify priority order: HIGH should run before NORMAL before LOW
    let high_idx = order.iter().position(|x| x == "high").unwrap();
    let normal_idx = order.iter().position(|x| x == "normal").unwrap();
    let low_idx = order.iter().position(|x| x == "low").unwrap();

    assert!(high_idx < normal_idx, "HIGH should run before NORMAL");
    assert!(normal_idx < low_idx, "NORMAL should run before LOW");

    println!("✓ high_priority_runs_first passed: {:?}", order);
}

#[test]
fn same_priority_fifo() {
    clear_execution_order();

    let executor = Arc::new(PriorityExecutor::new_without_aging());

    // Spawn three NORMAL tasks
    executor.clone().spawn(Priority::Normal, async move {
        record_execution("normal_1");
    });

    executor.clone().spawn(Priority::Normal, async move {
        record_execution("normal_2");
    });

    executor.clone().spawn(Priority::Normal, async move {
        record_execution("normal_3");
    });

    // Run the executor
    executor.run();

    let order = get_execution_order();

    // Verify FIFO order within same priority
    assert_eq!(order, vec!["normal_1", "normal_2", "normal_3"]);

    println!("✓ same_priority_fifo passed: {:?}", order);
}

#[test]
fn wake_requeues_to_correct_priority() {
    clear_execution_order();

    let executor = Arc::new(PriorityExecutor::new_without_aging());

    // Use Reactor with different timer durations to test wake behavior
    let reactor = Reactor::new();

    // Spawn HIGH priority task with 1 second timer
    let reactor_high = reactor.clone();
    executor.clone().spawn(Priority::High, async move {
        record_execution("high_start");
        let val = Task::new(reactor_high, 1, 1).await;
        record_execution(&format!("high_done_{}", val));
    });

    // Spawn NORMAL priority task with 1 second timer (same duration)
    let reactor_normal = reactor.clone();
    executor.clone().spawn(Priority::Normal, async move {
        record_execution("normal_start");
        let val = Task::new(reactor_normal, 1, 2).await;
        record_execution(&format!("normal_done_{}", val));
    });

    // Run executor in background thread
    let executor_clone = executor.clone();
    let handle = std::thread::spawn(move || {
        executor_clone.run();
    });

    // Wait for both tasks to complete
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Wait for executor thread
    handle.join().unwrap();

    // Clean up reactor
    reactor.lock().map(|mut r| r.close()).unwrap();

    let final_order = get_execution_order();

    println!("Final execution order: {:?}", final_order);

    // Both tasks should have started
    assert!(final_order.iter().any(|x| x.contains("high_start")), "HIGH should start");
    assert!(final_order.iter().any(|x| x.contains("normal_start")), "NORMAL should start");

    // Both tasks should have completed
    assert!(final_order.iter().any(|x| x.starts_with("high_done")), "HIGH should complete");
    assert!(final_order.iter().any(|x| x.starts_with("normal_done")), "NORMAL should complete");

    // The key assertion: after both tasks are woken, HIGH should be processed first
    // This verifies that wake() preserves priority
    let high_done_idx = final_order.iter().position(|x| x.starts_with("high_done")).unwrap();
    let normal_done_idx = final_order.iter().position(|x| x.starts_with("normal_done")).unwrap();

    assert!(high_done_idx < normal_done_idx, "HIGH should complete before NORMAL after wake");

    println!("✓ wake_requeues_to_correct_priority passed");
}

#[test]
fn pending_then_ready_transition() {
    // Force clear before this specific test
    let _ = std::fs::remove_file("/tmp/test_execution_order");
    clear_execution_order();

    let executor = Arc::new(PriorityExecutor::new_without_aging());

    // Use Reactor to test pending -> ready transition
    let reactor = Reactor::new();

    executor.clone().spawn(Priority::Normal, {
        let reactor_clone = reactor.clone();
        async move {
            record_execution("test_pending_phase");
            let val = Task::new(reactor_clone, 1, 99).await;
            record_execution(&format!("test_ready_phase_{}", val));
        }
    });

    // Run executor in background thread
    let executor_clone = executor.clone();
    let handle = std::thread::spawn(move || {
        executor_clone.run();
    });

    // Wait for task to complete
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Wait for executor thread
    handle.join().unwrap();

    // Clean up reactor
    reactor.lock().map(|mut r| r.close()).unwrap();

    let final_order = get_execution_order();

    println!("Final execution order: {:?}", final_order);

    // Check for our specific test markers
    let has_pending = final_order.iter().any(|x| x.contains("test_pending_phase"));
    let has_ready = final_order.iter().any(|x| x.contains("test_ready_phase"));

    assert!(has_pending, "Should have pending phase marker");
    assert!(has_ready, "Should have ready phase marker");

    println!("✓ pending_then_ready_transition passed");
}

#[test]
fn priority_respected_with_immediate_tasks() {
    clear_execution_order();

    let executor = Arc::new(PriorityExecutor::new_without_aging());

    // Test that priority is respected even when tasks complete immediately
    executor.clone().spawn(Priority::Low, async move {
        record_execution("low_immediate");
    });

    executor.clone().spawn(Priority::High, async move {
        record_execution("high_immediate");
    });

    executor.clone().spawn(Priority::Normal, async move {
        record_execution("normal_immediate");
    });

    executor.run();

    let order = get_execution_order();

    // Verify HIGH runs first
    assert_eq!(order[0], "high_immediate");

    // Verify NORMAL runs before LOW
    let normal_idx = order.iter().position(|x| x == "normal_immediate").unwrap();
    let low_idx = order.iter().position(|x| x == "low_immediate").unwrap();
    assert!(normal_idx < low_idx);

    println!("✓ priority_respected_with_immediate_tasks passed: {:?}", order);
}

#[test]
fn empty_executor_exits_immediately() {
    let executor = Arc::new(PriorityExecutor::new_without_aging());

    // This should exit immediately without hanging
    let start = std::time::Instant::now();
    executor.run();
    let elapsed = start.elapsed();

    assert!(elapsed < std::time::Duration::from_millis(100),
           "Empty executor should exit immediately, took {:?}", elapsed);

    println!("✓ empty_executor_exits_immediately passed: {:?}", elapsed);
}

#[test]
fn task_completion_decrements_counter() {
    let executor = Arc::new(PriorityExecutor::new_without_aging());

    assert_eq!(executor.inner.get_remaining(), 0, "Should start with 0 tasks");

    executor.clone().spawn(Priority::Normal, async move {
        // Immediate task
    });

    assert_eq!(executor.inner.get_remaining(), 1, "Should have 1 task");

    executor.clone().spawn(Priority::High, async move {
        // Another immediate task
    });

    assert_eq!(executor.inner.get_remaining(), 2, "Should have 2 tasks");

    executor.run();

    assert_eq!(executor.inner.get_remaining(), 0, "Should have 0 tasks after completion");

    println!("✓ task_completion_decrements_counter passed");
}

#[test]
fn spawn_increments_counter() {
    clear_execution_order();

    let executor = Arc::new(PriorityExecutor::new_without_aging());

    let initial_count = executor.inner.get_remaining();
    assert_eq!(initial_count, 0);

    // Spawn multiple tasks
    for i in 0..5 {
        executor.clone().spawn(Priority::Normal, async move {
            record_execution(&format!("task_{}", i));
        });
    }

    assert_eq!(executor.inner.get_remaining(), 5, "Should have 5 tasks");

    executor.run();

    assert_eq!(executor.inner.get_remaining(), 0, "All tasks should be completed");

    let order = get_execution_order();
    assert_eq!(order.len(), 5, "All 5 tasks should have executed");

    println!("✓ spawn_increments_counter passed: {:?}", order);
}
