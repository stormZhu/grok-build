use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileChange {
    before: Option<String>,
    after: Option<String>,
}

#[derive(Default)]
struct MemoryWorkspace {
    files: BTreeMap<String, String>,
    current_turn: BTreeMap<String, FileChange>,
    checkpoints: Vec<BTreeMap<String, FileChange>>,
    fail_writes: BTreeSet<String>,
}

impl MemoryWorkspace {
    fn edit(&mut self, path: &str, contents: &str) -> Result<(), String> {
        if !path.starts_with("/workspace/") {
            return Err(format!("permission denied: {path}"));
        }

        let before = self.files.get(path).cloned();
        self.current_turn
            .entry(path.to_owned())
            .or_insert(FileChange {
                before,
                after: None,
            });
        self.files.insert(path.to_owned(), contents.to_owned());
        Ok(())
    }

    fn finish_turn(&mut self) {
        for (path, change) in &mut self.current_turn {
            change.after = self.files.get(path).cloned();
        }
        self.checkpoints
            .push(std::mem::take(&mut self.current_turn));
    }

    fn write_snapshot(&mut self, path: &str, snapshot: &Option<String>) -> Result<(), String> {
        if self.fail_writes.contains(path) {
            return Err(format!("simulated write failure: {path}"));
        }
        match snapshot {
            Some(contents) => {
                self.files.insert(path.to_owned(), contents.clone());
            }
            None => {
                self.files.remove(path);
            }
        }
        Ok(())
    }

    fn rewind_last(&mut self) -> Result<Vec<String>, String> {
        let checkpoint = self
            .checkpoints
            .last()
            .cloned()
            .ok_or_else(|| "no checkpoint".to_owned())?;
        let mut conflicts = Vec::new();

        for (path, change) in checkpoint {
            if self.files.get(&path) != change.after.as_ref() {
                conflicts.push(path.clone());
            }
            self.write_snapshot(&path, &change.before)?;
        }

        self.checkpoints.pop();
        Ok(conflicts)
    }
}

fn main() {
    let mut workspace = MemoryWorkspace::default();
    workspace
        .files
        .insert("/workspace/app.rs".to_owned(), "v1".to_owned());

    let files_before_denial = workspace.files.clone();
    assert_eq!(
        workspace.edit("/etc/passwd", "blocked").unwrap_err(),
        "permission denied: /etc/passwd"
    );
    assert_eq!(workspace.files, files_before_denial);
    assert!(workspace.current_turn.is_empty());

    workspace.edit("/workspace/app.rs", "v2").unwrap();
    workspace.edit("/workspace/app.rs", "v2-final").unwrap();
    assert_eq!(
        workspace.current_turn["/workspace/app.rs"]
            .before
            .as_deref(),
        Some("v1")
    );
    workspace.finish_turn();
    assert_eq!(
        workspace.checkpoints[0]["/workspace/app.rs"]
            .after
            .as_deref(),
        Some("v2-final")
    );

    workspace
        .files
        .insert("/workspace/app.rs".to_owned(), "v3-external".to_owned());
    let conflicts = workspace.rewind_last().unwrap();
    assert_eq!(conflicts, ["/workspace/app.rs"]);
    assert_eq!(workspace.files["/workspace/app.rs"], "v1");
    assert!(workspace.checkpoints.is_empty());

    workspace.edit("/workspace/app.rs", "v4").unwrap();
    workspace.finish_turn();
    workspace.fail_writes.insert("/workspace/app.rs".to_owned());
    assert_eq!(
        workspace.rewind_last().unwrap_err(),
        "simulated write failure: /workspace/app.rs"
    );
    assert_eq!(workspace.files["/workspace/app.rs"], "v4");
    assert_eq!(workspace.checkpoints.len(), 1);

    workspace.fail_writes.clear();
    assert!(workspace.rewind_last().unwrap().is_empty());
    assert_eq!(workspace.files["/workspace/app.rs"], "v1");
    assert!(workspace.checkpoints.is_empty());

    println!("permission before write; conflict reported; checkpoints clear only after restore");
}
