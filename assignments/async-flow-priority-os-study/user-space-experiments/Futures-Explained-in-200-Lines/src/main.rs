use std::{
    future::Future,
    sync::atomic::{AtomicUsize, Ordering},
    sync::{mpsc::{channel, Sender}, Arc, Mutex, Condvar},
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
    pin::Pin,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
    collections::HashMap,
    mem
};

// ============================= TRACE MACRO ====================================
static TRACE_COUNTER: AtomicUsize = AtomicUsize::new(0);

macro_rules! trace {
    ($($arg:tt)*) => {
        let id = TRACE_COUNTER.fetch_add(1, Ordering::SeqCst);
        println!("[{:04}] {}", id, format!($($arg)*));
    };
}

// ============================= MAIN ====================================
fn main() {
    let start = Instant::now();
    trace!("main: enter");

    let reactor = Reactor::new();

    let fut1 = async {
        trace!("fut1: begin");
        let val = Task::new(reactor.clone(), 1, 1).await;
        trace!("fut1: resumed with value={}", val);
        println!("Got {} at time: {:.2}.", val, start.elapsed().as_secs_f32());
    };

    let fut2 = async {
        trace!("fut2: begin");
        let val = Task::new(reactor.clone(), 2, 2).await;
        trace!("fut2: resumed with value={}", val);
        println!("Got {} at time: {:.2}.", val, start.elapsed().as_secs_f32());
    };

    let mainfut = async {
        trace!("mainfut: begin");
        trace!("mainfut: await fut1");
        fut1.await;
        trace!("mainfut: await fut2");
        fut2.await;
        trace!("mainfut: done");
    };

    block_on(mainfut);
    reactor.lock().map(|mut r| r.close()).unwrap();
    trace!("main: exit");
}

// ============================= EXECUTOR ====================================
#[derive(Default)]
struct Parker(Mutex<bool>, Condvar);

impl Parker {
    fn park(&self) {
        trace!("Parker::park: enter");
        let mut resumable = self.0.lock().unwrap();
        trace!("Parker::park: waiting on condvar, resumable={}", *resumable);
        while !*resumable {
            resumable = self.1.wait(resumable).unwrap();
        }
        trace!("Parker::park: woke up, resumable={}", *resumable);
        *resumable = false;
    }

    fn unpark(&self) {
        trace!("Parker::unpark: enter");
        let mut resumable = self.0.lock().unwrap();
        *resumable = true;
        trace!("Parker::unpark: calling notify_one");
        self.1.notify_one();
    }
}

fn block_on<F: Future>(mut future: F) -> F::Output {
    trace!("executor: block_on begin");
    let parker = Arc::new(Parker::default());
    trace!("executor: created Parker");

    let mywaker = Arc::new(MyWaker { parker: parker.clone() });
    trace!("executor: created MyWaker");

    let waker = mywaker_into_waker(Arc::into_raw(mywaker));
    trace!("executor: created Waker from MyWaker");

    let mut cx = Context::from_waker(&waker);
    trace!("executor: created Context from Waker");

    // SAFETY: we shadow `future` so it can't be accessed again.
    let mut future = unsafe { Pin::new_unchecked(&mut future) };

    loop {
        trace!("executor: about to poll future");
        match Future::poll(future.as_mut(), &mut cx) {
            Poll::Ready(val) => {
                trace!("executor: poll returned Ready");
                break val;
            }
            Poll::Pending => {
                trace!("executor: poll returned Pending, preparing to park");
                parker.park();
                trace!("executor: resumed from park");
            }
        };
    }
}

// ====================== FUTURE IMPLEMENTATION ==============================
#[derive(Clone)]
struct MyWaker {
    parker: Arc<Parker>,
}

#[derive(Clone)]
pub struct Task {
    id: usize,
    reactor: Arc<Mutex<Box<Reactor>>>,
    data: u64,
}

fn mywaker_wake(s: &MyWaker) {
    trace!("mywaker_wake: called");
    // We must reconstruct the Arc to properly manage reference counting
    let waker_arc = unsafe { Arc::from_raw(s) };
    waker_arc.parker.unpark();
    // Note: we don't call forget here because Arc::from_raw takes ownership
}

fn mywaker_clone(s: &MyWaker) -> RawWaker {
    trace!("mywaker_clone: called");
    let arc = unsafe { Arc::from_raw(s) };
    // Clone the Arc to increase ref count, then forget one to balance
    let cloned = arc.clone();
    std::mem::forget(arc); // increase ref count (don't decrement)
    RawWaker::new(Arc::into_raw(cloned) as *const (), &VTABLE)
}

const VTABLE: RawWakerVTable = unsafe {
    RawWakerVTable::new(
        |s| mywaker_clone(&*(s as *const MyWaker)),   // clone
        |s| mywaker_wake(&*(s as *const MyWaker)),    // wake
        |s| mywaker_wake(*(s as *const &MyWaker)),    // wake by ref
        |s| {
            trace!("RawWaker: drop called");
            drop(Arc::from_raw(s as *const MyWaker)); // decrease refcount
        },
    )
};

fn mywaker_into_waker(s: *const MyWaker) -> Waker {
    trace!("mywaker_into_waker: called, creating RawWaker");
    let raw_waker = RawWaker::new(s as *const (), &VTABLE);
    unsafe { Waker::from_raw(raw_waker) }
}

impl Task {
    fn new(reactor: Arc<Mutex<Box<Reactor>>>, data: u64, id: usize) -> Self {
        Task { id, reactor, data }
    }
}

impl Future for Task {
    type Output = usize;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        trace!("task-{}: poll enter", self.id);
        let mut r = self.reactor.lock().unwrap();

        if r.is_ready(self.id) {
            trace!("task-{}: reactor.is_ready({}) == true", self.id, self.id);
            *r.tasks.get_mut(&self.id).unwrap() = TaskState::Finished;
            trace!("task-{}: state marked as Finished", self.id);
            Poll::Ready(self.id)
        } else if r.tasks.contains_key(&self.id) {
            trace!("task-{}: already registered, updating waker", self.id);
            r.tasks.insert(self.id, TaskState::NotReady(cx.waker().clone()));
            trace!("task-{}: returning Poll::Pending", self.id);
            Poll::Pending
        } else {
            trace!("task-{}: first registration to reactor, duration={}s", self.id, self.data);
            r.register(self.data, cx.waker().clone(), self.id);
            trace!("task-{}: returning Poll::Pending", self.id);
            Poll::Pending
        }
    }
}

// =============================== REACTOR ===================================
enum TaskState {
    Ready,
    NotReady(Waker),
    Finished,
}

struct Reactor {
    dispatcher: Sender<Event>,
    handle: Option<JoinHandle<()>>,
    tasks: HashMap<usize, TaskState>,
}

#[derive(Debug)]
enum Event {
    Close,
    Timeout(u64, usize),
}

impl Reactor {
    fn new() -> Arc<Mutex<Box<Self>>> {
        trace!("Reactor::new: enter");
        let (tx, rx) = channel::<Event>();
        let reactor = Arc::new(Mutex::new(Box::new(Reactor {
            dispatcher: tx,
            handle: None,
            tasks: HashMap::new(),
        })));

        let reactor_clone = Arc::downgrade(&reactor);
        let handle = thread::spawn(move || {
            trace!("reactor-thread: start");
            let mut handles = vec![];
            for event in rx {
                let reactor = reactor_clone.clone();
                match event {
                    Event::Close => {
                        trace!("reactor-thread: receive Close event");
                        break;
                    }
                    Event::Timeout(duration, id) => {
                        trace!("reactor-thread: receive Timeout event duration={}s id={}", duration, id);
                        let event_handle = thread::spawn(move || {
                            trace!("timer-thread-{}: start, sleeping for {}s", id, duration);
                            thread::sleep(Duration::from_secs(duration));
                            trace!("timer-thread-{}: sleep end", id);
                            let reactor = reactor.upgrade().unwrap();
                            reactor.lock().map(|mut r| r.wake(id)).unwrap();
                        });
                        handles.push(event_handle);
                    }
                }
            }
            trace!("reactor-thread: joining timer threads");
            handles.into_iter().for_each(|handle| handle.join().unwrap());
            trace!("reactor-thread: exit");
        });
        reactor.lock().map(|mut r| r.handle = Some(handle)).unwrap();
        trace!("Reactor::new: complete, returning Arc");
        reactor
    }

    fn wake(&mut self, id: usize) {
        trace!("reactor: wake({}) called", id);
        let state = self.tasks.get_mut(&id).unwrap();
        match mem::replace(state, TaskState::Ready) {
            TaskState::NotReady(waker) => {
                trace!("reactor: task-{} state NotReady -> Ready", id);
                trace!("reactor: calling waker.wake() for task-{}", id);
                waker.wake();
            }
            TaskState::Finished => panic!("Called 'wake' twice on task: {}", id),
            _ => {
                trace!("reactor: task-{} unexpected state in wake", id);
                unreachable!()
            }
        }
    }

    fn register(&mut self, duration: u64, waker: Waker, id: usize) {
        trace!("reactor: register task-{} duration={}s", id, duration);
        if self.tasks.insert(id, TaskState::NotReady(waker)).is_some() {
            panic!("Tried to insert a task with id: '{}', twice!", id);
        }
        trace!("reactor: sending Timeout event duration={}s id={}", duration, id);
        self.dispatcher.send(Event::Timeout(duration, id)).unwrap();
    }

    fn close(&mut self) {
        trace!("reactor: close called, sending Close event");
        self.dispatcher.send(Event::Close).unwrap();
    }

    fn is_ready(&self, id: usize) -> bool {
        self.tasks.get(&id).map(|state| match state {
            TaskState::Ready => true,
            _ => false,
        }).unwrap_or(false)
    }
}

impl Drop for Reactor {
    fn drop(&mut self) {
        trace!("Reactor::drop: enter, joining reactor thread");
        self.handle.take().map(|h| h.join().unwrap()).unwrap();
        trace!("Reactor::drop: complete");
    }
}
