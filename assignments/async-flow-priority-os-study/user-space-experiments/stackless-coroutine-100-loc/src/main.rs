use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

// ============================= Trace ====================================
static TRACE_COUNTER: AtomicUsize = AtomicUsize::new(1);

macro_rules! trace {
    ($($arg:tt)*) => {
        let seq = TRACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        println!("[{:04}] {}", seq, format!($($arg)*));
    };
}

// ============================= Yielder ====================================
#[derive(Debug, Clone, Copy)]
enum State {
    Halted,
    Running,
}

struct Fib {
    id: usize,
    state: State,
}

impl Fib {
    fn waiter<'a>(&'a mut self) -> Waiter<'a> {
        Waiter { fib: self }
    }
}

struct Waiter<'a> {
    fib: &'a mut Fib,
}

impl<'a> Future for Waiter<'a> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Self::Output> {
        match self.fib.state {
            State::Halted => {
                trace!("task-{} state: Halted -> Running, result=Ready", self.fib.id);
                self.fib.state = State::Running;
                Poll::Ready(())
            }
            State::Running => {
                trace!("task-{} state: Running -> Halted, result=Pending", self.fib.id);
                self.fib.state = State::Halted;
                Poll::Pending
            }
        }
    }
}

// ============================= Executor ====================================
struct Executor {
    fibs: VecDeque<(usize, Pin<Box<dyn Future<Output = ()>>>)>,
}

impl Executor {
    fn new() -> Self {
        Executor {
            fibs: VecDeque::new(),
        }
    }

    fn push<C, F>(&mut self, task_id: usize, closure: C)
    where
        F: Future<Output = ()> + 'static,
        C: FnOnce(Fib) -> F,
    {
        let fib = Fib {
            id: task_id,
            state: State::Running,
        };
        self.fibs.push_back((task_id, Box::pin(closure(fib))));
        trace!("push task-{}", task_id);
    }

    fn run(&mut self) {
        trace!("run begin, queue_len={}", self.fibs.len());
        let waker = waker::create();
        let mut context = Context::from_waker(&waker);

        while let Some((task_id, mut fib)) = self.fibs.pop_front() {
            trace!("pop task-{}", task_id);
            trace!("poll task-{}", task_id);
            match fib.as_mut().poll(&mut context) {
                Poll::Pending => {
                    trace!("task-{} Pending -> push_back", task_id);
                    self.fibs.push_back((task_id, fib));
                }
                Poll::Ready(()) => {
                    trace!("task-{} Ready -> finished", task_id);
                }
            }
        }
        trace!("run end");
    }
}

// ============================= Null Waker ====================================
mod waker {
    use super::*;

    pub fn create() -> Waker {
        unsafe { Waker::from_raw(RAW_WAKER) }
    }

    const RAW_WAKER: RawWaker = RawWaker::new(std::ptr::null(), &VTABLE);
    const VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

    unsafe fn clone(_: *const ()) -> RawWaker { RAW_WAKER }
    unsafe fn wake(_: *const ()) {}
    unsafe fn wake_by_ref(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}
}

// ============================= Giving it a go ====================================
pub fn main() {
    let mut exec = Executor::new();

    for instance in 1..=3 {
        exec.push(instance, move |mut fib| {
            let id = fib.id;
            async move {
                trace!("task-{} print A (before await 1)", id);
                println!("{} A", id);
                trace!("task-{} after print A, await waiter 1", id);
                fib.waiter().await;
                trace!("task-{} resumed after await 1, print B", id);
                println!("{} B", id);
                trace!("task-{} after print B, await waiter 2", id);
                fib.waiter().await;
                trace!("task-{} resumed after await 2, print C", id);
                println!("{} C", id);
                trace!("task-{} after print C, await waiter 3", id);
                fib.waiter().await;
                trace!("task-{} resumed after await 3, print D", id);
                println!("{} D", id);
                trace!("task-{} done", id);
            }
        });
    }

    println!("Running");
    exec.run();
    println!("Done");
}
