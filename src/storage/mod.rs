use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemChange {
    pub addr: u64,
    pub old_val: u8,
    pub new_val: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    pub step: u64,
    pub pc: u64,
    pub insn_bytes: Vec<u8>,
    pub insn_text: String,
    pub regs: String,
    pub mem_changes: Vec<MemChange>,
}

pub struct TraceDb {
    entries: Arc<RwLock<BTreeMap<u64, TraceEntry>>>,
    path: String,
}

impl TraceDb {
    pub fn new(path: &str) -> Result<Self, String> {
        Ok(Self {
            entries: Arc::new(RwLock::new(BTreeMap::new())),
            path: path.to_string(),
        })
    }

    pub fn insert(&self, entry: TraceEntry) -> Result<(), String> {
        self.entries.write().insert(entry.step, entry);
        Ok(())
    }

    pub fn get(&self, step: u64) -> Option<TraceEntry> {
        self.entries.read().get(&step).cloned()
    }

    pub fn get_range(&self, start: u64, end: u64) -> Vec<TraceEntry> {
        self.entries.read()
            .range(start..=end)
            .map(|(_, v)| v.clone())
            .collect()
    }

    pub fn get_all(&self) -> Vec<TraceEntry> {
        self.entries.read().values().cloned().collect()
    }

    pub fn count(&self) -> u64 {
        self.entries.read().len() as u64
    }

    pub fn save(&self) -> Result<(), String> {
        let entries = self.entries.read();
        let data = bincode::serialize(&*entries)
            .map_err(|e| format!("Serialize failed: {}", e))?;
        std::fs::write(&self.path, data)
            .map_err(|e| format!("Write failed: {}", e))?;
        Ok(())
    }

    pub fn load(path: &str) -> Result<Self, String> {
        let data = std::fs::read(path)
            .map_err(|e| format!("Read failed: {}", e))?;
        let entries: BTreeMap<u64, TraceEntry> = bincode::deserialize(&data)
            .map_err(|e| format!("Deserialize failed: {}", e))?;
        Ok(Self {
            entries: Arc::new(RwLock::new(entries)),
            path: path.to_string(),
        })
    }
}

