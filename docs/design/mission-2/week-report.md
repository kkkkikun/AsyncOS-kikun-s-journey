# **第 2 周周报：用户态线程、协程与优先级调度机制研究**

## **1. 本周目标**

本周围绕用户态线程、Rust Future、无栈协程和有栈绿色线程展开学习，重点分析不同执行流机制在创建、调度、挂起、恢复和结束过程中的状态变迁方式。在完成多组动态跟踪实验后，我进一步选择在一个简化的 Rust Future Executor 中扩展优先级调度支持，并通过测试验证优先级机制、异步唤醒路径和 aging 防饥饿机制的正确性。

本周工作主要包括四个部分。第一，学习 Tokio Future、100 行无栈协程、200 行 Future executor 和 200 行绿色线程的核心机制；第二，对这些示例进行动态跟踪，记录执行流状态迁移；第三，在 Future Executor 上实现优先级调度和 aging 机制；第四，将用户态异步执行流模型与 ArceOS / StarryOS 中 iozone 并发测试暴露出的 I/O 饥饿问题联系起来，探索优先级感知 WaitQueue 或 block I/O 唤醒路径的后续改进方向。

## **2. 学习内容概览**

本周主要学习和分析了四篇材料：Tokio Future、100 行无栈协程、200 行 Future executor，以及 200 行绿色线程。它们分别代表了 Rust 异步执行流的不同层次。

Tokio Future 展示的是 Rust 异步编程的工业级抽象基础。Future 表示一个尚未完成的异步计算，Executor 通过 `poll()` 驱动 Future 前进。当 Future 暂时无法继续执行时，会返回 `Poll::Pending`，并通过 `Context` 中的 `Waker` 将恢复执行的能力交给运行时。等底层事件完成后，运行时调用 `Waker::wake()`，使该任务重新进入可调度状态。这个机制说明，Future 不会自己“后台运行”，真正推动 Future 前进的动作仍然是 Executor 后续的 `poll()`。

100 行 stack-less coroutine 示例展示的是一个极简无栈协程模型。它依靠 Rust 编译器生成的 `async/await` 状态机和一个 FIFO 队列，实现了多个任务之间的协作式轮转。该示例没有真实 Reactor，也没有有效的外部事件源；它使用 null waker，并在任务返回 `Poll::Pending` 后由 Executor 主动将任务重新放回队尾。因此，它更适合作为理解 `async/await` 状态机和协作式调度的教学示例，而不是标准异步 I/O runtime 模板。

200 行 Future executor 示例则更接近现代 Rust async runtime 的核心闭环。它引入了 `Reactor`、`Waker` 和 `Parker`：Executor 首次 `poll` Future，Future 注册 Waker 并返回 `Pending`，Executor 挂起当前线程；Reactor 在外部事件完成后调用 `waker.wake()`，Waker 进一步调用 `Parker::unpark()` 唤醒 Executor；Executor 被唤醒后再次 `poll` Future，最终得到 `Poll::Ready`。这个示例完整展示了 `poll -> Pending -> register waker -> park -> event ready -> wake -> unpark -> poll again -> Ready` 的事件驱动闭环。

200 行绿色线程示例代表的是另一条执行流路线：有栈用户态线程。绿色线程由用户态 Runtime 管理，操作系统内核对其不可见。每个绿色线程拥有独立栈空间，Runtime 通过汇编 `switch` 保存和恢复寄存器、栈指针等上下文，实现用户态任务切换。与 Future 依赖编译器生成状态机不同，绿色线程的执行状态保存在独立栈和寄存器上下文中。它的优点是业务代码可以接近同步阻塞风格，缺点是需要维护独立栈、处理栈溢出风险，并封装可能阻塞的系统调用，实现复杂度和运行时负担都更高。

## **3. 几类执行流机制的核心对比**

本周的学习让我对有栈协程、无栈协程和 Future runtime 的差异有了更清晰的认识。它们都能实现“暂停与恢复”，但实现暂停的位置、保存状态的方式和恢复执行的触发机制并不相同。

| 机制 | 状态保存位置 | 恢复方式 | 典型特征 |
|---|---|---|---|
| 100 行无栈协程 | Future 状态机 | Executor 再次 `poll` | 协作式轮转 |
| 200 行 Future executor | Future 状态机 + Reactor 状态 | Waker 唤醒后再次 `poll` | 事件驱动 |
| 200 行绿色线程 | 独立栈 + 寄存器上下文 | `switch` 恢复上下文 | 有栈切换 |
| Priority Future Executor | Future 状态机 + 调度元数据 | priority-aware Waker 重新入队 | 优先级调度 |

无栈 Future 的状态保存在编译器生成的状态机中。遇到 `.await` 时，如果内部 Future 返回 `Pending`，当前 Future 会保存执行位置并退出 `poll()`。下次被 `poll()` 时，它会从上一次暂停的位置继续执行。也就是说，无栈 Future 的“暂停”本质上是函数返回，执行状态被结构体字段保存。

有栈绿色线程则不同。绿色线程遇到 `yield_thread()` 时，当前函数并没有通过普通意义上的 return 退出。Runtime 会通过汇编保存当前任务的栈指针和 callee-saved 寄存器，然后切换到调度器或另一个任务的栈。等该绿色线程再次被调度时，Runtime 恢复它的寄存器和栈指针，使它像普通函数调用返回一样从 `yield_thread()` 后继续执行。

这一区别可以概括为：

```text
无栈 Future：
  await 点返回 Pending
  当前调用栈退出
  状态保存在 Future 状态机字段中
  下次 poll 时由状态机恢复

有栈绿色线程：
  yield 时函数不返回
  Runtime 保存 rsp 和寄存器
  执行流切换到另一个栈
  下次 switch 回来后从原位置继续
```

因此，Future 更轻量、平台无关性更强，也更符合 Rust “零成本抽象”和系统编程语言的定位；绿色线程对业务代码侵入性更低，但需要独立栈、上下文切换汇编和更复杂的运行时支持。

## **4. 动态跟踪方法**

为了分析这些执行流的状态迁移，我在不同示例的关键位置加入了递增序号日志。日志记录的目标不是简单打印函数调用，而是还原任务从创建到结束的完整状态变化。

在 100 行无栈协程中，我主要跟踪 Executor 队列操作、任务被 `poll` 的时机、`Waiter::poll` 中 `Running` 与 `Halted` 的切换，以及业务代码中 `A`、`B`、`C`、`D` 输出点的交错顺序。该实验用于观察协作式轮转调度如何通过 `Pending -> push_back` 实现多个任务交错推进。

在 200 行 Future executor 中，我主要跟踪 `block_on`、`Task::poll`、`Reactor::register`、`Reactor::wake`、`Waker::wake`、`Parker::park` 和 `Parker::unpark`。该实验用于观察 `Future`、`Executor`、`Reactor` 和 `Waker` 之间的完整事件驱动闭环。

在 200 行绿色线程中，我主要跟踪 Runtime 初始化、`spawn`、线程状态切换、`yield_thread()`、`t_yield`、`switch` 调用前后、`guard` 和 `t_return`。由于 `switch` 函数内部涉及裸汇编、寄存器和栈指针操作，我没有在汇编内部插入日志，而是在 `switch` 调用前和恢复后记录事件，避免破坏上下文切换语义。

在 Priority Future Executor 中，我进一步扩展了 trace 字段，加入任务优先级、队列类型、等待 tick、调度次数、aging promote 和 priority restore 等信息。这样可以观察优先级信息是否贯穿 `spawn -> enqueue -> pick -> poll -> Pending -> wake -> re-enqueue -> poll` 的完整路径。

## **5. 100 行无栈协程执行流分析**

100 行 stack-less coroutine 示例的核心是一个极简 Executor 和一个自定义 `Waiter`。每个任务的结构大致如下：

```rust
println!("A");
fib.waiter().await;

println!("B");
fib.waiter().await;

println!("C");
fib.waiter().await;

println!("D");
```

每个 `waiter().await` 都是一个人为构造的协作式让出点。第一次被 `poll` 时，`Waiter` 会把内部状态从 `Running` 改为 `Halted`，并返回 `Poll::Pending`；Executor 收到 `Pending` 后，将当前任务放回队尾。第二次被 `poll` 时，`Waiter` 发现状态为 `Halted`，于是将其改回 `Running`，并返回 `Poll::Ready(())`，使外层 async block 从 await 后继续执行。

因此，每个 await 点都经历两次 poll：

```text
第一次 poll：Running -> Halted，返回 Pending，任务让出
第二次 poll：Halted -> Running，返回 Ready，任务恢复
```

动态跟踪中，三个任务最终的业务输出顺序为：

```text
1 A
2 A
3 A
1 B
2 B
3 B
1 C
2 C
3 C
1 D
2 D
3 D
```

这说明 Executor 使用了简单的 FIFO round-robin 策略。任务执行到 await 点后主动让出，Executor 将其放回队尾，然后继续调度下一个任务。

从状态迁移角度看，单个任务大致经历：

```text
Queued
  -> Running
  -> Pending
  -> Queued
  -> Running
  -> Ready at await point
  -> Continue
  -> Pending
  -> ...
  -> Finished
```

这个实验说明，Rust `async/await` 生成的无栈 Future 能够保存任务执行位置。任务返回 `Pending` 后不会丢失上下文，下一次被 `poll` 时可以从上一次暂停点继续。不过，该示例没有真实事件源和有效 Waker，任务恢复依赖 Executor 主动重新轮询，因此它更像是一个协作式 coroutine 教学模型，而不是完整异步 I/O runtime。

## **6. 200 行 Future Executor 执行流分析**

200 行 Future executor 示例展示了更接近现代 async runtime 的结构。它的核心闭环是：

```text
poll
  -> register waker
  -> Pending
  -> park
  -> reactor event
  -> wake
  -> unpark
  -> poll again
  -> Ready
```

第一次 `poll` 时，`mainfut` 开始执行，并进入 `fut1.await`。`task-1` 第一次被 `poll` 时发现自己尚未 ready，于是将当前 `Context` 中的 `Waker` 克隆并注册到 `Reactor`，然后返回 `Poll::Pending`。Executor 收到 `Pending` 后并不继续轮询，而是调用 `Parker::park()` 挂起当前线程。

当 timer 线程模拟的外部事件完成后，Reactor 调用 `wake(1)`，将 task 状态从 `NotReady(Waker)` 改为 `Ready`，并调用保存的 `waker.wake()`。Waker 内部进一步调用 `Parker::unpark()`，通过条件变量唤醒 Executor。Executor 恢复后再次 `poll mainfut`，此时 `task-1` 发现 Reactor 中对应状态已经是 `Ready`，于是返回 `Poll::Ready`，`fut1.await` 完成，async block 从 await 后继续执行。

这条路径可以表示为：

```text
Executor poll
  -> Task::poll
  -> Reactor::register
  -> Poll::Pending
  -> Executor park

Timer event complete
  -> Reactor::wake
  -> Waker::wake
  -> Parker::unpark
  -> Executor resumes

Executor poll again
  -> Task sees Ready
  -> Poll::Ready
  -> async block resumes
```

该实验还验证了一个重要结论：`fut1.await; fut2.await;` 是顺序等待，不是并发执行。虽然 `fut1` 和 `fut2` 可以在语法上提前构造，但 Future 是惰性的，只有被 `poll` 时才会真正推进。因此，`task-2` 并不是一开始就注册到 Reactor，而是在 `fut1` 完成后才被 `poll` 和注册。日志中的总耗时约为 3 秒，正好对应 task-1 的 1 秒等待加上 task-2 的 2 秒等待。如果二者是真正并发注册，总耗时应接近 2 秒。

该实验让我更清楚地理解了 Waker 的作用：Waker 不负责直接运行 Future，也不直接恢复 async block。它只是通知 Executor：某个任务已经可能可以继续执行了。真正推动 Future 状态机继续前进的动作仍然是 Executor 后续的 `poll()`。

## **7. 200 行绿色线程执行流分析**

绿色线程实验展示的是有栈执行流模型。Runtime 初始化时会创建一个线程表，其中 `thread-0` 是主线程，初始为 `Running`；其他线程槽位初始为 `Available`。调用 `spawn` 时，Runtime 会寻找一个 `Available` 槽位，为其分配独立栈，设置初始栈指针和任务入口，并将状态改为 `Ready`。

状态变化为：

```text
Available -> Ready
```

Runtime 进入调度循环后，会扫描线程表，选择下一个 `Ready` 线程运行。例如从 `thread-0` 切换到 `thread-1` 时，状态变化为：

```text
thread-0: Running -> Ready
thread-1: Ready -> Running
```

随后 Runtime 调用汇编 `switch(old, new)`，保存旧线程的上下文并恢复新线程上下文。`switch` 的核心步骤包括保存旧任务的 callee-saved 寄存器和栈指针，切换 CPU 的 `rsp` 到新任务栈，再从新任务栈中恢复寄存器，最后通过 `ret` 跳转到新任务的执行位置。

当绿色线程执行到 `yield_thread()` 时，它主动让出 CPU，Runtime 再次选择下一个 `Ready` 线程。动态跟踪显示，被切出的线程未来再次被调度时，会从 `yield_thread()` 返回后继续执行。这说明绿色线程恢复执行不依赖 `poll()`，而是依赖保存和恢复栈指针与寄存器上下文。

单个绿色线程的生命周期可以表示为：

```text
Available
  -> Ready
  -> Running
  -> Ready
  -> Running
  -> ...
  -> Running
  -> Available
```

其中，`Running -> Ready` 发生在任务主动调用 `yield_thread()` 时，`Running -> Available` 发生在任务函数返回后。任务返回后会进入预设的 `guard` 函数，`guard` 调用 `t_return` 将当前线程标记为 `Available`，使线程槽位可以被后续任务复用。

绿色线程的优点是业务代码侵入性低，可以接近同步函数方式编写；但代价是每个任务需要独立栈，存在栈内存浪费和栈溢出风险，并且上下文切换依赖平台相关汇编。对于 Rust 来说，无栈 Future 更符合零成本抽象、内存安全和无强制运行时的设计目标。

## **8. 优先级 Future Executor 实践**

在完成上述执行流动态跟踪后，我选择在简化 Future Executor 上扩展优先级调度支持，而不是直接修改绿色线程或 Tokio。原因是 Future Executor 的调度边界更清晰：任务从 ready queue 中被取出，经由 `poll()` 推进；如果返回 `Pending`，则等待 Waker 唤醒；如果返回 `Ready`，则任务完成。这条路径便于插桩、测试和验证。

原始 200 行 Future executor 中的 `block_on(mainfut)` 只能驱动一个顶层 Future。即使 `mainfut` 内部包含 `fut1.await; fut2.await;`，从 Executor 角度看，它仍然只是在反复 poll 一个顶层状态机。为了实现真正的多任务优先级调度，我将模型改造成支持 `spawn(priority, future)` 的 `PriorityExecutor`。每个顶层 Future 会被包装为一个 `ExecTask`，并拥有自己的任务编号、基础优先级、当前优先级、等待 tick、调度次数和 Future 本体。

优先级被划分为 High、Normal、Low 三类。Executor 内部不再使用单一 ready queue，而是维护三个队列：

```text
high queue
normal queue
low queue
```

任务首次 spawn 时，根据基础优先级进入对应队列。调度器每次选择任务时，优先从 high 队列取任务；如果 high 为空，再取 normal；最后取 low。同一优先级内部保持 FIFO 顺序。

该策略保证：

```text
High 优先于 Normal
Normal 优先于 Low
同优先级内部 FIFO
```

本次改造中最关键的一点是：优先级不仅要作用于任务首次进入 ready queue，也必须作用于异步唤醒路径。Future 的执行通常不会一次 poll 到结束，而是会反复经历：

```text
Running -> Pending -> wake -> Ready -> Running
```

如果任务返回 `Poll::Pending` 后，后续被 `Waker` 唤醒时没有按照优先级重新入队，那么优先级机制只在任务创建时有效，而在异步恢复路径上失效。因此，我重构了原始 Waker，使其不再只是简单地 unpark Executor，而是携带具体的 `ExecTask` 和 executor 内部状态。当 Reactor 或测试 Future 调用 `waker.wake()` 时，Waker 会将对应任务按照当前优先级重新放回对应 ready queue，并唤醒 Executor 继续调度。

为了避免手写 `RawWaker` 带来的引用计数和生命周期风险，我使用 `futures::task::ArcWake` 实现任务唤醒器。相比手动维护 `RawWakerVTable`，`ArcWake` 使用 `Arc` 管理生命周期，可以更安全地表达“wake 时重新入队该任务”这一语义。

## **9. Aging 机制与饥饿缓解**

严格优先级调度虽然能够保证高优先级任务优先运行，但它也可能导致低优先级任务饥饿。如果 high 队列中持续有任务进入，low 队列中的任务可能长期无法被调度。为了缓解这个问题，我在 Priority Executor 中实现了 aging 机制。

每个任务维护两个优先级：

```text
base_priority：任务创建时的基础优先级
current_priority：当前参与调度的动态优先级
```

同时记录：

```text
wait_ticks：任务在 ready queue 中等待的 tick 数
scheduled_count：任务被调度次数
```

Aging 的基本规则是：

```text
每轮调度时，对 ready queue 中未被选中的任务增加 wait_ticks

如果 wait_ticks >= aging_threshold:
    Low -> Normal
    Normal -> High
    High 保持不变

任务被 poll 一次后:
    wait_ticks 清零
    current_priority 恢复为 base_priority
```

也就是说，等待越久的任务会获得临时优先级提升。这样可以在保持高优先级任务优先响应的同时，避免低优先级任务永久等待。

需要注意的是，本实验中的 aging 主要解决 starvation，也就是低优先级任务长期得不到运行的问题。它并不等同于完整的 priority inversion 解决方案。优先级反转通常涉及高优先级任务等待低优先级任务持有的锁，而中优先级任务不断抢占低优先级任务。此类问题通常需要 priority inheritance 或 priority donation 机制，本实验暂未实现。

## **10. 测试设计与结果**

为了验证 Priority Executor 的正确性，我设计了多组测试，覆盖基础优先级调度、同优先级公平性、异步唤醒路径和 aging 防饥饿机制。

`high_priority_runs_first` 用于验证高优先级任务优先运行。测试中按照 Low、High、Normal 的顺序创建任务，但期望执行顺序是 High、Normal、Low。该测试证明调度器不是简单按照 spawn 顺序执行，而是根据 ready queue 优先级选择任务。

`same_priority_fifo` 用于验证同优先级内部保持 FIFO。多个 Normal 任务按照创建顺序进入队列，最终也按照相同顺序执行。这说明优先级调度并不意味着完全牺牲公平性，在同一优先级内部仍然保留先到先服务的顺序。

`wake_keeps_priority` 用于验证异步唤醒路径是否保持优先级。该测试构造 High 和 Low 两个会先返回 `Poll::Pending` 的任务，随后触发 Waker。预期行为是：High 任务 wake 后重新进入 high queue，Low 任务 wake 后重新进入 low queue，Executor 仍然优先调度 High。这个测试是本次实验的关键，因为它证明优先级机制不仅作用于 spawn，也作用于 `Pending -> wake -> re-enqueue` 路径。

`pending_then_ready_transition` 用于验证任务能够正确经历 Pending 到 Ready 的状态转换。测试可以使用一个 `YieldOnce` Future：第一次 poll 时调用 `wake_by_ref()` 并返回 `Pending`，第二次 poll 时返回 `Ready`。该测试避免依赖真实时间，使调度器测试更加稳定。

`strict_priority_can_starve_low` 用于展示严格优先级的局限。关闭 aging 后，构造多个 High 任务和一个 Low 任务，并限制调度 tick 数，可以观察到 Low 任务在限定轮次内可能完全无法运行。

`aging_prevents_starvation` 则在相同场景下开启 aging。测试结果显示，Low 任务等待若干 tick 后会被提升到 Normal，继续等待后被提升到 High，最终获得运行机会。

测试整体结果如下：

```text
running 17 tests
test high_priority_runs_first ... ok
test same_priority_fifo ... ok
test wake_keeps_priority ... ok
test pending_then_ready_transition ... ok
test strict_priority_can_starve_low ... ok
test aging_prevents_starvation ... ok

test result: ok. 17 passed; 0 failed
```

这些测试说明，Priority Executor 正确实现了三队列优先级调度、同优先级 FIFO、异步唤醒后按优先级重新入队，以及 aging 防饥饿机制。

## **11. 执行流状态机总结**

综合本周动态跟踪结果，可以将几类执行流的状态机抽象如下。

100 行无栈协程的执行流可以表示为：

```text
Queued
  -> Running
  -> Pending
  -> Queued
  -> Running
  -> Finished
```

200 行 Future executor 的执行流可以表示为：

```text
Created
  -> FirstPoll
  -> RegisterWaker
  -> Pending
  -> Parked
  -> ReactorEventReady
  -> Woken
  -> RePoll
  -> Finished
```

绿色线程的执行流可以表示为：

```text
Available
  -> Ready
  -> Running
  -> Ready
  -> Running
  -> Available
```

Priority Future Executor 的执行流可以表示为：

```text
Created
  -> ReadyHigh / ReadyNormal / ReadyLow
  -> Running
  -> Pending
  -> Woken
  -> ReadyHigh / ReadyNormal / ReadyLow
  -> Running
  -> Finished
```

统一来看，所有执行流机制都需要解决三个问题：状态保存在哪里，谁决定何时恢复，恢复后由谁继续推进执行。无栈 Future 的答案是状态机、Waker 和 Executor；绿色线程的答案是独立栈、调度器和上下文切换；Priority Executor 则在 Future 模型之上进一步加入了调度策略和公平性机制。

## **12. 与 ArceOS / StarryOS 比赛任务的关联**

本周的用户态执行流实验也为后续分析 ArceOS / StarryOS 的 I/O 公平性问题提供了一个小型模型。

在 Future Executor 中，`Poll::Pending` 可以类比为任务阻塞等待 I/O、锁或事件；`Waker::wake()` 可以类比为内核中的 `WaitQueue::notify_one()` 或 I/O completion；ready queue 可以类比为内核中的可运行队列。虽然这些概念并不完全等价，但它们在结构上都体现了：

```text
执行流暂时无法继续
  -> 注册等待信息
  -> 外部事件完成
  -> 唤醒执行流
  -> 重新进入可调度集合
```

当前比赛测试中，`iozone -t 4` 曾出现：

```text
Min throughput per process = 0.00 kB/sec
```

这说明在多进程 I/O 场景下，至少有一个进程在测试窗口内几乎没有获得有效 I/O 前进机会。该问题可能来自多个层面，包括 VFS 或文件系统层粗粒度锁、Ext4 元数据路径串行化、block cache 竞争、virtio-blk 同步等待、WaitQueue 唤醒不公平，或者 scheduler 对 I/O 唤醒任务调度不均衡。

本周实现的 Priority Executor 不能直接证明内核瓶颈所在，但它提供了一个后续分析方向：如果多个执行流竞争 I/O 资源，那么唤醒顺序、等待队列组织方式和等待时间补偿机制都可能影响最小吞吐和公平性。后续可以在 ArceOS / StarryOS 的 syscall、VFS、block I/O 或 WaitQueue 路径中加入 trace，记录：

```text
pid
wait_start_tick
wake_tick
wait_duration
wake_count
I/O operation type
block number
```

如果发现某些进程等待时间明显更长、唤醒次数明显更少，就可以尝试局部引入 priority-aware notify 或 aging 机制。该机制的目标不一定是提升平均吞吐，而是改善多进程并发场景下的最小吞吐和等待时间分布，避免某些进程长期没有前进机会。

## **13. 本周收获**

本周最大的收获是对“执行流”有了更具体的理解。过去我更多是从语法层面理解 `async/await`，认为 `.await` 就是“等待”。通过动态跟踪后可以看到，`.await` 背后实际是 Future 状态机、`poll()`、`Pending`、`Waker` 和 Executor 协作的结果。Future 不会自动运行，Waker 也不会直接运行 Future，真正推动执行流继续前进的动作仍然是 Executor 再次调用 `poll()`。

第二个收获是理解了无栈 Future 和有栈绿色线程的本质差异。无栈 Future 在 `Pending` 时退出当前 `poll()`，状态保存在编译器生成的结构体中；绿色线程在 `yield` 时不通过普通 return 退出，而是通过保存和恢复栈指针、寄存器上下文实现执行流切换。二者都能实现暂停和恢复，但工程权衡完全不同。

第三个收获是认识到调度策略必须覆盖完整执行路径。优先级调度不仅要在任务创建时生效，也要在任务从 `Pending` 被唤醒后重新入队时生效。如果 wake 路径丢失优先级信息，那么异步任务恢复时就会绕过调度策略，导致实现不完整。

第四个收获是明确了严格优先级和公平性之间的矛盾。严格优先级可以保证高优先级任务优先运行，但也可能导致低优先级任务饥饿。Aging 机制通过等待时间提升动态优先级，是一种简单有效的公平性补偿方式。

## **14. 下周计划**

内核改进方面，我暂时不直接修改 ArceOS / StarryOS 的全局调度器，而是优先在 I/O 路径中做局部插桩。重点观察 syscall read/write、VFS、block cache、virtio-blk 和 WaitQueue 路径，记录不同进程的等待时长和唤醒次数。如果确认存在明显不公平，再尝试在局部 WaitQueue 或 block I/O 请求队列中引入 priority-aware notify 和 aging 策略，并重跑 iozone 多进程测试，观察 `Min throughput per process` 是否改善。

此外，`which ls` 失败问题也需要单独排查。该问题更可能与 `envp`、`PATH` 传递、`execve` 用户栈构造或 `openat` / `faccessat` 路径解析有关，和本周的异步调度主线关系不大，因此可以作为独立 bug 修复任务推进。

## **15. 文档与参考链接**

本周相关代码、动态跟踪日志、设计文档和测试结果已经整理到目录中：

主要文档包括：

```text
assignments/async-flow-priority-os-study/user-space-experiments/stackless-coroutine-100-loc/100-LoC-stack-less-coroutine-执行流动态跟踪分析.md
assignments/async-flow-priority-os-study/user-space-experiments/Futures-Explained-in-200-Lines/200行-无栈协程-执行流动态跟踪分析.md
assignments/async-flow-priority-os-study/user-space-experiments/green-thread/green-thread-执行流动态跟踪.md
assignments/async-flow-priority-os-study/user-space-experiments/priority-executor/priority-executor-design.md
docs/design/mission-2/execution-flow-comparison.md
```

参考资料包括：

- [Tokio 中文文档 - Futures](https://tokio-zh.github.io/document/going-deeper/futures.html)
- [Green Threads Explained in 200 Lines of Rust](https://web.archive.org/web/20220529000219/https://cfsamson.gitbook.io/green-threads-explained-in-200-lines-of-rust/)
- [Green threads in Rust](https://gitee.com/ZIP97/green-thread)
- [A stack-less Rust coroutine library under 100 LoC](https://blog.aloni.org/posts/a-stack-less-rust-coroutine-100-loc/)
- [200行实现协程](https://nkbai.github.io/rust/Futures_Explained_in_200_lines_of_Rust.html)
- [Rust Stackless Coroutine](https://ruststack.org/stackless-coroutine/)

---
