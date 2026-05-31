use std::arch::naked_asm;

#[derive(Debug, Default)]
#[repr(C)]
pub struct ThreadContext {
    pub xmm6: [u64; 2],
    pub xmm7: [u64; 2],
    pub xmm8: [u64; 2],
    pub xmm9: [u64; 2],
    pub xmm10: [u64; 2],
    pub xmm11: [u64; 2],
    pub xmm12: [u64; 2],
    pub xmm13: [u64; 2],
    pub xmm14: [u64; 2],
    pub xmm15: [u64; 2],
    pub rsp: u64,
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub stack_start: u64,
    pub stack_end: u64,
}

// reference: https://probablydance.com/2013/02/20/handmade-coroutines-for-windows/
// Contents of TIB on Windows: https://en.wikipedia.org/wiki/Win32_Thread_Information_Block
#[naked]
#[no_mangle]
pub unsafe extern "C" fn switch(old: *mut ThreadContext, new: *const ThreadContext) {
    naked_asm!(
        "movaps      [rcx + 0x00], xmm6",
        "movaps      [rcx + 0x10], xmm7",
        "movaps      [rcx + 0x20], xmm8",
        "movaps      [rcx + 0x30], xmm9",
        "movaps      [rcx + 0x40], xmm10",
        "movaps      [rcx + 0x50], xmm11",
        "movaps      [rcx + 0x60], xmm12",
        "movaps      [rcx + 0x70], xmm13",
        "movaps      [rcx + 0x80], xmm14",
        "movaps      [rcx + 0x90], xmm15",
        "mov         [rcx + 0xa0], rsp",
        "mov         [rcx + 0xa8], r15",
        "mov         [rcx + 0xb0], r14",
        "mov         [rcx + 0xb8], r13",
        "mov         [rcx + 0xc0], r12",
        "mov         [rcx + 0xc8], rbx",
        "mov         [rcx + 0xd0], rbp",
        "mov         [rcx + 0xd8], rdi",
        "mov         [rcx + 0xe0], rsi",
        "mov         rax, gs:0x08",
        "mov         [rcx + 0xe8], rax",
        "mov         rax, gs:0x10",
        "mov         [rcx + 0xf0], rax",
        "movaps      xmm6, [rdx + 0x00]",
        "movaps      xmm7, [rdx + 0x10]",
        "movaps      xmm8, [rdx + 0x20]",
        "movaps      xmm9, [rdx + 0x30]",
        "movaps      xmm10, [rdx + 0x40]",
        "movaps      xmm11, [rdx + 0x50]",
        "movaps      xmm12, [rdx + 0x60]",
        "movaps      xmm13, [rdx + 0x70]",
        "movaps      xmm14, [rdx + 0x80]",
        "movaps      xmm15, [rdx + 0x90]",
        "mov         rsp, [rdx + 0xa0]",
        "mov         r15, [rdx + 0xa8]",
        "mov         r14, [rdx + 0xb0]",
        "mov         r13, [rdx + 0xb8]",
        "mov         r12, [rdx + 0xc0]",
        "mov         rbx, [rdx + 0xc8]",
        "mov         rbp, [rdx + 0xd0]",
        "mov         rdi, [rdx + 0xd8]",
        "mov         rsi, [rdx + 0xe0]",
        "mov         rax, [rdx + 0xe8]",
        "mov         gs:0x08, rax",
        "mov         rax, [rdx + 0xf0]",
        "mov         gs:0x10, rax",
        "ret",
        options(noreturn)
    );
}

impl super::Runtime {
    pub fn spawn(&mut self, f: fn()) {
        use super::{guard, skip, State};

        let available = self
            .threads
            .iter_mut()
            .find(|t| t.state == State::Available)
            .expect("no available thread.");

        let size = available.stack.len();

        // see: https://docs.microsoft.com/en-us/cpp/build/stack-usage?view=vs-2019#stack-allocation
        unsafe {
            let s_ptr = available.stack.as_mut_ptr().offset(size as isize);
            let s_ptr = (s_ptr as usize & !15) as *mut u8;
            std::ptr::write(s_ptr.offset((size - 16) as isize) as *mut u64, guard as u64);
            std::ptr::write(s_ptr.offset((size - 24) as isize) as *mut u64, skip as u64);
            std::ptr::write(s_ptr.offset((size - 32) as isize) as *mut u64, f as u64);
            available.ctx.rsp = s_ptr.offset((size - 32) as isize) as u64;
            available.ctx.stack_start = s_ptr as u64;

            available.ctx.stack_end = s_ptr as *const u64 as u64;
        }

        available.state = State::Ready;
    }
}
