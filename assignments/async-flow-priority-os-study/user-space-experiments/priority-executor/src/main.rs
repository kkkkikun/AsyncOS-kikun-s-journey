use priority_executor::*;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
use std::cell::RefCell;
use std::thread_local;

// ============================= TRACE MACRO ====================================
static TRACE_COUNTER: AtomicUsize = AtomicUsize::new(0);

macro_rules! trace {
    ($($arg:tt)*) => {
        let id = TRACE_COUNTER.fetch_add(1, Ordering::SeqCst);
        println!("[{:04}] {}", id, format!($($arg)*));
    };
}

thread_local! {
    static TRACE_ENABLED: RefCell<bool> = RefCell::new(true);
}

fn enable_trace() {
    TRACE_ENABLED.with(|enabled| *enabled.borrow_mut() = true);
}

fn disable_trace() {
    TRACE_ENABLED.with(|enabled| *enabled.borrow_mut() = false);
}

fn is_trace_enabled() -> bool {
    TRACE_ENABLED.with(|enabled| *enabled.borrow())
}

// ============================= MAIN ==========================================
fn main() {
    let start = Instant::now();
    trace!("main: enter");

    let reactor = Reactor::new();

    // Create PriorityExecutor (without aging for demo)
    let executor = std::sync::Arc::new(PriorityExecutor::new_without_aging());

    // Create three tasks with different priorities
    let fut1 = {
        let reactor_clone = reactor.clone();
        let start_clone = start;
        async move {
            trace!("exec_task_low: begin");
            let val = Task::new(reactor_clone, 1, 101).await;
            trace!("exec_task_low: resumed with value={}", val);
            println!("Got LOW priority task value {} at time: {:.2}.", val, start_clone.elapsed().as_secs_f32());
        }
    };

    let fut2 = {
        let reactor_clone = reactor.clone();
        let start_clone = start;
        async move {
            trace!("exec_task_high: begin");
            let val = Task::new(reactor_clone, 2, 102).await;
            trace!("exec_task_high: resumed with value={}", val);
            println!("Got HIGH priority task value {} at time: {:.2}.", val, start_clone.elapsed().as_secs_f32());
        }
    };

    let fut3 = {
        let reactor_clone = reactor.clone();
        let start_clone = start;
        async move {
            trace!("exec_task_normal: begin");
            let val = Task::new(reactor_clone, 3, 103).await;
            trace!("exec_task_normal: resumed with value={}", val);
            println!("Got NORMAL priority task value {} at time: {:.2}.", val, start_clone.elapsed().as_secs_f32());
        }
    };

    // Spawn tasks with different priorities
    executor.clone().spawn(Priority::Low, fut1);
    executor.clone().spawn(Priority::High, fut2);
    executor.clone().spawn(Priority::Normal, fut3);

    // Run the executor
    executor.run();

    reactor.lock().map(|mut r| r.close()).unwrap();
    trace!("main: exit");
}
