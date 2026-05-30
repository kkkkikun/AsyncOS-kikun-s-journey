## 学习：Tokio中文文档 - Futures

阅读学习《Tokio中文文档 - Futures》部分记录的一点个人心得

> 原文链接🔗：https://tokio-zh.github.io/document/going-deeper/futures.html

---

### 什么是 Futures

“future是表示异步计算完成的值。”

future 构造了一种结构，可以不用马上返回值，而是返回一种预期。比如下面的片段：

```rust
async fn my_network_task() {
    // 1. 瞬间返回一个 Future，此时网络请求刚发出
    let response_future = client.get("https://www.example.com"); 
    
    // 2. 使用 .await 语法
    // 这行代码的意思是：“Tokio，我在这里挂起了，等这个 future 变成 Ready 状态后再唤醒我”
    // 代码执行在这里停下，进入“挂起（Suspend）”状态
    // 函数将返回 NotReady，函数本身将被转成一个留在堆里的结构体（状态机）
    // await 变成了状态机内部的 match 分支跳转
    // 直到任务完成，底层的网卡返回硬件通知
    // 通过注册 Waker 唤醒器，Tokio 可以在未来的任何时候，精准地顺着指针找到这个结构体，重新调用它的 poll
    // CPU将转向其他任务
    let response = response_future.await; 
    
    // 3. 走到这里时，说明 Future 已经完成了，response 已经变成了真正的响应数据
    println!("状态码是: {}", response.status());
}
```



硬件、操作系统、Tokio 和你的 Future 之间是如何打配合的：


```
[步骤 1] 你的代码：client.get("https://...") 
        -> 创建了一个 ResponseFuture，此时状态是：初始状态。

[步骤 2] Tokio 把这个网络连接的“死活”注册给操作系统（比如 Linux 的 epoll）。
        接着 Tokio 第一次调用 poll(&mut future)。
        Future 检查发现网卡还没收到数据，于是返回 NotReady。
        此时，整个任务挂起，CPU 拍拍屁股去干别的事了（比如处理别人的请求）。

[步骤 3] 【底层硬件通知】过了几毫秒，网卡芯片（硬件）收到了网络数据包！
        网卡通过 DMA 把数据丢进内存，并给 CPU 发了一个硬中断。
        操作系统的 epoll 睁开眼，标记着：“这个连接有数据到了！”

         ⚠️ 注意！此时此刻：
         数据只是躺在内核的缓冲区里，你的 ResponseFuture 依然是 NotReady。
         没有任何业务代码在运行，它确实没有“默默自我推进”。

[步骤 4] Tokio 的事件循环（Event Loop）拿到了操作系统的通知：“刚才那个连接有新数据了！”
        Tokio 顺藤摸瓜，找到了对应这个连接的 ResponseFuture。

[步骤 5] 【Tokio 主动调用 poll】Tokio 再次调用这尊 Future 的 poll 方法。
        这一次，Future 内部一检查：“哎呀，内存里真的有网络数据了！”
        于是它高高兴兴地解析数据，把状态修改为 Ready(Response)，任务成功推进并结束。
```

---

### 样例 future

对作者给出的一个更复杂的`future`的一点分析：

作者给出的：

- ```rust
  // 假设存在 resolve 函数
  pub fn resolve(host: &str) -> ResolveFuture;
  
  // 使用枚举来跟踪future的状态
  enum State {
      // Currently resolving the host name
      Resolving(ResolveFuture),
  
      // Establishing a TCP connection to the remote host
      Connecting(ConnectFuture),
  }
  
  
  // ResolveAndConnect的future定义为：
  pub struct ResolveAndConnect {
      state: State,
  }
  
  
  pub fn resolve_and_connect(host: &str) -> ResolveAndConnect {
      let state = State::Resolving(resolve(host));
      ResolveAndConnect { state }
  }
  
  impl Future for ResolveAndConnect {
      type Item = TcpStream;
      type Error = io::Error;
  
      fn poll(&mut self) -> Result<Async<TcpStream>, io::Error> {
          use self::State::*;
          
  		// 🚀 注意：这里有一个不带条件死循环 loop！
          loop {
              let addr = match self.state {
                  // 【分支 A】如果当前处于“域名解析”状态
                  Resolving(ref mut fut) => {
                      // 轮询内部的域名解析 Future。
                      // try_ready! 宏：如果返回 NotReady，它会直接 return 退出整个 poll 函数；
                      // 如果返回 Ready(addr)，代码会跨过这条宏，把 addr 提取出来，继续往下走！
                      try_ready!(fut.poll())
                  }
                  // 【分支 B】如果当前（或者由于上面的跳转）处于“建立连接”状态
                  Connecting(ref mut fut) => {
                      return fut.poll();
                  }
              };
  			// 【状态跳转】走到这里说明域名解析成功了！
              // 我们立刻发起 TCP 连接，并把状态修改为 Connecting
              let connecting = TcpStream::connect(&addr);
              self.state = Connecting(connecting);
          }
      }
  }
  ```

- #### 🚀 `loop` 的真正目的：

  - 如果没有这个 `loop`，当第一步 `Resolving` 成功拿到 IP 变成 `Connecting` 之后，函数就结束了。必须等待 Tokio 下一次再来 `poll` 才能进入第二步。
  - 而加上 `loop`，可以让状态机在**同一时刻**如果发现前一步好了，**不需要等待下一次事件循环，立刻、无缝地在内存里推进到下一步**！



#### 更进一步，假如说有三个future：

- 假设要写一个爬虫，业务逻辑分为三步：
  1. **第一步 (DNS)**：把网页域名解析成 IP 地址（产生 `ResolveFuture`）。
  2. **第二步 (Connect)**：连接到该 IP 地址，建立 TCP 通道（产生 `ConnectFuture`）。
  3. **第三步 (Download)**：发送 HTTP 请求，并把整个网页数据下载下来（产生 `DownloadFuture`）。

- 用 Rust 的 `async/await` 语法写，它长这样：

   ```rust
   async fn crawl_page(host: &str) -> String {
        let addr = resolve(host).await;       // ➔ 卡点 1
        let mut stream = connect(&addr).await; // ➔ 卡点 2
        let html = download(&mut stream).await; // ➔ 卡点 3
        html
    }
   ```

- future 实现：

   ```rust
   enum State {
       // 状态 1：正在解析域名
       Resolving(ResolveFuture),
       
       // 状态 2：域名好了，正在建立 TCP 连接（肚子里抱着解析好的 IP 地址和连接任务）
       Connecting { fut: ConnectFuture, addr: SocketAddr },
       
       // 状态 3：连接也好了，正在下载网页（肚子里抱着建立好的连接和下载任务）
       Downloading { fut: DownloadFuture, stream: TcpStream },
       
       // 状态 4：彻底完工
       Done,
   }
   
   // 🌿不论函数里套了 3 个还是 30 个 .await，它们在内存中永远只占用一个 CrawlTask 结构体的大小。
   pub struct CrawlTask {
       state: State,
   }
   
   
   
   impl Future for CrawlTask {
       type Item = String;
       type Error = io::Error;
   
       fn poll(&mut self) -> Result<Async<String>, io::Error> {
           use self::State::*;
           
           loop {
               match self.state {
                   // 【第一关】域名解析
                   Resolving(ref mut fut) => {
                       let addr = try_ready!(fut.poll()); // 如果没好，立刻 return NotReady 冻结！
                       
                       // 好了！立刻原地蜕壳，进入下一阶段
                       let connect_fut = connect(&addr);
                       self.state = Connecting { fut: connect_fut, addr };
                       // 没有 return！loop 弹回顶端，进入下一次迭代！
                   }
                   
                   // 【第二关】建立连接
                   Connecting { ref mut fut, .. } => {
                       let stream = try_ready!(fut.poll()); // 如果网络在握手，立刻 return NotReady 冻结！
                       
                       // 好了！再次原地蜕壳，进入下载阶段
                       let download_fut = download(&stream);
                       self.state = Downloading { fut: download_fut, stream };
                       // 没有 return！loop 弹回顶端，进入下一次迭代！
                   }
                   
                   // 【第三关】下载数据
                   Downloading { ref mut fut, .. } => {
                       let html = try_ready!(fut.poll()); // 如果数据还没传输完，立刻 return NotReady 冻结！
                       
                       // 全部大功告成！
                       self.state = Done;
                       return Ok(Async::Ready(html)); // 彻底退出
                   }
                   
                   Done => panic!("Task already completed"),
               }
           }
       }
   }
   ```

   - 模拟运行：假设我们这个任务执行到一半，**卡在第二步（TCP 握手慢）**，操作系统和 Tokio 是如何精确保存并找回它的？

      1. **第一次 poll**： Tokio 过来调用 `CrawlTask::poll`。状态是 `Resolving`。运气很好，DNS 瞬间解析完了。 `try_ready!` 放行，状态在内存里**被修改为 `Connecting`**。 `loop` 弹回顶端。

      2. **在同一次 poll 中**： 进入第二轮循环，`match` 匹配到了 `Connecting`。开始调用 TCP 连接的 `fut.poll()`。 此时，网络有延迟，TCP 握手还没完，返回了 `NotReady`。 `try_ready!` 运行，**直接执行 `return Ok(Async::NotReady);`，整个大函数彻底结束并退出。**

         > **⚠️ 此时的内存状态：** 虽然 `poll` 函数退出了，但 `CrawlTask` 结构体留在堆里。它的 `state` 字段现在**死死地固定在了 `Connecting` 这一档**，里面还保管着连接到一半的句柄。

      3. **第二次被唤醒**： 过了 50 毫秒，网卡收到了 TCP 握手成功的包，操作系统通知 Tokio。Tokio 顺着 `Waker` 指针找到这个结构体，重新调用 `CrawlTask::poll`。 一进来，`match self.state` 睁眼一看：“哦，上次停在 `Connecting` 状态。” 代码**直接略过 `Resolving`**，进入 `Connecting` 分支。 再次调用底层 `fut.poll()`，这次顺利返回 `Ready(stream)`。 修改状态为 `Downloading`。 `loop` 再次弹回顶端…… 依此类推。



## 学习：A stack-less Rust coroutine library under 100 LoC

该项目展示了如何脱离任何复杂的异步运行时（如 Tokio），仅依靠 Rust 编译器自带的 `async/await` 状态机机制，在百行内实现一个协作式（Cooperative）多任务调度器。

### 1. 执行流状态变迁的独特实现
该设计通过一个自定义的 `Waiter` 结构体实现了类似生成器（Generator）的 `yield` 语义。其状态机变迁逻辑如下：
- **Running -> Pending**：任务主动 `.await` 触发 `poll`。此时状态为 `State::Running`，代码强行将其修改为 `State::Halted` 并返回 `Poll::Pending`。此时执行流交还给 Executor，实现“主动让出（Yield）”。
- **Pending -> Ready**：Executor 将任务置于队尾并重新轮询。再次进入 `poll` 时状态为 `State::Halted`，此时将其改回 `State::Running` 并返回 `Poll::Ready(())`，使异步上下文得以在断点处继续向下推进。

### 2. 与标准运行时的差异
该项目揭示了无栈协程的极低开销特质（每轮迭代仅需约 5 纳秒）。但为了追求极简，其设计故意违反了 Rust 标准的 Future 契约：它没有使用真正的事件通知机制，而是依赖一个空操作的 `Null Waker`，并通过 Executor 内部的 `while let` 死循环对就绪队列（`VecDeque`）进行强制无条件轮询。这种模式适合无硬件中断的纯协作式计算流，但无法直接承载真正的网络或块设备异步 I/O 驱动。



## 学习心得：《Futures Explained in 200 lines of Rust》深度执行流分析

该项目不依赖高级的运行时库，完全使用标准库底层的 `RawWakerVTable`、`Condvar` 和多线程，完整还原了异步运行时（如 Tokio）的内部闭环机制。

### 1. 核心组件的执行流协作模型
- **Executor 阻塞机制**：通过手写 `Parker` 结构体，内部封装 `Mutex<bool>` 与 `Condvar`。在 `block_on` 的主循环中，若 `Future::poll` 返回 `Poll::Pending`，主线程立刻调用 `parker.park()` 挂起，切出 CPU。
- **Waker 桥梁构建**：通过不安全指针操作构建 `RawWakerVTable`，将包含 `Parker` 引用的 `MyWaker` 打包为标准库 `Waker`。该 Waker 会在 `Task::poll` 阻塞时被克隆并交由后台 `Reactor` 保管。
- **Reactor 事件唤醒**：`Reactor` 内部维护后台线程池（模拟硬件中断/DMA 行为）。当延迟时间到达后，后台线程顺着保存的 `Waker` 调用 `wake()`，触发 `unpark()` 激活条件变量，促使主线程在 `block_on` 处“就地复活”并进行下一轮 `poll`。

### 2. 本项目对 ArceOS 比赛任务的启发
本项目的 `Reactor::tasks` 使用了一个 `HashMap<usize, TaskState>` 来记录所有挂起任务的 Waker。当事件就绪时，`Reactor` 会**精准唤醒**（Notify_one）对应的 Waker。
这与操作系统的 **等待队列（WaitQueue）** 结构完全一致。在 ArceOS 的 I/O 路径中，当块设备未就绪时，内核任务会被挂在 WaitQueue 上；当硬中断解析出 I/O 完成后，再通过 WaitQueue 唤醒任务。如果在 `Reactor` 的 `tasks` 或者是内核的 `WaitQueue` 唤醒时加入优先级决策，就可能解决进程饥饿问题。