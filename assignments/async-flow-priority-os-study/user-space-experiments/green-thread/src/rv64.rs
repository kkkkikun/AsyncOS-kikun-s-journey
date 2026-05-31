//! src: https://github.com/systemxlabs/green-threads-in-200-lines-of-rust

use std::arch::naked_asm;

#[derive(Debug, Default)]
#[repr(C)]
pub struct ThreadContext {
    // return address
    ra: u64,
    // stack pointer
    sp: u64,
    // s0 - s11 (callee saved registers)
    s0: u64,
    s1: u64,
    s2: u64,
    s3: u64,
    s4: u64,
    s5: u64,
    s6: u64,
    s7: u64,
    s8: u64,
    s9: u64,
    s10: u64,
    s11: u64,
    // task entry
    entry: u64,
}

#[naked]
#[no_mangle]
pub unsafe extern "C" fn switch(old: *mut ThreadContext, new: *const ThreadContext) {
    // a0: old, a1: new
    naked_asm!(
        "
        sd ra, 0*8(a0)
        sd sp, 1*8(a0)
        sd s0, 2*8(a0)
        sd s1, 3*8(a0)
        sd s2, 4*8(a0)
        sd s3, 5*8(a0)
        sd s4, 6*8(a0)
        sd s5, 7*8(a0)
        sd s6, 8*8(a0)
        sd s7, 9*8(a0)
        sd s8, 10*8(a0)
        sd s9, 11*8(a0)
        sd s10, 12*8(a0)
        sd s11, 13*8(a0)
        // When user task scheduled for the second time,
        // overwrite task entry address with the return address
        sd ra, 14*8(a0)

        ld ra, 0*8(a1)
        ld sp, 1*8(a1)
        ld s0, 2*8(a1)
        ld s1, 3*8(a1)
        ld s2, 4*8(a1)
        ld s3, 5*8(a1)
        ld s4, 6*8(a1)
        ld s5, 7*8(a1)
        ld s6, 8*8(a1)
        ld s7, 9*8(a1)
        ld s8, 10*8(a1)
        ld s9, 11*8(a1)
        ld s10, 12*8(a1)
        ld s11, 13*8(a1)
        // When user task scheduled for the first time, t0 will be task entry address.
        // After that, t0 will be return address
        ld t0, 14*8(a1)

        // pseudo instruction, actually is jalr x0, 0(t0)
        jr t0
    "
    );
}

impl crate::Runtime {
    /// Spawn a new task to be executed by a green thread in runtime
    pub fn spawn(&mut self, f: fn()) {
        use super::{guard, State};

        let available = self
            .threads
            .iter_mut()
            .find(|t| t.state == State::Available)
            .expect("no available green thread.");

        println!("RUNTIME: spawning task on green thread {}", available.id);
        let size = available.stack.len();
        unsafe {
            let s_ptr = available.stack.as_mut_ptr().offset(size as isize);

            // make sure our stack itself is 8 byte aligned,
            // risc-v ld/sd instructions need address 8 byte alignment
            let s_ptr = (s_ptr as usize & !7) as *mut u8;

            available.ctx.ra = guard as u64; // task return address
            available.ctx.sp = s_ptr as u64; // stack pointer
            available.ctx.entry = f as u64; // task entry address
        }
        available.state = State::Ready;
    }
}
