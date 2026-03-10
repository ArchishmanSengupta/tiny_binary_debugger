use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemChange {
    pub addr: u64,
    pub old_val: u8,
    pub new_val: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

impl std::fmt::Debug for TraceDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TraceDb")
            .field("path", &self.path)
            .field("count", &self.entries.read().len())
            .finish()
    }
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
        self.entries
            .read()
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
        let data = bincode::serialize(&*entries).map_err(|e| format!("Serialize failed: {}", e))?;
        std::fs::write(&self.path, data).map_err(|e| format!("Write failed: {}", e))?;
        Ok(())
    }

    pub fn load(path: &str) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("Read failed: {}", e))?;
        let entries: BTreeMap<u64, TraceEntry> =
            bincode::deserialize(&data).map_err(|e| format!("Deserialize failed: {}", e))?;
        Ok(Self {
            entries: Arc::new(RwLock::new(entries)),
            path: path.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a TraceEntry with sensible defaults.
    fn make_entry(step: u64, pc: u64, insn: &str) -> TraceEntry {
        TraceEntry {
            step,
            pc,
            insn_bytes: vec![0xAA, 0xBB],
            insn_text: insn.to_string(),
            regs: serde_json::json!({"x0": step, "sp": 0x7000}).to_string(),
            mem_changes: vec![],
        }
    }

    fn make_entry_with_mem(step: u64, changes: Vec<MemChange>) -> TraceEntry {
        TraceEntry {
            step,
            pc: 0x1000 + step * 4,
            insn_bytes: vec![0xCC],
            insn_text: "str x0, [sp]".to_string(),
            regs: serde_json::json!({"x0": 42, "sp": 0x7000}).to_string(),
            mem_changes: changes,
        }
    }

    // ── creation ──

    #[test]
    fn new_creates_empty_db() {
        let db = TraceDb::new(":memory:").unwrap();
        assert_eq!(db.count(), 0);
        assert!(db.get_all().is_empty());
    }

    // ── insert / get ──

    #[test]
    fn insert_and_get_roundtrip() {
        let db = TraceDb::new(":memory:").unwrap();
        let e = make_entry(0, 0x1000, "nop");
        db.insert(e.clone()).unwrap();

        let got = db.get(0).expect("should exist");
        assert_eq!(got, e);
    }

    #[test]
    fn get_missing_returns_none() {
        let db = TraceDb::new(":memory:").unwrap();
        assert!(db.get(0).is_none());
        assert!(db.get(999).is_none());
    }

    #[test]
    fn insert_preserves_all_fields() {
        let db = TraceDb::new(":memory:").unwrap();
        let e = TraceEntry {
            step: 42,
            pc: 0xDEAD_BEEF,
            insn_bytes: vec![0x01, 0x02, 0x03, 0x04],
            insn_text: "stp x29, x30, [sp, #-16]! ; CALL [depth:3]".to_string(),
            regs: serde_json::json!({
                "x0": 100, "x1": 200, "sp": 0x6FF0, "pc": 0xDEAD_BEEFu64
            })
            .to_string(),
            mem_changes: vec![
                MemChange {
                    addr: 0x6FF0,
                    old_val: 0x00,
                    new_val: 0x41,
                },
                MemChange {
                    addr: 0x6FF1,
                    old_val: 0xFF,
                    new_val: 0x42,
                },
            ],
        };
        db.insert(e.clone()).unwrap();

        let got = db.get(42).unwrap();
        assert_eq!(got.step, 42);
        assert_eq!(got.pc, 0xDEAD_BEEF);
        assert_eq!(got.insn_bytes, vec![0x01, 0x02, 0x03, 0x04]);
        assert!(got.insn_text.contains("stp"));
        assert!(got.insn_text.contains("CALL [depth:3]"));
        assert_eq!(got.mem_changes.len(), 2);
        assert_eq!(got.mem_changes[0].addr, 0x6FF0);
        assert_eq!(got.mem_changes[0].new_val, 0x41);
        assert_eq!(got.mem_changes[1].old_val, 0xFF);

        // Verify regs JSON round-trips correctly
        let regs: serde_json::Value = serde_json::from_str(&got.regs).unwrap();
        assert_eq!(regs["x0"], 100);
        assert_eq!(regs["sp"], 0x6FF0);
    }

    #[test]
    fn count_matches_inserts() {
        let db = TraceDb::new(":memory:").unwrap();
        for i in 0..50 {
            db.insert(make_entry(i, 0x1000 + i * 4, "nop")).unwrap();
        }
        assert_eq!(db.count(), 50);
    }

    #[test]
    fn overwrite_same_step_replaces() {
        let db = TraceDb::new(":memory:").unwrap();
        db.insert(make_entry(0, 0x1000, "first")).unwrap();
        db.insert(make_entry(0, 0x2000, "second")).unwrap();

        assert_eq!(db.count(), 1, "overwrite should not increase count");
        let got = db.get(0).unwrap();
        assert_eq!(got.insn_text, "second");
        assert_eq!(got.pc, 0x2000);
    }

    // ── range queries ──

    #[test]
    fn get_range_returns_inclusive() {
        let db = TraceDb::new(":memory:").unwrap();
        for i in 0..10 {
            db.insert(make_entry(i, 0x1000 + i * 4, "nop")).unwrap();
        }
        let range = db.get_range(3, 7);
        assert_eq!(range.len(), 5);
        assert_eq!(range[0].step, 3);
        assert_eq!(range[4].step, 7);
    }

    #[test]
    fn get_range_empty_when_no_match() {
        let db = TraceDb::new(":memory:").unwrap();
        db.insert(make_entry(0, 0x1000, "nop")).unwrap();
        db.insert(make_entry(1, 0x1004, "nop")).unwrap();
        let range = db.get_range(100, 200);
        assert!(range.is_empty());
    }

    #[test]
    fn get_range_partial_overlap() {
        let db = TraceDb::new(":memory:").unwrap();
        for i in 5..10 {
            db.insert(make_entry(i, 0x1000, "nop")).unwrap();
        }
        // Request range 0..7 but only 5,6,7 exist
        let range = db.get_range(0, 7);
        assert_eq!(range.len(), 3);
        assert_eq!(range[0].step, 5);
        assert_eq!(range[2].step, 7);
    }

    // ── get_all ordering ──

    #[test]
    fn get_all_returns_in_step_order() {
        let db = TraceDb::new(":memory:").unwrap();
        // Insert out of order
        db.insert(make_entry(5, 0x1000, "five")).unwrap();
        db.insert(make_entry(2, 0x1004, "two")).unwrap();
        db.insert(make_entry(8, 0x1008, "eight")).unwrap();
        db.insert(make_entry(0, 0x100c, "zero")).unwrap();

        let all = db.get_all();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].step, 0);
        assert_eq!(all[1].step, 2);
        assert_eq!(all[2].step, 5);
        assert_eq!(all[3].step, 8);
    }

    // ── save / load ──

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.tdb");
        let path_str = path.to_str().unwrap();

        let db = TraceDb::new(path_str).unwrap();
        let entries = vec![
            make_entry(0, 0x1000, "mov x0, #1"),
            make_entry(1, 0x1004, "add x0, x0, #2"),
            make_entry(2, 0x1008, "ret ; RETURN [depth:0]"),
        ];
        for e in &entries {
            db.insert(e.clone()).unwrap();
        }
        db.save().unwrap();

        let loaded = TraceDb::load(path_str).unwrap();
        assert_eq!(loaded.count(), 3);

        for e in &entries {
            let got = loaded.get(e.step).expect("entry should exist after load");
            assert_eq!(got, *e);
        }
    }

    #[test]
    fn save_and_load_empty_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.tdb");
        let path_str = path.to_str().unwrap();

        let db = TraceDb::new(path_str).unwrap();
        db.save().unwrap();

        let loaded = TraceDb::load(path_str).unwrap();
        assert_eq!(loaded.count(), 0);
    }

    #[test]
    fn save_and_load_preserves_mem_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memtest.tdb");
        let path_str = path.to_str().unwrap();

        let changes = vec![
            MemChange {
                addr: 0xAAAA,
                old_val: 0x00,
                new_val: 0x41,
            },
            MemChange {
                addr: 0xBBBB,
                old_val: 0xFF,
                new_val: 0x00,
            },
        ];
        let db = TraceDb::new(path_str).unwrap();
        db.insert(make_entry_with_mem(0, changes.clone())).unwrap();
        db.save().unwrap();

        let loaded = TraceDb::load(path_str).unwrap();
        let got = loaded.get(0).unwrap();
        assert_eq!(got.mem_changes, changes);
    }

    #[test]
    fn load_nonexistent_file_fails() {
        let result = TraceDb::load("/tmp/this_file_does_not_exist_tdb_test.tdb");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Read failed"),
            "Error should mention read failure: {}",
            err
        );
    }

    #[test]
    fn load_corrupt_file_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.tdb");
        std::fs::write(&path, b"this is not valid bincode data at all!!!").unwrap();

        let result = TraceDb::load(path.to_str().unwrap());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Deserialize failed"),
            "Error should mention deserialization: {}",
            err
        );
    }

    #[test]
    fn large_db_insert_and_retrieve() {
        let db = TraceDb::new(":memory:").unwrap();
        let n = 10_000u64;
        for i in 0..n {
            db.insert(make_entry(i, 0x1000 + i * 4, "nop")).unwrap();
        }
        assert_eq!(db.count(), n);

        // Spot-check various positions
        assert_eq!(db.get(0).unwrap().step, 0);
        assert_eq!(db.get(n / 2).unwrap().step, n / 2);
        assert_eq!(db.get(n - 1).unwrap().step, n - 1);
        assert!(db.get(n).is_none());
    }

    #[test]
    fn concurrent_reads_dont_deadlock() {
        let db = Arc::new(TraceDb::new(":memory:").unwrap());
        for i in 0..100 {
            db.insert(make_entry(i, 0x1000, "nop")).unwrap();
        }

        let mut handles = vec![];
        for _ in 0..8 {
            let db = db.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..1000 {
                    let _ = db.count();
                    let _ = db.get(50);
                    let _ = db.get_all();
                    let _ = db.get_range(10, 20);
                }
            }));
        }
        for h in handles {
            h.join().expect("thread should not panic");
        }
    }

    // ── data model serialization ──

    #[test]
    fn trace_entry_json_roundtrip() {
        let e = make_entry(7, 0xCAFE, "add x0, x1, x2");
        let json = serde_json::to_string(&e).unwrap();
        let back: TraceEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn trace_entry_bincode_roundtrip() {
        let e = make_entry(99, 0xBEEF, "sub sp, sp, #16");
        let bytes = bincode::serialize(&e).unwrap();
        let back: TraceEntry = bincode::deserialize(&bytes).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn mem_change_json_roundtrip() {
        let mc = MemChange {
            addr: 0x12345678,
            old_val: 0xAA,
            new_val: 0xBB,
        };
        let json = serde_json::to_string(&mc).unwrap();
        let back: MemChange = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mc);
    }

    #[test]
    fn trace_entry_with_large_regs_string() {
        let regs = serde_json::json!({
            "x0": u64::MAX, "x1": u64::MAX, "x2": u64::MAX, "x3": u64::MAX,
            "x4": u64::MAX, "x5": u64::MAX, "x6": u64::MAX, "x7": u64::MAX,
            "x28": u64::MAX, "fp": u64::MAX, "lr": u64::MAX, "sp": u64::MAX,
            "pc": u64::MAX, "cpsr": u32::MAX,
        });
        let e = TraceEntry {
            step: 0,
            pc: 0,
            insn_bytes: vec![],
            insn_text: "nop".into(),
            regs: regs.to_string(),
            mem_changes: vec![],
        };
        let bytes = bincode::serialize(&e).unwrap();
        let back: TraceEntry = bincode::deserialize(&bytes).unwrap();
        // Verify the regs JSON can be parsed back and contains correct max values
        let parsed: serde_json::Value = serde_json::from_str(&back.regs).unwrap();
        assert_eq!(parsed["x0"], u64::MAX);
        assert_eq!(parsed["sp"], u64::MAX);
    }
}
