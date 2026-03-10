use crate::storage::TraceDb;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceStats {
    pub total_steps: u64,
    pub unique_addresses: usize,
    pub instruction_counts: Vec<(String, usize)>,
    pub most_executed_address: Option<(u64, usize)>,
    pub call_count: usize,
    pub ret_count: usize,
    pub jump_count: usize,
    pub mem_change_count: usize,
}

/// Returns true if the mnemonic is a branch/jump (not a call).
///
/// ARM64 branches: b, b.cond, br, cbz, cbnz, tbz, tbnz
/// x86_64 branches: j* (jmp, je, jne, jz, jnz, jg, jge, jl, jle, ja, jb, etc.)
fn is_branch_mnemonic(m: &str) -> bool {
    // x86_64: all jumps start with 'j'
    if m.starts_with('j') {
        return true;
    }
    // ARM64: unconditional branch, branch-to-register, conditional branches
    if m == "b" || m == "br" || m.starts_with("b.") {
        return true;
    }
    // ARM64: compare-and-branch, test-bit-and-branch
    matches!(m, "cbz" | "cbnz" | "tbz" | "tbnz")
}

impl TraceStats {
    pub fn analyze(db: &TraceDb) -> Self {
        let entries = db.get_all();
        let total_steps = entries.len() as u64;

        let mut addr_counts: HashMap<u64, usize> = HashMap::new();
        let mut insn_counts: HashMap<String, usize> = HashMap::new();
        let mut call_count = 0;
        let mut ret_count = 0;
        let mut jump_count = 0;
        let mut mem_change_count = 0;

        for entry in &entries {
            *addr_counts.entry(entry.pc).or_insert(0) += 1;

            let mnemonic = entry.insn_text.split_whitespace().next().unwrap_or("");
            *insn_counts.entry(mnemonic.to_string()).or_insert(0) += 1;

            if entry.insn_text.contains("CALL") {
                call_count += 1;
            }
            if entry.insn_text.contains("RETURN") {
                ret_count += 1;
            }
            if is_branch_mnemonic(mnemonic) {
                jump_count += 1;
            }
            mem_change_count += entry.mem_changes.len();
        }

        let unique_addresses = addr_counts.len();

        let most_executed_address = addr_counts
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(&addr, &count)| (addr, count));

        let mut instruction_counts: Vec<_> = insn_counts.into_iter().collect();
        instruction_counts.sort_by(|a, b| b.1.cmp(&a.1));
        instruction_counts.truncate(20);

        Self {
            total_steps,
            unique_addresses,
            instruction_counts,
            most_executed_address,
            call_count,
            ret_count,
            jump_count,
            mem_change_count,
        }
    }

    pub fn print(&self) {
        println!("  Trace Statistics");
        println!("  ----------------");
        println!("  Total steps:      {}", self.total_steps);
        println!("  Unique addresses: {}", self.unique_addresses);
        println!("  Function calls:   {}", self.call_count);
        println!("  Returns:          {}", self.ret_count);
        println!("  Jumps/branches:   {}", self.jump_count);
        println!("  Memory changes:   {}", self.mem_change_count);

        if let Some((addr, count)) = self.most_executed_address {
            println!("  Most executed:    0x{:x} ({} times)", addr, count);
        }

        println!("\n  Top instructions:");
        for (insn, count) in &self.instruction_counts {
            println!("    {:12} {:>8}", insn, count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{MemChange, TraceEntry};

    fn entry(step: u64, pc: u64, insn: &str) -> TraceEntry {
        TraceEntry {
            step,
            pc,
            insn_bytes: vec![0x00],
            insn_text: insn.to_string(),
            regs: r#"{"x0":0,"sp":4096}"#.to_string(),
            mem_changes: vec![],
        }
    }

    fn entry_with_mem(step: u64, pc: u64, insn: &str, n: usize) -> TraceEntry {
        let changes: Vec<MemChange> = (0..n)
            .map(|i| MemChange {
                addr: 0x1000 + i as u64,
                old_val: 0,
                new_val: i as u8,
            })
            .collect();
        TraceEntry {
            step,
            pc,
            insn_bytes: vec![0x00],
            insn_text: insn.to_string(),
            regs: r#"{"x0":0,"sp":4096}"#.to_string(),
            mem_changes: changes,
        }
    }

    fn db_with(entries: Vec<TraceEntry>) -> TraceDb {
        let db = TraceDb::new(":memory:").unwrap();
        for e in entries {
            db.insert(e).unwrap();
        }
        db
    }

    #[test]
    fn empty_trace_gives_all_zeros() {
        let db = db_with(vec![]);
        let s = TraceStats::analyze(&db);
        assert_eq!(s.total_steps, 0);
        assert_eq!(s.unique_addresses, 0);
        assert_eq!(s.call_count, 0);
        assert_eq!(s.ret_count, 0);
        assert_eq!(s.jump_count, 0);
        assert_eq!(s.mem_change_count, 0);
        assert!(s.most_executed_address.is_none());
        assert!(s.instruction_counts.is_empty());
    }

    #[test]
    fn single_step_counted() {
        let db = db_with(vec![entry(0, 0x1000, "mov x0, #1")]);
        let s = TraceStats::analyze(&db);
        assert_eq!(s.total_steps, 1);
        assert_eq!(s.unique_addresses, 1);
    }

    #[test]
    fn call_annotation_counted() {
        let db = db_with(vec![
            entry(0, 0x1000, "bl #0x2000 ; CALL [depth:1]"),
            entry(1, 0x2000, "mov x0, #0"),
            entry(2, 0x2004, "bl #0x3000 ; CALL [depth:2]"),
        ]);
        let s = TraceStats::analyze(&db);
        assert_eq!(s.call_count, 2);
    }

    #[test]
    fn return_annotation_counted() {
        let db = db_with(vec![
            entry(0, 0x1000, "ret ; RETURN [depth:0]"),
            entry(1, 0x2000, "mov x0, #0"),
            entry(2, 0x2004, "ret ; RETURN [depth:0]"),
        ]);
        let s = TraceStats::analyze(&db);
        assert_eq!(s.ret_count, 2);
    }

    #[test]
    fn call_and_return_are_independent_counts() {
        let db = db_with(vec![
            entry(0, 0x1000, "bl #0x2000 ; CALL [depth:1]"),
            entry(1, 0x2000, "ret ; RETURN [depth:0]"),
        ]);
        let s = TraceStats::analyze(&db);
        assert_eq!(s.call_count, 1);
        assert_eq!(s.ret_count, 1);
    }

    #[test]
    fn x86_jumps_counted() {
        let db = db_with(vec![
            entry(0, 0x1000, "jmp 0x2000"),
            entry(1, 0x2000, "je 0x3000"),
            entry(2, 0x3000, "jne 0x4000"),
            entry(3, 0x4000, "mov eax, 1"),
        ]);
        let s = TraceStats::analyze(&db);
        assert_eq!(s.jump_count, 3);
    }

    #[test]
    fn arm64_branches_counted() {
        let db = db_with(vec![
            entry(0, 0x1000, "b #0x2000"),
            entry(1, 0x2000, "b.eq #0x3000"),
            entry(2, 0x3000, "br x16"),
            entry(3, 0x4000, "cbz x0, #0x5000"),
            entry(4, 0x5000, "cbnz x1, #0x6000"),
            entry(5, 0x6000, "tbz x2, #3, #0x7000"),
            entry(6, 0x7000, "tbnz x3, #5, #0x8000"),
        ]);
        let s = TraceStats::analyze(&db);
        assert_eq!(s.jump_count, 7);
    }

    #[test]
    fn bl_blr_not_counted_as_jumps() {
        // bl and blr are calls, not jumps
        let db = db_with(vec![
            entry(0, 0x1000, "bl #0x2000 ; CALL [depth:1]"),
            entry(1, 0x2000, "blr x8 ; CALL [depth:2]"),
        ]);
        let s = TraceStats::analyze(&db);
        assert_eq!(s.jump_count, 0, "bl/blr should not be counted as jumps");
    }

    #[test]
    fn bic_bfm_not_counted_as_jumps() {
        // bic (bit clear), bfm (bitfield move) are NOT branches
        let db = db_with(vec![
            entry(0, 0x1000, "bic x0, x1, x2"),
            entry(1, 0x1004, "bfm x0, x1, #2, #5"),
            entry(2, 0x1008, "bfi x3, x4, #0, #8"),
            entry(3, 0x100c, "bfxil x5, x6, #0, #16"),
        ]);
        let s = TraceStats::analyze(&db);
        assert_eq!(
            s.jump_count, 0,
            "bic/bfm/bfi/bfxil should not be counted as jumps"
        );
    }

    #[test]
    fn memory_changes_summed() {
        let db = db_with(vec![
            entry_with_mem(0, 0x1000, "str x0, [sp]", 3),
            entry(1, 0x1004, "mov x1, #0"),
            entry_with_mem(2, 0x1008, "stp x0, x1, [sp, #-16]!", 5),
        ]);
        let s = TraceStats::analyze(&db);
        assert_eq!(s.mem_change_count, 8);
    }

    #[test]
    fn unique_addresses_with_repeats() {
        let db = db_with(vec![
            entry(0, 0x1000, "mov x0, #1"),
            entry(1, 0x1004, "mov x1, #2"),
            entry(2, 0x1000, "mov x0, #1"), // same PC as step 0
            entry(3, 0x1008, "mov x2, #3"),
            entry(4, 0x1000, "mov x0, #1"), // same PC again
        ]);
        let s = TraceStats::analyze(&db);
        assert_eq!(s.unique_addresses, 3);
    }

    #[test]
    fn most_executed_address_is_correct() {
        let db = db_with(vec![
            entry(0, 0xA, "nop"),
            entry(1, 0xB, "nop"),
            entry(2, 0xA, "nop"),
            entry(3, 0xA, "nop"),
            entry(4, 0xB, "nop"),
        ]);
        let s = TraceStats::analyze(&db);
        let (addr, count) = s.most_executed_address.unwrap();
        assert_eq!(addr, 0xA);
        assert_eq!(count, 3);
    }

    #[test]
    fn instruction_counts_sorted_descending() {
        let db = db_with(vec![
            entry(0, 0x1000, "mov x0, #1"),
            entry(1, 0x1004, "add x0, x0, #1"),
            entry(2, 0x1008, "mov x1, #2"),
            entry(3, 0x100c, "add x0, x0, #2"),
            entry(4, 0x1010, "add x0, x0, #3"),
        ]);
        let s = TraceStats::analyze(&db);
        // "add" appears 3 times, "mov" 2 times
        assert_eq!(s.instruction_counts[0].0, "add");
        assert_eq!(s.instruction_counts[0].1, 3);
        assert_eq!(s.instruction_counts[1].0, "mov");
        assert_eq!(s.instruction_counts[1].1, 2);
    }

    #[test]
    fn instruction_counts_truncated_to_20() {
        let entries: Vec<TraceEntry> = (0..25)
            .map(|i| {
                let mnemonic = format!("insn{} x0, #1", i);
                entry(i as u64, 0x1000 + i as u64 * 4, &mnemonic)
            })
            .collect();
        let db = db_with(entries);
        let s = TraceStats::analyze(&db);
        assert!(
            s.instruction_counts.len() <= 20,
            "instruction_counts should be truncated to 20, got {}",
            s.instruction_counts.len()
        );
    }

    // ── is_branch_mnemonic unit tests ──

    #[test]
    fn branch_mnemonic_x86_jmp() {
        assert!(is_branch_mnemonic("jmp"));
    }

    #[test]
    fn branch_mnemonic_x86_conditional() {
        for m in &[
            "je", "jne", "jz", "jnz", "jg", "jge", "jl", "jle", "ja", "jb", "jbe", "jae",
        ] {
            assert!(is_branch_mnemonic(m), "{} should be a branch", m);
        }
    }

    #[test]
    fn branch_mnemonic_arm64_unconditional() {
        assert!(is_branch_mnemonic("b"));
        assert!(is_branch_mnemonic("br"));
    }

    #[test]
    fn branch_mnemonic_arm64_conditional() {
        for m in &[
            "b.eq", "b.ne", "b.cs", "b.cc", "b.mi", "b.pl", "b.vs", "b.vc", "b.hi", "b.ls", "b.ge",
            "b.lt", "b.gt", "b.le", "b.al",
        ] {
            assert!(is_branch_mnemonic(m), "{} should be a branch", m);
        }
    }

    #[test]
    fn branch_mnemonic_arm64_compare_branch() {
        assert!(is_branch_mnemonic("cbz"));
        assert!(is_branch_mnemonic("cbnz"));
        assert!(is_branch_mnemonic("tbz"));
        assert!(is_branch_mnemonic("tbnz"));
    }

    #[test]
    fn not_branch_calls() {
        assert!(!is_branch_mnemonic("bl"), "bl is a call, not a branch");
        assert!(!is_branch_mnemonic("blr"), "blr is a call, not a branch");
        assert!(!is_branch_mnemonic("blraa"));
        assert!(!is_branch_mnemonic("blrab"));
    }

    #[test]
    fn not_branch_alu_starting_with_b() {
        assert!(!is_branch_mnemonic("bic"), "bic is bit-clear, not a branch");
        assert!(
            !is_branch_mnemonic("bfm"),
            "bfm is bitfield move, not a branch"
        );
        assert!(!is_branch_mnemonic("bfi"));
        assert!(!is_branch_mnemonic("bfxil"));
    }

    #[test]
    fn not_branch_common_insns() {
        for m in &[
            "mov", "add", "sub", "ldr", "str", "nop", "ret", "svc", "adr", "adrp",
        ] {
            assert!(!is_branch_mnemonic(m), "{} should not be a branch", m);
        }
    }
}
