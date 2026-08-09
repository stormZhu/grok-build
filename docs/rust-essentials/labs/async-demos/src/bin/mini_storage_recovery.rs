use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailureStage {
    None,
    Write,
    Summary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppendError {
    NotCommitted,
    Committed,
}

#[derive(Default)]
struct Storage {
    updates_jsonl: String,
    summary_count: usize,
    pending: Vec<Value>,
    durable_chat: Vec<Value>,
    visible_chat_snapshot: Vec<Value>,
    temporary_snapshot: Option<Vec<Value>>,
}

impl Storage {
    fn append_update(&mut self, value: &Value, failure: FailureStage) -> Result<(), AppendError> {
        if failure == FailureStage::Write {
            return Err(AppendError::NotCommitted);
        }
        if !self.updates_jsonl.is_empty() && !self.updates_jsonl.ends_with('\n') {
            self.updates_jsonl.push('\n');
        }
        self.updates_jsonl.push_str(&value.to_string());
        self.updates_jsonl.push('\n');
        if failure == FailureStage::Summary {
            return Err(AppendError::Committed);
        }
        self.summary_count += 1;
        Ok(())
    }

    fn enqueue_chat(&mut self, item: Value) {
        self.pending.push(item);
    }

    fn flush_and_ack(&mut self) {
        self.durable_chat.append(&mut self.pending);
    }

    fn recover_updates(&self) -> Vec<Value> {
        self.updates_jsonl
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    fn stage_chat_replace(&mut self, replacement: Vec<Value>) {
        self.temporary_snapshot = Some(replacement);
    }

    fn commit_chat_replace(&mut self) {
        self.visible_chat_snapshot = self.temporary_snapshot.take().unwrap();
    }

    fn abandon_chat_replace(&mut self) {
        self.temporary_snapshot = None;
    }
}

fn main() {
    let mut storage = Storage::default();

    storage.enqueue_chat(json!({"role": "user", "text": "hello"}));
    assert!(storage.durable_chat.is_empty());
    storage.flush_and_ack();
    assert_eq!(storage.durable_chat.len(), 1);
    assert!(storage.pending.is_empty());

    let first = json!({"kind": "user", "prompt": 1});
    assert_eq!(
        storage.append_update(&first, FailureStage::Write),
        Err(AppendError::NotCommitted)
    );
    assert!(storage.updates_jsonl.is_empty());
    storage.append_update(&first, FailureStage::None).unwrap();

    let committed = json!({"kind": "assistant", "prompt": 1});
    assert_eq!(
        storage.append_update(&committed, FailureStage::Summary),
        Err(AppendError::Committed)
    );
    assert_eq!(storage.summary_count, 1);

    storage.updates_jsonl.push_str("{\"kind\":\"torn\"");
    storage
        .append_update(
            &json!({"kind": "turn_completed", "prompt": 1}),
            FailureStage::None,
        )
        .unwrap();
    let recovered = storage.recover_updates();
    assert_eq!(recovered.len(), 3);
    assert_eq!(recovered[0], first);
    assert_eq!(recovered[1], committed);
    assert_eq!(recovered[2]["kind"], "turn_completed");

    storage.visible_chat_snapshot = vec![json!({"role": "user", "text": "old"})];
    storage.stage_chat_replace(vec![json!({"role": "summary", "text": "new"})]);
    storage.abandon_chat_replace();
    assert_eq!(storage.visible_chat_snapshot[0]["text"], "old");
    storage.stage_chat_replace(vec![json!({"role": "summary", "text": "new"})]);
    storage.commit_chat_replace();
    assert_eq!(storage.visible_chat_snapshot[0]["text"], "new");
    assert_eq!(storage.recover_updates().len(), 3);

    println!("flush ack + committed boundary + torn-tail recovery + atomic snapshot replace");
}
