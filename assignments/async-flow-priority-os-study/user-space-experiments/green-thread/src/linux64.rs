use std::arch::naked_asm;

#[derive(Debug, Default)]
#[repr(C)]
pub struct ThreadContext {
    pub rsp: u64,
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
}

// J. switch — naked assembly function, NO instrumentation
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn switch(old: *mut ThreadContext, new: *const ThreadContext) {
    naked_asm!(
        "mov [rdi + 0x00], rsp",
        "mov [rdi + 0x08], r15",
        "mov [rdi + 0x10], r14",
        "mov [rdi + 0x18], r13",
        "mov [rdi + 0x20], r12",
        "mov [rdi + 0x28], rbx",
        "mov [rdi + 0x30], rbp",
        "mov rsp, [rsi + 0x00]",
        "mov r15, [rsi + 0x08]",
        "mov r14, [rsi + 0x10]",
        "mov r13, [rsi + 0x18]",
        "mov r12, [rsi + 0x20]",
        "mov rbx, [rsi + 0x28]",
        "mov rbp, [rsi + 0x30]",
        "ret"
    );
}

// C. Runtime::spawn
impl crate::Runtime {
    pub fn spawn(&mut self, f: fn()) {
        use super::{guard, skip, State};

        trace!("runtime: spawn enter");

        let available = self
            .threads
            .iter_mut()
            .find(|t| t.state == State::Available)
            .expect("no available thread.");

        trace!("runtime: spawn found available thread-{}", available.id);

        let size = available.stack.len();
        unsafe {
            let s_ptr = available.stack.as_mut_ptr().add(size);
            let s_ptr = (s_ptr as usize & !15) as *mut u8;
            std::ptr::write(s_ptr.offset(-16) as *mut u64, guard as usize as u64);
            std::ptr::write(s_ptr.offset(-24) as *mut u64, skip as usize as u64);
            std::ptr::write(s_ptr.offset(-32) as *mut u64, f as usize as u64);
            available.ctx.rsp = s_ptr.offset(-32) as u64;

            trace!(
                "runtime: spawn thread-{} stack_base={:#x} stack_size={} initial_rsp={:#x}",
                available.id,
                available.stack.as_ptr() as usize,
                size,
                available.ctx.rsp
            );
        }

        trace!("runtime: spawn thread-{} Available -> Ready", available.id);
        available.state = State::Ready;

        trace!("runtime: spawn complete");
    }
}
