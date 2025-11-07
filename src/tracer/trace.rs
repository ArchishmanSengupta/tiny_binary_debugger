use crate::tracer::mach::MachTask;
use crate::storage::{TraceDb, TraceEntry, MemChange};
use capstone::prelude::*;
use std::sync::Arc;
use std::collections::HashMap;

// Trait to abstract register access for both architectures
trait MemoryState {
    fn get_sp(&self) -> u64;
    #[cfg(target_arch = "aarch64")]
    fn get_x0(&self) -> u64;
    #[cfg(target_arch = "aarch64")]
    fn get_x1(&self) -> u64;
    #[cfg(target_arch = "x86_64")]
    fn get_rax(&self) -> u64;
    #[cfg(target_arch = "x86_64")]
    fn get_rbx(&self) -> u64;
    #[cfg(target_arch = "x86_64")]
    fn get_rsp(&self) -> u64;
}

#[cfg(target_arch = "x86_64")]
impl MemoryState for crate::tracer::mach::x86_thread_state64_t {
    fn get_sp(&self) -> u64 { self.rsp }
    fn get_rax(&self) -> u64 { self.rax }
    fn get_rbx(&self) -> u64 { self.rbx }
    fn get_rsp(&self) -> u64 { self.rsp }
}

#[cfg(target_arch = "aarch64")]
impl MemoryState for crate::tracer::mach::arm_thread_state64_t {
    fn get_sp(&self) -> u64 { self.sp }
    fn get_x0(&self) -> u64 { self.x[0] }
    fn get_x1(&self) -> u64 { self.x[1] }
}

pub struct Tracer {
    task: MachTask,
    db: Arc<TraceDb>,
    cs: Capstone,
    step_count: u64,
    memory_cache: HashMap<u64, Vec<u8>>,
    last_sp: u64,
    call_depth: u64,
}

impl Tracer {
    pub fn new(pid: i32, db_path: &str) -> Result<Self, String> {
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
            db,
            cs,
            step_count: 0,
            memory_cache: HashMap::new(),
            last_sp: 0,
            call_depth: 0,
        })
    }

    pub fn single_step(&mut self) -> Result<TraceEntry, String> {
        self.task.suspend()?;
        
        let threads = self.task.get_threads()?;
        if threads.is_empty() {
            return Err("No threads".into());
        }
        
        let state = self.task.get_thread_state(threads[0])?;
        
        #[cfg(target_arch = "x86_64")]
        let (pc, sp) = (state.rip, state.rsp);
        #[cfg(target_arch = "aarch64")]
        let (pc, sp) = (state.pc, state.sp);
        
        // Initialize last_sp on first step
        if self.last_sp == 0 {
            self.last_sp = sp;
        }
        
        let code = self.task.read_memory(pc, 16)?;
        let insns = self.cs.disasm_all(&code, pc)
            .map_err(|e| format!("Disasm failed: {}", e))?;
        
        let insn = insns.iter().next().ok_or("No instruction")?;
        let insn_bytes = insn.bytes().to_vec();
        let mnemonic = insn.mnemonic().unwrap_or("");
        let operands = insn.op_str().unwrap_or("");
        let insn_text = format!("{} {}", mnemonic, operands);
        
        // Detect function calls and returns
        let mut insn_type = String::new();
        #[cfg(target_arch = "x86_64")]
        {
            if mnemonic == "call" {
                self.call_depth += 1;
                insn_type = format!("CALL [depth:{}]", self.call_depth);
            } else if mnemonic == "ret" {
                if self.call_depth > 0 { self.call_depth -= 1; }
                insn_type = format!("RETURN [depth:{}]", self.call_depth);
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            if mnemonic == "bl" || mnemonic == "blr" {
                self.call_depth += 1;
                insn_type = format!("CALL [depth:{}]", self.call_depth);
            } else if mnemonic == "ret" {
                if self.call_depth > 0 { self.call_depth -= 1; }
                insn_type = format!("RETURN [depth:{}]", self.call_depth);
            }
        }
        
        // Track memory changes by checking stack and common memory regions
        let mut mem_changes = Vec::new();
        
        // Monitor stack changes (check 256 bytes around SP)
        if sp != self.last_sp || mnemonic.contains("str") || mnemonic.contains("st") 
           || mnemonic.contains("push") || mnemonic.contains("pop") {
            let stack_check_size = 256;
            let stack_base = sp.saturating_sub(128);
            
            if let Ok(new_data) = self.task.read_memory(stack_base, stack_check_size) {
                if let Some(old_data) = self.memory_cache.get(&stack_base) {
                    for i in 0..new_data.len().min(old_data.len()) {
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
        
        // Track heap writes (for store instructions, try to read target address)
        if mnemonic.contains("str") || mnemonic.contains("st") || mnemonic.contains("mov") {
            // Try to extract memory address from operands
            if let Some(addr) = self.extract_memory_address(&state, &operands) {
                if let Ok(new_val) = self.task.read_memory(addr, 1) {
                    if let Some(old_data) = self.memory_cache.get(&(addr & !0xFF)) {
                        let offset = (addr & 0xFF) as usize;
                        if offset < old_data.len() && !new_val.is_empty() && new_val[0] != old_data[offset] {
                            mem_changes.push(MemChange {
                                addr,
                                old_val: old_data[offset],
                                new_val: new_val[0],
                            });
                        }
                    }
                }
            }
        }
        
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
        
        // Add instruction type to the text if it's a call/return
        let full_insn_text = if !insn_type.is_empty() {
            format!("{} ; {}", insn_text, insn_type)
        } else {
            insn_text
        };
        
        let entry = TraceEntry {
            step: self.step_count,
            pc,
            insn_bytes,
            insn_text: full_insn_text,
            regs: regs.to_string(),
            mem_changes,
        };
        
        self.db.insert(entry.clone())?;
        self.step_count += 1;
        self.last_sp = sp;
        
        self.task.resume()?;
        std::thread::sleep(std::time::Duration::from_micros(100));
        
        Ok(entry)
    }
    
    fn extract_memory_address(&self, state: &impl MemoryState, operands: &str) -> Option<u64> {
        // Simple parser for ARM64/x86_64 memory operands like [x0], [rax+0x10], etc.
        #[cfg(target_arch = "aarch64")]
        {
            if operands.contains("[x0]") { return Some(state.get_x0()); }
            if operands.contains("[x1]") { return Some(state.get_x1()); }
            if operands.contains("[sp") { return Some(state.get_sp()); }
        }
        #[cfg(target_arch = "x86_64")]
        {
            if operands.contains("[rax]") { return Some(state.get_rax()); }
            if operands.contains("[rbx]") { return Some(state.get_rbx()); }
            if operands.contains("[rsp") { return Some(state.get_rsp()); }
        }
        None
    }

    pub fn db(&self) -> Arc<TraceDb> {
        self.db.clone()
    }
}

