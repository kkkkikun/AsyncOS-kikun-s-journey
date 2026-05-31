use std::ptr;

const DEFAULT_STACK_SIZE: usize = 1024 * 4;
const MAX_THREADS: usize = 4;
static mut RUNTIME: *mut Runtime = ptr::NonNull::dangling().as_ptr();

#[derive(Debug)]
pub struct Runtime {
    threads: Vec<Thread>,
    current: usize,
}

#[derive(PartialEq, Eq, Debug)]
enum State {
    Uninitialized,
    Running,
    Ready,
}

struct Thread {
    id: usize,
    stack: Vec<u8>,
    ctx: ThreadContext,
    state: State,
    task: Option<Box<dyn Fn()>>,
}

impl Thread {
    fn max_usage_on_stack(&self) -> usize {
        self.stack.len() - self.stack.iter().filter(|x| **x == 0).count()
    }
}

impl std::fmt::Debug for Thread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        //write!(f, "Thread {{ id: {}, state: {:?} }}", self.id, self.state)
        f.debug_struct("Thread")
            .field("id", &self.id)
            .field("ctx", &self.ctx)
            .field("state", &self.state)
            .finish()
    }
}

#[derive(Debug, Default)]
#[repr(C)]
struct ThreadContext {
    rsp: u64,
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,
    thread_ptr: u64,
}

impl Thread {
    fn new(id: usize) -> Self {
        Thread {
            id,
            stack: vec![0_u8; DEFAULT_STACK_SIZE],
            ctx: ThreadContext::default(),
            state: State::Uninitialized,
            task: None,
        }
    }
}

impl Runtime {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let main_thread = Thread {
            id: 0, // main thread
            stack: vec![0_u8; DEFAULT_STACK_SIZE],
            ctx: ThreadContext::default(),
            state: State::Running, // main thread is either Runing or Ready
            task: None,
        };

        let mut threads = vec![main_thread];
        threads[0].ctx.thread_ptr = &threads[0] as *const Thread as u64;
        threads.extend((1..MAX_THREADS).map(Thread::new));

        Runtime {
            threads,
            current: 0,
        }
    }

    pub fn init(&mut self) {
        unsafe { RUNTIME = self };
    }

    pub fn run(&mut self) -> ! {
        while self.t_yield() {}
        println!("exit from main thread~");
        std::process::exit(0);
    }

    fn t_return(&mut self) {
        // for non-0 thread: i.e. non-main thread
        if self.current != 0 {
            // a thread is finished, so set it to Uninitialized
            self.threads[self.current].state = State::Uninitialized;
            self.t_yield();
        }
    }

    fn t_yield(&mut self) -> bool {
        let mut pos = self.current;
        //dbg!(&self);
        println!("[id{pos}] yield_start");

        // find the Ready thread: kind of seaching in ring buffer
        while self.threads[pos].state != State::Ready {
            pos += 1;
            if pos == self.threads.len() {
                // when no ready in thread pos..len, set to 0, and
                pos = 0;
            }
            if pos == self.current {
                // All threads are not ready, then return to exit in run method.
                // In practice, this means all threads are back to Uninitialized except main thread.
                return false;
            }
        }

        // update the state from Running to Ready for current thread
        if self.threads[self.current].state != State::Uninitialized {
            self.threads[self.current].state = State::Ready;
        }

        // update the state from Ready to Runing for chosen thread
        self.threads[pos].state = State::Running;
        let old_pos = self.current;
        // set chosen thread as current thread
        self.current = pos;

        println!("[old=id{old_pos} => new=id{pos}] thread switch");
        unsafe {
            // exchange thread stacks:
            // * save current registry status to previous thread context
            // * restore register status from current thread context
            //   * most important: current stack becomes
            //
            //    low addr ┌─────────────┐◄── sp (should always align to 16B)
            //             │__call fn ptr│callback (i.e. user closure passed to spawn)
            //             ├─────────────┤
            //             │ guard fn ptr│will be run when the thread is finished
            //   high addr └─────────────┘◄── base
            //
            __switch(&mut self.threads[old_pos].ctx, &self.threads[pos].ctx);
        }
        println!("[id{old_pos}] yield end");
        true
    }

    pub fn spawn<F: Fn() + 'static>(f: F) {
        unsafe {
            let available = (*RUNTIME)
                .threads
                .iter_mut()
                .find(|t| t.state == State::Uninitialized)
                .expect("no available thread.");

            let size = available.stack.len();
            // align to 16 bytes: s_ptr now becomes a base pointer to the stack
            let s_ptr = available.stack.as_mut_ptr().add(size & !0xf);
            ptr::write_unaligned(s_ptr.sub(16).cast::<u64>(), guard as usize as u64);
            ptr::write_unaligned(s_ptr.sub(32).cast::<u64>(), __call as usize as u64);
            available.ctx.rsp = s_ptr.sub(32) as u64; // set the top of thread stack

            available.task = Some(Box::new(f));
            available.ctx.thread_ptr = available as *const Thread as u64;
            available.state = State::Ready;
        }
    }
}

#[no_mangle]
fn call_entry(thread: u64) {
    let thread = unsafe { &*(thread as *const Thread) };
    if let Some(f) = &thread.task {
        println!(
            "\u{1b}[1;34m[id{} before running callback] max stack size: {}\u{1b}[0m",
            thread.id,
            thread.max_usage_on_stack()
        );
        f();
        println!(
            "\u{1b}[1;34m[id{} after running callback] max stack size: {}\u{1b}[0m",
            thread.id,
            thread.max_usage_on_stack()
        );
    }
}

std::arch::global_asm!(
    ".globl __call",
    "__call:",
    "  sub   rsp, 8", // 栈地址对齐
    "  call  call_entry",
    "  add   rsp, 16", // 这里要再加一个8，这样retq使用的就是栈中的那个guard
    "  ret",
);

fn guard() {
    let rt = unsafe { &mut *RUNTIME };
    let current = &rt.threads[rt.current];
    println!(
        "\u{1b}[1;31mTHREAD {} FINISHED. Stack size: {}\u{1b}[0m",
        current.id,
        current.max_usage_on_stack()
    );
    rt.t_return();
}

pub fn yield_thread() {
    unsafe { (*RUNTIME).t_yield() };
}

std::arch::global_asm!(
    r#"
.globl __switch
__switch:
  mov  [rdi+0x00], rsp
  mov  [rdi+0x08], r15
  mov  [rdi+0x10], r14
  mov  [rdi+0x18], r13
  mov  [rdi+0x20], r12
  mov  [rdi+0x28], rbx
  mov  [rdi+0x30], rbp

  mov  rsp, 0x00[rsi]
  mov  r15, 0x08[rsi]
  mov  r14, 0x10[rsi]
  mov  r13, 0x18[rsi]
  mov  r12, 0x20[rsi]
  mov  rbx, 0x28[rsi]
  mov  rbp, 0x30[rsi]
  mov  rdi, 0x38[rsi]
  ret
"#
);

extern "C" {
    fn __switch(old: *mut ThreadContext, new: *const ThreadContext);
    fn __call(thread: u64);
}

fn info(s: &str) {
    println!("\u{1b}[1;43;30m{s}\u{1b}[0m"); // print in color in debug build
}

#[cfg(not(windows))]
fn main() {
    let mut runtime = Runtime::new();
    runtime.init();
    Runtime::spawn(|| {
        info("[id1] I haven't implemented a timer in this example.");
        yield_thread();
        info("[id1 yieled] Finally, notice how the tasks are executed concurrently.");
    });
    Runtime::spawn(|| {
        info("[id2] But we can still nest tasks...");
        Runtime::spawn(|| {
            info("[id3] ...like this!");
        })
    });
    runtime.run();
}
