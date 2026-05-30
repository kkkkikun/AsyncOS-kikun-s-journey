use std::{
    collections::VecDeque,
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicUsize, AtomicU8, Ordering},
    sync::{mpsc::channel, Arc, Mutex, Condvar},
    task::{Context, Poll, Waker},
    thread::{self, JoinHandle},
    time::Duration,
};

use futures::task::{ArcWake, waker_ref};

// ============================= PRIORITY ====================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::High => "HIGH",
            Priority::Normal => "NORMAL",
            Priority::Low => "LOW",
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => Priority::Low,
            1 => Priority::Normal,
            2 => Priority::High,
            _ => Priority::Low,
        }
    }
}

// ============================= EXEC TASK ====================================
pub struct ExecTask {
    pub id: usize,
    pub base_priority: Priority,
    pub current_priority: Mutex<Priority>,
    pub wait_ticks: AtomicUsize,
    pub scheduled_count: AtomicUsize,
    future: Mutex<Option<Pin<Box<dyn Future<Output = ()> + Send>>>>,
}

impl ExecTask {
    pub fn new(id: usize, priority: Priority, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> Self {
        ExecTask {
            id,
            base_priority: priority,
            current_priority: Mutex::new(priority),
            wait_ticks: AtomicUsize::new(0),
            scheduled_count: AtomicUsize::new(0),
            future: Mutex::new(Some(future)),
        }
    }

    pub fn get_priority(&self) -> Priority {
        let current = self.current_priority.lock().unwrap();
        *current
    }

    pub fn set_priority(&self, priority: Priority) {
        let mut current = self.current_priority.lock().unwrap();
        *current = priority;
    }

    pub fn get_base_priority(&self) -> Priority {
        self.base_priority
    }

    pub fn increment_wait_ticks(&self) -> usize {
        self.wait_ticks.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn get_wait_ticks(&self) -> usize {
        self.wait_ticks.load(Ordering::SeqCst)
    }

    pub fn reset_wait_ticks(&self) {
        self.wait_ticks.store(0, Ordering::SeqCst);
    }

    pub fn increment_scheduled_count(&self) -> usize {
        self.scheduled_count.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn get_scheduled_count(&self) -> usize {
        self.scheduled_count.load(Ordering::SeqCst)
    }
}

// ============================= EXECUTOR CONFIG =================================
#[derive(Clone, Copy)]
pub struct ExecutorConfig {
    pub aging_enabled: bool,
    pub aging_threshold: usize,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        ExecutorConfig {
            aging_enabled: false,
            aging_threshold: 5,
        }
    }
}

// ============================= READY QUEUES ====================================
struct ReadyQueues {
    high: VecDeque<Arc<ExecTask>>,
    normal: VecDeque<Arc<ExecTask>>,
    low: VecDeque<Arc<ExecTask>>,
}

impl ReadyQueues {
    fn new() -> Self {
        ReadyQueues {
            high: VecDeque::new(),
            normal: VecDeque::new(),
            low: VecDeque::new(),
        }
    }

    fn enqueue(&mut self, task: Arc<ExecTask>) {
        let priority = task.get_priority();
        match priority {
            Priority::High => {
                self.high.push_back(task);
            }
            Priority::Normal => {
                self.normal.push_back(task);
            }
            Priority::Low => {
                self.low.push_back(task);
            }
        }
    }

    fn pop_next(&mut self) -> Option<Arc<ExecTask>> {
        // Try high queue first, then normal, then low
        if let Some(task) = self.high.pop_front() {
            return Some(task);
        }
        if let Some(task) = self.normal.pop_front() {
            return Some(task);
        }
        if let Some(task) = self.low.pop_front() {
            return Some(task);
        }
        None
    }

    fn retain_high<F>(&mut self, f: F)
    where
        F: Fn(&Arc<ExecTask>) -> bool,
    {
        self.high.retain(f);
    }

    fn retain_normal<F>(&mut self, f: F)
    where
        F: Fn(&Arc<ExecTask>) -> bool,
    {
        self.normal.retain(f);
    }

    fn retain_low<F>(&mut self, f: F)
    where
        F: Fn(&Arc<ExecTask>) -> bool,
    {
        self.low.retain(f);
    }

    fn is_empty(&self) -> bool {
        self.high.is_empty() && self.normal.is_empty() && self.low.is_empty()
    }
}

// ============================= EXECUTOR INNER ================================
pub struct ExecutorInner {
    queues: Mutex<ReadyQueues>,
    parker: Arc<Parker>,
    pub remaining_tasks: AtomicUsize,
    pub config: ExecutorConfig,
}

impl ExecutorInner {
    pub fn new(config: ExecutorConfig) -> Self {
        ExecutorInner {
            queues: Mutex::new(ReadyQueues::new()),
            parker: Arc::new(Parker::default()),
            remaining_tasks: AtomicUsize::new(0),
            config,
        }
    }

    pub fn enqueue(&self, task: Arc<ExecTask>) {
        let mut queues = self.queues.lock().unwrap();
        queues.enqueue(task);
        drop(queues);
        self.parker.unpark();
    }

    pub fn pop_next(&self) -> Option<Arc<ExecTask>> {
        let mut queues = self.queues.lock().unwrap();
        queues.pop_next()
    }

    pub fn is_empty(&self) -> bool {
        let queues = self.queues.lock().unwrap();
        queues.is_empty()
    }

    pub fn decrement_remaining(&self) {
        let prev = self.remaining_tasks.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn park(&self) {
        self.parker.park();
    }

    pub fn get_remaining(&self) -> usize {
        self.remaining_tasks.load(Ordering::SeqCst)
    }

    pub fn apply_aging(&self, max_ticks: Option<usize>) -> Vec<String> {
        let mut aging_logs = Vec::new();

        if !self.config.aging_enabled {
            return aging_logs;
        }

        let mut queues = self.queues.lock().unwrap();
        let mut tasks_to_promote: Vec<(Arc<ExecTask>, Priority)> = Vec::new();

        // Increment wait_ticks for all waiting tasks
        for task in queues.high.iter().chain(queues.normal.iter()).chain(queues.low.iter()) {
            let ticks = task.increment_wait_ticks();
            if ticks >= self.config.aging_threshold {
                let current = task.get_priority();
                let new_priority = match current {
                    Priority::Low => Some(Priority::Normal),
                    Priority::Normal => Some(Priority::High),
                    Priority::High => None, // High stays High
                };

                if let Some(new_prio) = new_priority {
                    tasks_to_promote.push((task.clone(), new_prio));
                    let from_str = current.as_str();
                    let to_str = new_prio.as_str();
                    aging_logs.push(format!("promote: task-{} {} -> {}", task.id, from_str, to_str));
                } else {
                    aging_logs.push(format!("aging: task-{} wait_ticks={}", task.id, ticks));
                }
            } else {
                aging_logs.push(format!("aging: task-{} wait_ticks={}", task.id, ticks));
            }
        }

        // Promote tasks (remove from current queue, add to higher queue)
        for (task, new_priority) in tasks_to_promote {
            // Remove from current queue
            let current_priority = task.get_priority();
            match current_priority {
                Priority::High => {} // Should not happen
                Priority::Normal => {
                    queues.high.retain(|t| t.id != task.id);
                    queues.normal.retain(|t| t.id != task.id);
                }
                Priority::Low => {
                    queues.high.retain(|t| t.id != task.id);
                    queues.normal.retain(|t| t.id != task.id);
                    queues.low.retain(|t| t.id != task.id);
                }
            }

            // Update task priority
            task.set_priority(new_priority);

            // Add to new queue
            match new_priority {
                Priority::High => queues.high.push_back(task),
                Priority::Normal => queues.normal.push_back(task),
                Priority::Low => queues.low.push_back(task),
            }
        }

        aging_logs
    }
}

// ============================= TASK WAKER ====================================
pub struct TaskWaker {
    pub task: Arc<ExecTask>,
    pub executor: Arc<ExecutorInner>,
}

impl ArcWake for TaskWaker {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        let task = arc_self.task.clone();
        let executor = arc_self.executor.clone();
        executor.enqueue(task);
    }

    fn wake(self: Arc<Self>) {
        Self::wake_by_ref(&self);
    }
}

// ============================= PRIORITY EXECUTOR ==============================
pub struct PriorityExecutor {
    pub inner: Arc<ExecutorInner>,
    next_task_id: AtomicUsize,
    max_ticks: AtomicUsize,
    current_tick: AtomicUsize,
}

impl PriorityExecutor {
    pub fn new(config: ExecutorConfig) -> Self {
        PriorityExecutor {
            inner: Arc::new(ExecutorInner::new(config)),
            next_task_id: AtomicUsize::new(0),
            max_ticks: AtomicUsize::new(usize::MAX),
            current_tick: AtomicUsize::new(0),
        }
    }

    pub fn new_with_aging() -> Self {
        Self::new(ExecutorConfig {
            aging_enabled: true,
            aging_threshold: 3,
        })
    }

    pub fn new_without_aging() -> Self {
        Self::new(ExecutorConfig {
            aging_enabled: false,
            aging_threshold: 5,
        })
    }

    pub fn spawn<F>(self: &Arc<Self>, priority: Priority, future: F) -> Arc<ExecTask>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let id = self.next_task_id.fetch_add(1, Ordering::SeqCst);

        let boxed_future = Box::pin(future);
        let exec_task = Arc::new(ExecTask::new(id, priority, boxed_future));

        // Increment remaining tasks counter
        self.inner.remaining_tasks.fetch_add(1, Ordering::SeqCst);

        // Enqueue for first execution
        self.inner.enqueue(exec_task.clone());

        exec_task
    }

    pub fn run(&self) {
        self.run_for_ticks(None);
    }

    pub fn run_for_ticks(&self, max_ticks: Option<usize>) {
        if let Some(max) = max_ticks {
            self.max_ticks.store(max, Ordering::SeqCst);
        }

        loop {
            // Check if we've exceeded max ticks
            let current = self.current_tick.load(Ordering::SeqCst);
            let max = self.max_ticks.load(Ordering::SeqCst);
            if current >= max {
                break;
            }

            // Apply aging before picking task
            let aging_logs = self.inner.apply_aging(max_ticks);
            for log in aging_logs {
                println!("{}", log);
            }

            // Check for available tasks
            if let Some(task) = self.inner.pop_next() {
                self.current_tick.fetch_add(1, Ordering::SeqCst);

                let priority = task.get_priority();
                let base_priority = task.get_base_priority();
                let was_aged = priority != base_priority;

                // Create waker for this task
                let waker = self.create_waker_for_task(task.clone());
                let mut cx = Context::from_waker(&waker);

                // Poll the task
                let mut future_guard = task.future.lock().unwrap();
                if let Some(fut) = future_guard.as_mut() {
                    match fut.as_mut().poll(&mut cx) {
                        Poll::Ready(_) => {
                            drop(future_guard);
                            // Task completed, decrement counter
                            self.inner.decrement_remaining();

                            // Restore base priority if it was aged
                            if was_aged {
                                task.set_priority(base_priority);
                                println!("restore: task-{} {} -> base {}", task.id, priority.as_str(), base_priority.as_str());
                            }
                        }
                        Poll::Pending => {
                            drop(future_guard);
                            // Task is pending, will be re-enqueued when waker is called

                            // Reset wait_ticks after poll
                            task.reset_wait_ticks();

                            // Restore base priority if it was aged
                            if was_aged {
                                task.set_priority(base_priority);
                                println!("restore: task-{} {} -> base {}", task.id, priority.as_str(), base_priority.as_str());
                            }
                        }
                    }
                }

                continue;
            }

            // No tasks available
            let remaining = self.inner.get_remaining();
            if remaining == 0 {
                break;
            }

            self.inner.park();
        }
    }

    fn create_waker_for_task(&self, task: Arc<ExecTask>) -> Waker {
        let task_waker = Arc::new(TaskWaker {
            task: task.clone(),
            executor: self.inner.clone(),
        });

        waker_ref(&task_waker).clone()
    }
}

// ============================= PARKER ========================================
#[derive(Default)]
pub struct Parker(Mutex<bool>, Condvar);

impl Parker {
    pub fn park(&self) {
        let mut resumable = self.0.lock().unwrap();
        while !*resumable {
            resumable = self.1.wait(resumable).unwrap();
        }
        *resumable = false;
    }

    pub fn unpark(&self) {
        let mut resumable = self.0.lock().unwrap();
        *resumable = true;
        self.1.notify_one();
    }
}

// ============================= REACTOR =======================================
pub enum TaskState {
    Ready,
    NotReady(Waker),
    Finished,
}

pub struct Reactor {
    pub dispatcher: std::sync::mpsc::Sender<Event>,
    pub handle: Option<JoinHandle<()>>,
    pub tasks: HashMap<usize, TaskState>,
}

#[derive(Debug)]
pub enum Event {
    Close,
    Timeout(u64, usize),
}

impl Reactor {
    pub fn new() -> Arc<Mutex<Box<Self>>> {
        let (tx, rx) = channel::<Event>();
        let reactor = Arc::new(Mutex::new(Box::new(Reactor {
            dispatcher: tx,
            handle: None,
            tasks: HashMap::new(),
        })));

        let reactor_clone = Arc::downgrade(&reactor);
        let handle = thread::spawn(move || {
            let mut handles = vec![];
            for event in rx {
                let reactor = reactor_clone.clone();
                match event {
                    Event::Close => {
                        break;
                    }
                    Event::Timeout(duration, id) => {
                        let event_handle = thread::spawn(move || {
                            thread::sleep(Duration::from_secs(duration));
                            let reactor = reactor.upgrade().unwrap();
                            reactor.lock().map(|mut r| r.wake(id)).unwrap();
                        });
                        handles.push(event_handle);
                    }
                }
            }
            handles.into_iter().for_each(|handle| handle.join().unwrap());
        });
        reactor.lock().map(|mut r| r.handle = Some(handle)).unwrap();
        reactor
    }

    pub fn wake(&mut self, id: usize) {
        let state = self.tasks.get_mut(&id).unwrap();
        match std::mem::replace(state, TaskState::Ready) {
            TaskState::NotReady(waker) => {
                waker.wake();
            }
            TaskState::Finished => panic!("Called 'wake' twice on task: {}", id),
            _ => {
                unreachable!()
            }
        }
    }

    pub fn register(&mut self, duration: u64, waker: Waker, id: usize) {
        if self.tasks.insert(id, TaskState::NotReady(waker)).is_some() {
            panic!("Tried to insert a task with id: '{}', twice!", id);
        }
        self.dispatcher.send(Event::Timeout(duration, id)).unwrap();
    }

    pub fn close(&mut self) {
        self.dispatcher.send(Event::Close).unwrap();
    }

    pub fn is_ready(&self, id: usize) -> bool {
        self.tasks.get(&id).map(|state| match state {
            TaskState::Ready => true,
            _ => false,
        }).unwrap_or(false)
    }
}

impl Drop for Reactor {
    fn drop(&mut self) {
        self.handle.take().map(|h| h.join().unwrap()).unwrap();
    }
}

// ============================= TASK FUTURE ==================================
#[derive(Clone)]
pub struct Task {
    pub id: usize,
    pub reactor: Arc<Mutex<Box<Reactor>>>,
    pub data: u64,
}

impl Task {
    pub fn new(reactor: Arc<Mutex<Box<Reactor>>>, data: u64, id: usize) -> Self {
        Task { id, reactor, data }
    }
}

impl Future for Task {
    type Output = usize;
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut r = self.reactor.lock().unwrap();

        if r.is_ready(self.id) {
            *r.tasks.get_mut(&self.id).unwrap() = TaskState::Finished;
            Poll::Ready(self.id)
        } else if r.tasks.contains_key(&self.id) {
            r.tasks.insert(self.id, TaskState::NotReady(cx.waker().clone()));
            Poll::Pending
        } else {
            r.register(self.data, cx.waker().clone(), self.id);
            Poll::Pending
        }
    }
}

// ============================= TEST HELPERS =================================
pub mod test_helpers {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    static EXECUTION_ORDER: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static INITIALIZED: AtomicBool = AtomicBool::new(false);

    pub fn ensure_initialized() {
        if !INITIALIZED.load(Ordering::SeqCst) {
            clear_execution_order();
            INITIALIZED.store(true, Ordering::SeqCst);
        }
    }

    pub fn record_execution(name: &str) {
        ensure_initialized();
        let mut order = EXECUTION_ORDER.lock().unwrap();
        order.push(name.to_string());
    }

    pub fn get_execution_order() -> Vec<String> {
        let order = EXECUTION_ORDER.lock().unwrap();
        order.clone()
    }

    pub fn clear_execution_order() {
        let mut order = EXECUTION_ORDER.lock().unwrap();
        order.clear();
        INITIALIZED.store(true, Ordering::SeqCst);
    }

    /// A future that records its execution and is immediately ready
    pub struct ReadyFuture {
        pub name: String,
    }

    impl Future for ReadyFuture {
        type Output = ();
        fn poll(self: std::pin::Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            record_execution(&self.name);
            Poll::Ready(())
        }
    }

    /// A future that yields once on first poll, then completes
    pub struct YieldOnce {
        pub name: String,
        yielded: Arc<Mutex<bool>>,
        waker_storage: Arc<Mutex<Option<Waker>>>,
    }

    impl YieldOnce {
        pub fn new(name: String, waker_storage: Arc<Mutex<Option<Waker>>>) -> Self {
            YieldOnce {
                name,
                yielded: Arc::new(Mutex::new(false)),
                waker_storage,
            }
        }
    }

    impl Future for YieldOnce {
        type Output = ();
        fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut yielded = self.yielded.lock().unwrap();
            if !*yielded {
                *yielded = true;
                // Store the waker for later wake
                let mut storage = self.waker_storage.lock().unwrap();
                *storage = Some(cx.waker().clone());
                drop(storage);
                drop(yielded);
                Poll::Pending
            } else {
                record_execution(&self.name);
                Poll::Ready(())
            }
        }
    }

    /// A future that can be manually woken
    pub struct ManualWakeFuture {
        pub name: String,
        waker_storage: Arc<Mutex<Option<Waker>>>,
        completed: Arc<Mutex<bool>>,
    }

    impl ManualWakeFuture {
        pub fn new(name: String, waker_storage: Arc<Mutex<Option<Waker>>>) -> Self {
            ManualWakeFuture {
                name,
                waker_storage,
                completed: Arc::new(Mutex::new(false)),
            }
        }
    }

    impl Future for ManualWakeFuture {
        type Output = ();
        fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let completed = self.completed.lock().unwrap();
            if *completed {
                record_execution(&self.name);
                Poll::Ready(())
            } else {
                // Store the waker for later manual wake
                let mut storage = self.waker_storage.lock().unwrap();
                *storage = Some(cx.waker().clone());
                drop(storage);
                drop(completed);
                Poll::Pending
            }
        }
    }

    impl ManualWakeFuture {
        pub fn complete(&self) {
            let mut completed = self.completed.lock().unwrap();
            *completed = true;
        }

        pub fn wake(&self) {
            let storage = self.waker_storage.lock().unwrap();
            if let Some(waker) = storage.as_ref() {
                waker.wake_by_ref();
            }
        }
    }
}
