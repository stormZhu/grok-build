use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
struct JournalEntry {
    seq: u64,
    kind: String,
    request_hash: String,
    result: Value,
}

#[derive(Debug, PartialEq, Eq)]
enum JournalError {
    Sequence { expected: u64, actual: u64 },
    Divergence { seq: u64, kind: String },
}

#[derive(Default)]
struct Journal {
    entries: Vec<JournalEntry>,
}

impl Journal {
    fn replay(
        &self,
        seq: u64,
        kind: &str,
        request_hash: &str,
    ) -> Result<Option<Value>, JournalError> {
        let Some(entry) = usize::try_from(seq)
            .ok()
            .and_then(|index| self.entries.get(index))
        else {
            return Ok(None);
        };
        if entry.seq != seq || entry.kind != kind || entry.request_hash != request_hash {
            return Err(JournalError::Divergence {
                seq,
                kind: kind.to_owned(),
            });
        }
        Ok(Some(entry.result.clone()))
    }

    fn record(
        &mut self,
        seq: u64,
        kind: &str,
        request_hash: String,
        result: Value,
    ) -> Result<(), JournalError> {
        let expected = self.entries.len() as u64;
        if seq != expected {
            return Err(JournalError::Sequence {
                expected,
                actual: seq,
            });
        }
        self.entries.push(JournalEntry {
            seq,
            kind: kind.to_owned(),
            request_hash,
            result,
        });
        Ok(())
    }
}

fn request_hash(kind: &str, payload: &Value) -> String {
    // Deterministic FNV-1a is sufficient for this miniature; production uses SHA-256.
    let bytes = [kind.as_bytes(), &[0], payload.to_string().as_bytes()].concat();
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn call_host(
    journal: &mut Journal,
    seq: u64,
    kind: &str,
    payload: &Value,
    host_executions: &mut usize,
) -> Result<(Value, bool), JournalError> {
    let hash = request_hash(kind, payload);
    if let Some(result) = journal.replay(seq, kind, &hash)? {
        return Ok((result, true));
    }

    *host_executions += 1;
    let result = serde_json::json!({
        "agent_id": format!("child-{}", *host_executions),
        "success": true
    });
    journal.record(seq, kind, hash, result.clone())?;
    Ok((result, false))
}

#[derive(Debug, PartialEq, Eq)]
struct AgentCallBudget {
    maximum: u64,
    reserved: u64,
}

impl AgentCallBudget {
    fn reserve(&mut self, count: u64) -> Result<(), &'static str> {
        let requested = self.reserved.saturating_add(count);
        if requested > self.maximum {
            return Err("agent-call quota exceeded");
        }
        self.reserved = requested;
        Ok(())
    }

    fn release(&mut self, count: u64) {
        self.reserved = self.reserved.saturating_sub(count);
    }
}

fn main() {
    let payload = serde_json::json!({ "prompt": "inspect auth", "model": "fast" });
    let mut journal = Journal::default();
    let mut host_executions = 0;

    let (first, replayed) = call_host(
        &mut journal,
        0,
        "spawn_agent",
        &payload,
        &mut host_executions,
    )
    .unwrap();
    assert!(!replayed);
    assert_eq!(host_executions, 1);
    assert_eq!(journal.entries.len(), 1);

    let (restored, replayed) = call_host(
        &mut journal,
        0,
        "spawn_agent",
        &payload,
        &mut host_executions,
    )
    .unwrap();
    assert!(replayed);
    assert_eq!(restored, first);
    assert_eq!(host_executions, 1);

    let changed = serde_json::json!({ "prompt": "different prompt", "model": "fast" });
    assert_eq!(
        call_host(
            &mut journal,
            0,
            "spawn_agent",
            &changed,
            &mut host_executions,
        )
        .unwrap_err(),
        JournalError::Divergence {
            seq: 0,
            kind: "spawn_agent".to_owned()
        }
    );
    assert_eq!(host_executions, 1);

    assert_eq!(
        journal
            .record(2, "budget", "hash".to_owned(), Value::Null)
            .unwrap_err(),
        JournalError::Sequence {
            expected: 1,
            actual: 2
        }
    );
    assert_eq!(journal.entries.len(), 1);

    let mut budget = AgentCallBudget {
        maximum: 3,
        reserved: 0,
    };
    budget.reserve(2).unwrap();
    assert_eq!(budget.reserved, 2);
    assert_eq!(budget.reserve(2).unwrap_err(), "agent-call quota exceeded");
    assert_eq!(budget.reserved, 2);
    budget.release(1);
    budget.reserve(2).unwrap();
    assert_eq!(budget.reserved, 3);

    println!("dense journal + request hash + replay without side effects + atomic budget");
}
