use mach2::kern_return::KERN_SUCCESS;
use mach2::message::mach_msg_type_number_t;
use mach2::port::{mach_port_t, MACH_PORT_NULL};
use mach2::vm_types::{mach_vm_address_t, mach_vm_size_t};
use std::ptr;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[allow(non_camel_case_types, dead_code)]
pub struct x86_thread_state64_t {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
    pub cs: u64,
    pub fs: u64,
    pub gs: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[allow(non_camel_case_types, dead_code)]
pub struct arm_thread_state64_t {
    pub x: [u64; 29],
    pub fp: u64,
    pub lr: u64,
    pub sp: u64,
    pub pc: u64,
    pub cpsr: u32,
    pub pad: u32,
}

#[allow(dead_code)]
const X86_THREAD_STATE64: i32 = 4;
#[allow(dead_code)]
const ARM_THREAD_STATE64: i32 = 6;

extern "C" {
    fn task_for_pid(target_task: mach_port_t, pid: i32, task: *mut mach_port_t) -> i32;
    fn mach_vm_read_overwrite(
        target: mach_port_t,
        address: mach_vm_address_t,
        size: mach_vm_size_t,
        data: mach_vm_address_t,
        out_size: *mut mach_vm_size_t,
    ) -> i32;
    fn thread_get_state(
        thread: mach_port_t,
        flavor: i32,
        state: *mut u64,
        count: *mut mach_msg_type_number_t,
    ) -> i32;
    fn task_threads(
        task: mach_port_t,
        threads: *mut *mut mach_port_t,
        count: *mut mach_msg_type_number_t,
    ) -> i32;
    fn mach_vm_deallocate(
        target: mach_port_t,
        address: mach_vm_address_t,
        size: mach_vm_size_t,
    ) -> i32;
}

pub struct MachTask {
    pub task: mach_port_t,
}

impl MachTask {
    /// Get the Mach task port for a process.
    /// Used only for reading state (registers, memory) -- not for control.
    pub fn attach(pid: i32) -> Result<Self, String> {
        let mut task: mach_port_t = MACH_PORT_NULL;
        unsafe {
            let kr = task_for_pid(mach2::traps::mach_task_self(), pid, &mut task);
            if kr != KERN_SUCCESS {
                return Err(format!(
                    "task_for_pid failed: {} (need sudo + code signing with entitlements)",
                    kr
                ));
            }
        }
        Ok(Self { task })
    }

    pub fn read_memory(&self, addr: u64, size: usize) -> Result<Vec<u8>, String> {
        let mut buf = vec![0u8; size];
        let mut out_size: mach_vm_size_t = 0;
        unsafe {
            let kr = mach_vm_read_overwrite(
                self.task,
                addr as mach_vm_address_t,
                size as mach_vm_size_t,
                buf.as_mut_ptr() as mach_vm_address_t,
                &mut out_size,
            );
            if kr != KERN_SUCCESS {
                return Err(format!("mach_vm_read failed at 0x{:x}: {}", addr, kr));
            }
        }
        buf.truncate(out_size as usize);
        Ok(buf)
    }

    pub fn get_threads(&self) -> Result<Vec<mach_port_t>, String> {
        let mut threads: *mut mach_port_t = ptr::null_mut();
        let mut count: mach_msg_type_number_t = 0;
        unsafe {
            let kr = task_threads(self.task, &mut threads, &mut count);
            if kr != KERN_SUCCESS {
                return Err(format!("task_threads failed: {}", kr));
            }
            let thread_vec = std::slice::from_raw_parts(threads, count as usize).to_vec();

            // Free the Mach-allocated thread list to avoid memory leak
            let alloc_size = (count as usize) * std::mem::size_of::<mach_port_t>();
            mach_vm_deallocate(
                mach2::traps::mach_task_self(),
                threads as mach_vm_address_t,
                alloc_size as mach_vm_size_t,
            );

            Ok(thread_vec)
        }
    }

    #[cfg(target_arch = "x86_64")]
    pub fn get_thread_state(&self, thread: mach_port_t) -> Result<x86_thread_state64_t, String> {
        let mut state = x86_thread_state64_t {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rdi: 0,
            rsi: 0,
            rbp: 0,
            rsp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rip: 0,
            rflags: 0,
            cs: 0,
            fs: 0,
            gs: 0,
        };
        let mut count = std::mem::size_of::<x86_thread_state64_t>() as mach_msg_type_number_t / 4;
        unsafe {
            let kr = thread_get_state(
                thread,
                X86_THREAD_STATE64,
                &mut state as *mut _ as *mut u64,
                &mut count,
            );
            if kr != KERN_SUCCESS {
                return Err(format!("thread_get_state failed: {}", kr));
            }
        }
        Ok(state)
    }

    #[cfg(target_arch = "aarch64")]
    pub fn get_thread_state(&self, thread: mach_port_t) -> Result<arm_thread_state64_t, String> {
        let mut state = arm_thread_state64_t {
            x: [0; 29],
            fp: 0,
            lr: 0,
            sp: 0,
            pc: 0,
            cpsr: 0,
            pad: 0,
        };
        let mut count = std::mem::size_of::<arm_thread_state64_t>() as mach_msg_type_number_t / 4;
        unsafe {
            let kr = thread_get_state(
                thread,
                ARM_THREAD_STATE64,
                &mut state as *mut _ as *mut u64,
                &mut count,
            );
            if kr != KERN_SUCCESS {
                return Err(format!("thread_get_state failed: {}", kr));
            }
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The x86_64 thread state struct has 21 u64 fields = 168 bytes.
    /// This must match what the Mach kernel expects for x86_THREAD_STATE64.
    #[test]
    fn x86_thread_state64_size() {
        assert_eq!(
            std::mem::size_of::<x86_thread_state64_t>(),
            21 * 8, // 21 u64 fields = 168 bytes
            "x86_thread_state64_t should be 168 bytes (21 x u64)"
        );
    }

    /// Alignment must be 8 for correct FFI with Mach APIs.
    #[test]
    fn x86_thread_state64_alignment() {
        assert_eq!(
            std::mem::align_of::<x86_thread_state64_t>(),
            8,
            "x86_thread_state64_t must be 8-byte aligned"
        );
    }

    /// The ARM64 thread state struct: 33 u64 fields (264 bytes)
    /// plus cpsr (u32) and pad (u32) for 8 more bytes = 272 total.
    #[test]
    fn arm_thread_state64_size() {
        assert_eq!(
            std::mem::size_of::<arm_thread_state64_t>(),
            272,
            "arm_thread_state64_t should be 272 bytes (33 x u64 + u32 + u32)"
        );
    }

    /// Alignment must be 8 for correct FFI with Mach APIs.
    #[test]
    fn arm_thread_state64_alignment() {
        assert_eq!(
            std::mem::align_of::<arm_thread_state64_t>(),
            8,
            "arm_thread_state64_t must be 8-byte aligned"
        );
    }

    /// Verify x86_thread_state64_t is zero-initializable (all fields are plain integers).
    #[test]
    fn x86_thread_state64_zeroed() {
        let state: x86_thread_state64_t = unsafe { std::mem::zeroed() };
        assert_eq!(state.rax, 0);
        assert_eq!(state.rip, 0);
        assert_eq!(state.rsp, 0);
        assert_eq!(state.rflags, 0);
    }

    /// Verify arm_thread_state64_t is zero-initializable.
    #[test]
    fn arm_thread_state64_zeroed() {
        let state: arm_thread_state64_t = unsafe { std::mem::zeroed() };
        assert_eq!(state.x[0], 0);
        assert_eq!(state.x[28], 0);
        assert_eq!(state.pc, 0);
        assert_eq!(state.sp, 0);
        assert_eq!(state.cpsr, 0);
        assert_eq!(state.pad, 0);
    }

    /// The Mach thread_get_state count parameter is in units of natural_t (u32),
    /// so count = size_of / 4. Verify this math is correct for both structs.
    #[test]
    fn x86_state_count_calculation() {
        let count = std::mem::size_of::<x86_thread_state64_t>() / 4;
        assert_eq!(count, 42, "x86 state count should be 42 (168/4)");
    }

    #[test]
    fn arm_state_count_calculation() {
        let count = std::mem::size_of::<arm_thread_state64_t>() / 4;
        assert_eq!(count, 68, "arm64 state count should be 68 (272/4)");
    }

    /// Verify the flavor constants match known Mach values.
    #[test]
    fn flavor_constants() {
        assert_eq!(X86_THREAD_STATE64, 4);
        assert_eq!(ARM_THREAD_STATE64, 6);
    }
}
