use crate::storage::TraceDb;
use std::collections::HashMap;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TraceStats {
    pub total_steps: u64,
    pub unique_addresses: usize,
    pub instruction_counts: Vec<(String, usize)>,
    pub most_executed_address: Option<(u64, usize)>,
    pub call_count: usize,
    pub ret_count: usize,
    pub jump_count: usize,
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

        for entry in &entries {
            *addr_counts.entry(entry.pc).or_insert(0) += 1;
            
            let mnemonic = entry.insn_text.split_whitespace().next().unwrap_or("");
            *insn_counts.entry(mnemonic.to_string()).or_insert(0) += 1;
            
            match mnemonic {
                "call" | "bl" => call_count += 1,
                "ret" | "retq" => ret_count += 1,
                s if s.starts_with('j') || s.starts_with('b') => jump_count += 1,
                _ => {}
            }
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
        }
    }

    pub fn print(&self) {
        println!("=== Trace Statistics ===");
        println!("Total steps: {}", self.total_steps);
        println!("Unique addresses: {}", self.unique_addresses);
        println!("Function calls: {}", self.call_count);
        println!("Returns: {}", self.ret_count);
        println!("Jumps: {}", self.jump_count);
        
        if let Some((addr, count)) = self.most_executed_address {
            println!("Most executed: 0x{:x} ({} times)", addr, count);
        }

        println!("\nTop instructions:");
        for (insn, count) in &self.instruction_counts {
            println!("  {:12} {}", insn, count);
        }
    }
}

