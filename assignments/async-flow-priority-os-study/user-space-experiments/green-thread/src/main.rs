#![feature(naked_functions)]

use std::arch::naked_asm;

// ===== Trace Logger Infrastructure =====
use std::{
    fs::OpenOptions,
    io::Write,
    sync::{Mutex, OnceLock},
};

struct TraceLogger {
    seq: usize,
    file: std::fs::File,
}

static TRACE_LOGGER: OnceLock<Mutex<TraceLogger>> = OnceLock::new();

fn trace_write(msg: String) {
    let logger = TRACE_LOGGER.get_or_init(|| {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("trace.log")
            .expect("failed to open trace.log");

        Mutex::new(TraceLogger { seq: 0, file })
    });

    let mut logger = logger.lock().unwrap();
    let line = format!("[{:04}] {}", logger.seq, msg);
    logger.seq += 1;

    println!("{}", line);
    writeln!(logger.file, "{}", line).unwrap();
}

macro_rules! trace {
    ($($arg:tt)*) => {
        crate::trace_write(format!($($arg)*))
    };
}

// ===== Green Thread Runtime =====

#[cfg_attr(win64, path = "win64.rs")]
#[cfg_attr(linux64, path = "linux64.rs")]
#[cfg_attr(rv64, path = "rv64.rs")]
mod os;
use os::ThreadContext;

const DEFAULT_STACK_SIZE: usize = 1024 * 1024 * 2;
const MAX_THREADS: usize = 4;
static mut RUNTIME: usize = 0;

pub struct Runtime {
    threads: Vec<Thread>,
    current: usize,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum State {
    Available,
    Running,
    Ready,
}

#[allow(dead_code)]
struct Thread {
    id: usize,
    stack: Vec<u8>,
    ctx: ThreadContext,
    state: State,
}

impl Thread {
    fn new(id: usize) -> Self {
        Thread {
            id,
            stack: vec![0_u8; DEFAULT_STACK_SIZE],
            ctx: ThreadContext::default(),
            state: State::Available,
        }
    }
}

impl Runtime {
    // A. Runtime::new
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        trace!("runtime: initialization started");

        let base_thread = Thread {
            id: 0,
            stack: vec![0_u8; DEFAULT_STACK_SIZE],
            ctx: ThreadContext::default(),
            state: State::Running,
        };
        trace!("runtime: created base thread id=0, state=Running");

        let mut threads = vec![base_thread];
        let mut available_threads: Vec<Thread> = (1..MAX_THREADS).map(Thread::new).collect();
        for t in &available_threads {
            trace!("runtime: created thread id={}, state=Available", t.id);
        }
        threads.append(&mut available_threads);

        trace!(
            "runtime: initialization complete, {} threads, stack_size={} bytes",
            threads.len(),
            DEFAULT_STACK_SIZE
        );

        Runtime {
            threads,
            current: 0,
        }
    }

    // B. Runtime::init
    pub fn init(&self) {
        unsafe {
            let r_ptr: *const Runtime = self;
            RUNTIME = r_ptr as usize;
        }
        trace!(
            "runtime: set global RUNTIME pointer, addr={:#x}",
            unsafe { RUNTIME }
        );
    }

    // D. Runtime::run
    pub fn run(&mut self) -> ! {
        trace!("runtime: run begin, current thread={}", self.current);
        loop {
            trace!(
                "runtime: calling t_yield, current thread={}",
                self.current
            );
            let yielded = self.t_yield();
            trace!(
                "runtime: t_yield returned {}, current thread={}",
                yielded,
                self.current
            );
            if !yielded {
                break;
            }
        }
        trace!("runtime: no ready threads, run end");
        trace!("runtime: process exit");
        std::process::exit(0);
    }

    // F. Runtime::t_return
    fn t_return(&mut self) {
        let current = self.current;
        trace!("runtime: t_return enter, current thread={}", current);

        if current != 0 {
            trace!("runtime: thread-{} Running -> Available", current);
            self.threads[current].state = State::Available;
            self.t_yield();
        }

        trace!("runtime: t_return exit, current thread={}", self.current);
    }

    // E. Runtime::t_yield
    #[inline(never)]
    fn t_yield(&mut self) -> bool {
        let current = self.current;
        trace!("runtime: t_yield enter, current thread={}", current);

        let mut pos = current;
        trace!(
            "runtime: scanning for ready threads starting from current={}",
            pos
        );

        while self.threads[pos].state != State::Ready {
            pos += 1;
            if pos == self.threads.len() {
                pos = 0;
            }
            trace!(
                "runtime: checking thread-{} state={:?}",
                pos,
                self.threads[pos].state
            );
            if pos == current {
                trace!("runtime: no ready thread found, returning false");
                return false;
            }
        }

        trace!("runtime: selected thread-{} (Ready)", pos);

        if self.threads[self.current].state != State::Available {
            trace!(
                "runtime: thread-{} {:?} -> Ready",
                self.current,
                self.threads[self.current].state
            );
            self.threads[self.current].state = State::Ready;
        }

        trace!("runtime: thread-{} Ready -> Running", pos);
        self.threads[pos].state = State::Running;

        let old_pos = self.current;
        let new_pos = pos;
        trace!("runtime: old_pos={} new_pos={}", old_pos, new_pos);
        self.current = pos;

        trace!(
            "runtime: context switch old={} new={}",
            old_pos,
            new_pos
        );

        unsafe {
            let old: *mut ThreadContext = &mut self.threads[old_pos].ctx;
            let new: *const ThreadContext = &self.threads[pos].ctx;
            os::switch(old, new);
        }

        trace!(
            "runtime: context switch returned, current={}",
            self.current
        );

        // preventing compiler optimizing our code away on windows. Will never be reached anyway.
        !self.threads.is_empty()
    }
}

// J. skip — naked function, no instrumentation
#[unsafe(naked)]
#[allow(unused)]
unsafe extern "C" fn skip() {
    naked_asm!("ret")
}

// H. guard
fn guard() {
    trace!("runtime: guard enter, thread finished execution");
    unsafe {
        let rt_ptr = RUNTIME as *mut Runtime;
        (*rt_ptr).t_return();
    };
    trace!("runtime: guard exit (unreachable if t_return switches away)");
}

// G. yield_thread
pub fn yield_thread() {
    let current = unsafe {
        let rt_ptr = RUNTIME as *const Runtime;
        (*rt_ptr).current
    };
    trace!("yield_thread: called by thread-{}", current);

    unsafe {
        let rt_ptr = RUNTIME as *mut Runtime;
        (*rt_ptr).t_yield();
    };

    let current_after = unsafe {
        let rt_ptr = RUNTIME as *const Runtime;
        (*rt_ptr).current
    };
    trace!("yield_thread: returned, now running thread-{}", current_after);
}

// I. User task closures
pub fn main() {
    let mut runtime = Runtime::new();
    runtime.init();

    runtime.spawn(|| {
        trace!("thread-1: task begin");
        for i in 0..3 {
            trace!("thread-1: before yield, counter={}", i);
            yield_thread();
            trace!("thread-1: after yield, counter={}", i);
        }
        trace!("thread-1: task end");
    });

    runtime.spawn(|| {
        trace!("thread-2: task begin");
        for i in 0..3 {
            trace!("thread-2: before yield, counter={}", i);
            yield_thread();
            trace!("thread-2: after yield, counter={}", i);
        }
        trace!("thread-2: task end");
    });

    runtime.run();
}
