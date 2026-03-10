use crate::storage::{MemChange, TraceDb, TraceEntry};
use crate::tracer::mach::MachTask;
use capstone::prelude::*;
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::sync::Arc;

/// Result of a single-step operation.
pub enum StepResult {
    /// Successfully recorded one instruction.
    Ok(TraceEntry),
    /// The traced process exited (with exit code).
    ProcessExited(i32),
    /// An error occurred (retryable).
    Error(String),
}

pub struct Tracer {
    task: MachTask,
    pid: i32,
    db: Arc<TraceDb>,
    cs: Capstone,
    step_count: u64,
    memory_cache: HashMap<u64, Vec<u8>>,
    last_sp: u64,
    call_depth: u64,
    pending_signal: Option<Signal>,
}

impl Tracer {
    pub fn new(pid: i32, db_path: &str) -> Result<Self, String> {
        // Get Mach task port for reading state (registers + memory).
        // Process control (stepping) is done via ptrace, not Mach.
        let task = MachTask::attach(pid)?;
        let db = Arc::new(TraceDb::new(db_path)?);

        #[cfg(target_arch = "x86_64")]
        let cs = Capstone::new()
            .x86()
            .mode(arch::x86::ArchMode::Mode64)
            .build()
            .map_err(|e| format!("Capstone init failed: {}", e))?;

        #[cfg(target_arch = "aarch64")]
        let cs = Capstone::new()
            .arm64()
            .mode(arch::arm64::ArchMode::Arm)
            .build()
            .map_err(|e| format!("Capstone init failed: {}", e))?;

        Ok(Self {
            task,
            pid,
            db,
            cs,
            step_count: 0,
            memory_cache: HashMap::new(),
            last_sp: 0,
            call_depth: 0,
            pending_signal: None,
        })
    }

    /// Execute exactly one instruction and record the complete state.
    ///
    /// The process must already be in a ptrace-stopped state when this
    /// is called (either from launch with PT_TRACE_ME or a previous step).
    ///
    /// Flow:
    ///   1. Read registers via Mach thread_get_state
    ///   2. Read memory at PC, disassemble
    ///   3. Detect call/return, memory changes
    ///   4. Record the TraceEntry
    ///   5. ptrace(PT_STEP) to execute exactly one instruction
    ///   6. waitpid() to catch the stop after that instruction
    pub fn single_step(&mut self) -> StepResult {
        // 1. Read thread state via Mach (richer than ptrace on macOS)
        let threads = match self.task.get_threads() {
            Ok(t) if !t.is_empty() => t,
            Ok(_) => return StepResult::Error("No threads found".into()),
            Err(e) => return StepResult::Error(e),
        };

        let state = match self.task.get_thread_state(threads[0]) {
            Ok(s) => s,
            Err(e) => return StepResult::Error(e),
        };

        #[cfg(target_arch = "x86_64")]
        let (pc, sp) = (state.rip, state.rsp);
        #[cfg(target_arch = "aarch64")]
        let (pc, sp) = (state.pc, state.sp);

        if self.last_sp == 0 {
            self.last_sp = sp;
        }

        // 2. Read instruction bytes at PC and disassemble
        let code = match self.task.read_memory(pc, 16) {
            Ok(c) => c,
            Err(e) => return StepResult::Error(format!("Read at PC 0x{:x}: {}", pc, e)),
        };

        let insns = match self.cs.disasm_all(&code, pc) {
            Ok(i) => i,
            Err(e) => return StepResult::Error(format!("Disasm at 0x{:x}: {}", pc, e)),
        };

        let insn = match insns.iter().next() {
            Some(i) => i,
            None => return StepResult::Error(format!("No instruction at 0x{:x}", pc)),
        };

        let insn_bytes = insn.bytes().to_vec();
        let mnemonic = insn.mnemonic().unwrap_or("");
        let operands = insn.op_str().unwrap_or("");
        let insn_text = if operands.is_empty() {
            mnemonic.to_string()
        } else {
            format!("{} {}", mnemonic, operands)
        };

        // 3. Detect call/return instructions
        let mut annotation = String::new();
        #[cfg(target_arch = "x86_64")]
        {
            if mnemonic == "call" || mnemonic == "callq" {
                self.call_depth += 1;
                annotation = format!("CALL [depth:{}]", self.call_depth);
            } else if mnemonic == "ret" || mnemonic == "retq" {
                if self.call_depth > 0 {
                    self.call_depth -= 1;
                }
                annotation = format!("RETURN [depth:{}]", self.call_depth);
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            if matches!(mnemonic, "bl" | "blr" | "blraa" | "blrab") {
                self.call_depth += 1;
                annotation = format!("CALL [depth:{}]", self.call_depth);
            } else if matches!(mnemonic, "ret" | "retab" | "retaa") {
                if self.call_depth > 0 {
                    self.call_depth -= 1;
                }
                annotation = format!("RETURN [depth:{}]", self.call_depth);
            }
        }

        // 4. Detect memory changes (stack region)
        let mut mem_changes = Vec::new();
        if sp != self.last_sp || is_store_mnemonic(mnemonic) {
            let check_size: usize = 256;
            let stack_base = sp.saturating_sub(128);
            if let Ok(new_data) = self.task.read_memory(stack_base, check_size) {
                if let Some(old_data) = self.memory_cache.get(&stack_base) {
                    let len = new_data.len().min(old_data.len());
                    for i in 0..len {
                        if new_data[i] != old_data[i] {
                            mem_changes.push(MemChange {
                                addr: stack_base + i as u64,
                                old_val: old_data[i],
                                new_val: new_data[i],
                            });
                        }
                    }
                }
                self.memory_cache.insert(stack_base, new_data);
            }
        }

        // 5. Build register state JSON
        #[cfg(target_arch = "x86_64")]
        let regs = serde_json::json!({
            "rax": state.rax, "rbx": state.rbx, "rcx": state.rcx, "rdx": state.rdx,
            "rdi": state.rdi, "rsi": state.rsi, "rbp": state.rbp, "rsp": state.rsp,
            "r8": state.r8, "r9": state.r9, "r10": state.r10, "r11": state.r11,
            "r12": state.r12, "r13": state.r13, "r14": state.r14, "r15": state.r15,
            "rip": state.rip, "rflags": state.rflags,
        });

        #[cfg(target_arch = "aarch64")]
        let regs = serde_json::json!({
            "x0": state.x[0], "x1": state.x[1], "x2": state.x[2], "x3": state.x[3],
            "x4": state.x[4], "x5": state.x[5], "x6": state.x[6], "x7": state.x[7],
            "x8": state.x[8], "x9": state.x[9], "x10": state.x[10], "x11": state.x[11],
            "x12": state.x[12], "x13": state.x[13], "x14": state.x[14], "x15": state.x[15],
            "x16": state.x[16], "x17": state.x[17], "x18": state.x[18], "x19": state.x[19],
            "x20": state.x[20], "x21": state.x[21], "x22": state.x[22], "x23": state.x[23],
            "x24": state.x[24], "x25": state.x[25], "x26": state.x[26], "x27": state.x[27],
            "x28": state.x[28], "fp": state.fp, "lr": state.lr, "sp": state.sp,
            "pc": state.pc, "cpsr": state.cpsr,
        });

        let full_text = if annotation.is_empty() {
            insn_text
        } else {
            format!("{} ; {}", insn_text, annotation)
        };

        let entry = TraceEntry {
            step: self.step_count,
            pc,
            insn_bytes,
            insn_text: full_text,
            regs: regs.to_string(),
            mem_changes,
        };

        if let Err(e) = self.db.insert(entry.clone()) {
            return StepResult::Error(e);
        }
        self.step_count += 1;
        self.last_sp = sp;

        // 6. Execute exactly one instruction via ptrace PT_STEP
        let nix_pid = Pid::from_raw(self.pid);
        let sig = self.pending_signal.take();
        if let Err(e) = ptrace::step(nix_pid, sig) {
            return StepResult::Error(format!("ptrace(PT_STEP) failed: {}", e));
        }

        // 7. Wait for the process to stop after executing one instruction
        match waitpid(nix_pid, None) {
            Ok(WaitStatus::Stopped(_, Signal::SIGTRAP)) => {
                // Normal single-step completion
                StepResult::Ok(entry)
            }
            Ok(WaitStatus::Stopped(_, sig)) => {
                // Process received a different signal while stepping.
                // Save it for re-delivery on the next step.
                self.pending_signal = Some(sig);
                StepResult::Ok(entry)
            }
            Ok(WaitStatus::Exited(_, code)) => StepResult::ProcessExited(code),
            Ok(WaitStatus::Signaled(_, _sig, _)) => StepResult::ProcessExited(-1),
            Ok(status) => StepResult::Error(format!("Unexpected wait status: {:?}", status)),
            Err(e) => StepResult::Error(format!("waitpid: {}", e)),
        }
    }

    /// Detach from the traced process, allowing it to continue freely.
    pub fn detach(&self) {
        let _ = ptrace::detach(Pid::from_raw(self.pid), None);
    }

    pub fn db(&self) -> Arc<TraceDb> {
        self.db.clone()
    }

    pub fn step_count(&self) -> u64 {
        self.step_count
    }
}

fn is_store_mnemonic(m: &str) -> bool {
    matches!(
        m,
        "str" | "stp" | "stur" | "stlr" | "stxr" | "strb" | "strh" | "push" | "pushq"
    ) || m.starts_with("st")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ARM64 store mnemonics (all should return true) ──

    #[test]
    fn store_str() {
        assert!(is_store_mnemonic("str"));
    }

    #[test]
    fn store_stp() {
        assert!(is_store_mnemonic("stp"));
    }

    #[test]
    fn store_stur() {
        assert!(is_store_mnemonic("stur"));
    }

    #[test]
    fn store_stlr() {
        assert!(is_store_mnemonic("stlr"));
    }

    #[test]
    fn store_stxr() {
        assert!(is_store_mnemonic("stxr"));
    }

    #[test]
    fn store_strb() {
        assert!(is_store_mnemonic("strb"));
    }

    #[test]
    fn store_strh() {
        assert!(is_store_mnemonic("strh"));
    }

    #[test]
    fn store_stlxr() {
        // stlxr (store-release exclusive) starts with "st", should match
        assert!(is_store_mnemonic("stlxr"));
    }

    #[test]
    fn store_sttr() {
        // sttr (store unprivileged) starts with "st"
        assert!(is_store_mnemonic("sttr"));
    }

    #[test]
    fn store_stnp() {
        // stnp (store pair non-temporal) starts with "st"
        assert!(is_store_mnemonic("stnp"));
    }

    // ── x86_64 store mnemonics ──

    #[test]
    fn store_push() {
        assert!(is_store_mnemonic("push"));
    }

    #[test]
    fn store_pushq() {
        assert!(is_store_mnemonic("pushq"));
    }

    // ── Negative cases: non-store mnemonics should return false ──

    #[test]
    fn not_store_mov() {
        assert!(!is_store_mnemonic("mov"), "mov is not a store");
    }

    #[test]
    fn not_store_add() {
        assert!(!is_store_mnemonic("add"), "add is not a store");
    }

    #[test]
    fn not_store_sub() {
        assert!(!is_store_mnemonic("sub"), "sub is not a store");
    }

    #[test]
    fn not_store_ldr() {
        assert!(!is_store_mnemonic("ldr"), "ldr is a load, not a store");
    }

    #[test]
    fn not_store_ldp() {
        assert!(!is_store_mnemonic("ldp"), "ldp is a load, not a store");
    }

    #[test]
    fn not_store_nop() {
        assert!(!is_store_mnemonic("nop"));
    }

    #[test]
    fn not_store_ret() {
        assert!(!is_store_mnemonic("ret"));
    }

    #[test]
    fn not_store_bl() {
        assert!(!is_store_mnemonic("bl"), "bl is a call, not a store");
    }

    #[test]
    fn not_store_b() {
        assert!(!is_store_mnemonic("b"), "b is a branch, not a store");
    }

    #[test]
    fn not_store_pop() {
        assert!(!is_store_mnemonic("pop"), "pop is a load from stack");
    }

    #[test]
    fn not_store_svc() {
        // "svc" does NOT start with "st", so should be false
        assert!(!is_store_mnemonic("svc"));
    }

    #[test]
    fn not_store_empty() {
        assert!(!is_store_mnemonic(""));
    }

    #[test]
    fn not_store_single_s() {
        assert!(!is_store_mnemonic("s"));
    }
}
