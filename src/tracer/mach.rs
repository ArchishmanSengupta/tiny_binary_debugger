use mach2::kern_return::KERN_SUCCESS;
use mach2::port::{mach_port_t, MACH_PORT_NULL};
use mach2::task::{task_resume, task_suspend};
use mach2::vm_types::{mach_vm_address_t, mach_vm_size_t};
use mach2::message::mach_msg_type_number_t;
use std::ptr;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct x86_thread_state64_t {
    pub rax: u64, pub rbx: u64, pub rcx: u64, pub rdx: u64,
    pub rdi: u64, pub rsi: u64, pub rbp: u64, pub rsp: u64,
    pub r8: u64, pub r9: u64, pub r10: u64, pub r11: u64,
    pub r12: u64, pub r13: u64, pub r14: u64, pub r15: u64,
    pub rip: u64, pub rflags: u64, pub cs: u64, pub fs: u64, pub gs: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct arm_thread_state64_t {
    pub x: [u64; 29],
    pub fp: u64,
    pub lr: u64,
    pub sp: u64,
    pub pc: u64,
    pub cpsr: u32,
    pub pad: u32,
}

const X86_THREAD_STATE64: i32 = 4;
const ARM_THREAD_STATE64: i32 = 6;

extern "C" {
    fn task_for_pid(target_task: mach_port_t, pid: i32, task: *mut mach_port_t) -> i32;
    fn mach_vm_read_overwrite(
        target: mach_port_t, address: mach_vm_address_t, size: mach_vm_size_t,
        data: mach_vm_address_t, out_size: *mut mach_vm_size_t
    ) -> i32;
    fn thread_get_state(
        thread: mach_port_t, flavor: i32,
        state: *mut u64, count: *mut mach_msg_type_number_t
    ) -> i32;
    fn task_threads(
        task: mach_port_t, threads: *mut *mut mach_port_t, count: *mut mach_msg_type_number_t
    ) -> i32;
}

pub struct MachTask {
    pub task: mach_port_t,
    pub pid: i32,
}

impl MachTask {
    pub fn attach(pid: i32) -> Result<Self, String> {
        let mut task: mach_port_t = MACH_PORT_NULL;
        unsafe {
            let kr = task_for_pid(mach2::traps::mach_task_self(), pid, &mut task);
            if kr != KERN_SUCCESS {
                return Err(format!("task_for_pid failed: {}", kr));
            }
        }
        Ok(Self { task, pid })
    }

    pub fn suspend(&self) -> Result<(), String> {
        unsafe {
            let kr = task_suspend(self.task);
            if kr != KERN_SUCCESS {
                return Err(format!("task_suspend failed: {}", kr));
            }
        }
        Ok(())
    }

    pub fn resume(&self) -> Result<(), String> {
        unsafe {
            let kr = task_resume(self.task);
            if kr != KERN_SUCCESS {
                return Err(format!("task_resume failed: {}", kr));
            }
        }
        Ok(())
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
                return Err(format!("mach_vm_read failed: {}", kr));
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
            Ok(thread_vec)
        }
    }

    #[cfg(target_arch = "x86_64")]
    pub fn get_thread_state(&self, thread: mach_port_t) -> Result<x86_thread_state64_t, String> {
        let mut state = x86_thread_state64_t {
            rax: 0, rbx: 0, rcx: 0, rdx: 0, rdi: 0, rsi: 0, rbp: 0, rsp: 0,
            r8: 0, r9: 0, r10: 0, r11: 0, r12: 0, r13: 0, r14: 0, r15: 0,
            rip: 0, rflags: 0, cs: 0, fs: 0, gs: 0,
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
            x: [0; 29], fp: 0, lr: 0, sp: 0, pc: 0, cpsr: 0, pad: 0,
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

