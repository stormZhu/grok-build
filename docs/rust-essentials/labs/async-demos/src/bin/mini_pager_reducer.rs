use std::collections::HashMap;

type EntryId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, PartialEq, Eq)]
enum RenderBlock {
    User {
        text: String,
    },
    Agent {
        stream: u64,
        text: String,
        running: bool,
    },
    Tool {
        call_id: &'static str,
        title: &'static str,
        output: String,
        status: ToolStatus,
    },
}

#[derive(Debug, PartialEq, Eq)]
struct Entry {
    id: EntryId,
    block: RenderBlock,
}

#[derive(Clone, Copy, Debug)]
enum Update {
    UserChunk(&'static str),
    AgentChunk {
        stream: u64,
        text: &'static str,
    },
    ToolStart {
        call_id: &'static str,
        title: &'static str,
    },
    ToolDelta {
        call_id: &'static str,
        output: &'static str,
        status: ToolStatus,
    },
}

#[derive(Debug)]
struct PagerState {
    entries: Vec<Entry>,
    next_id: EntryId,
    following: bool,
    scroll_at_bottom: bool,
}

impl Default for PagerState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            following: true,
            scroll_at_bottom: true,
        }
    }
}

impl PagerState {
    fn push(&mut self, block: RenderBlock) -> EntryId {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(Entry { id, block });
        if self.following {
            self.scroll_at_bottom = true;
        }
        id
    }

    fn get_mut(&mut self, id: EntryId) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|entry| entry.id == id)
    }

    fn user_scrolled_up(&mut self) {
        self.following = false;
        self.scroll_at_bottom = false;
    }

    fn return_to_bottom(&mut self) {
        self.following = true;
        self.scroll_at_bottom = true;
    }
}

#[derive(Default)]
struct Tracker {
    current_agent: Option<(u64, EntryId)>,
    pending_tools: HashMap<&'static str, EntryId>,
    orphan_tool_updates: HashMap<&'static str, (&'static str, ToolStatus)>,
    skip_next_user_echo: bool,
}

impl Tracker {
    fn expect_user_echo(&mut self) {
        self.skip_next_user_echo = true;
    }

    fn finish_agent(&mut self, pager: &mut PagerState) {
        if let Some((_, id)) = self.current_agent.take()
            && let Some(entry) = pager.get_mut(id)
            && let RenderBlock::Agent { running, .. } = &mut entry.block
        {
            *running = false;
        }
    }

    fn apply(&mut self, update: Update, pager: &mut PagerState) -> bool {
        match update {
            Update::UserChunk(text) => {
                self.finish_agent(pager);
                if self.skip_next_user_echo {
                    self.skip_next_user_echo = false;
                    return false;
                }
                pager.push(RenderBlock::User {
                    text: text.to_owned(),
                });
                true
            }
            Update::AgentChunk { stream, text } => {
                if text.is_empty() || (self.current_agent.is_none() && text.trim().is_empty()) {
                    return false;
                }
                if self
                    .current_agent
                    .is_some_and(|(current_stream, _)| current_stream != stream)
                {
                    self.finish_agent(pager);
                }
                let id = match self.current_agent {
                    Some((_, id)) => id,
                    None => {
                        let id = pager.push(RenderBlock::Agent {
                            stream,
                            text: String::new(),
                            running: true,
                        });
                        self.current_agent = Some((stream, id));
                        id
                    }
                };
                let entry = pager.get_mut(id).unwrap();
                let RenderBlock::Agent { text: body, .. } = &mut entry.block else {
                    unreachable!()
                };
                body.push_str(text);
                true
            }
            Update::ToolStart { call_id, title } => {
                self.finish_agent(pager);
                let id = pager.push(RenderBlock::Tool {
                    call_id,
                    title,
                    output: String::new(),
                    status: ToolStatus::Running,
                });
                self.pending_tools.insert(call_id, id);
                if let Some((output, status)) = self.orphan_tool_updates.remove(call_id) {
                    self.update_tool(call_id, output, status, pager);
                }
                true
            }
            Update::ToolDelta {
                call_id,
                output,
                status,
            } => {
                if self.pending_tools.contains_key(call_id) {
                    self.update_tool(call_id, output, status, pager);
                    true
                } else {
                    self.orphan_tool_updates.insert(call_id, (output, status));
                    false
                }
            }
        }
    }

    fn update_tool(
        &mut self,
        call_id: &'static str,
        output: &'static str,
        status: ToolStatus,
        pager: &mut PagerState,
    ) {
        let id = self.pending_tools[call_id];
        let entry = pager.get_mut(id).unwrap();
        let RenderBlock::Tool {
            output: body,
            status: current,
            ..
        } = &mut entry.block
        else {
            unreachable!()
        };
        body.push_str(output);
        *current = status;
        if status != ToolStatus::Running {
            self.pending_tools.remove(call_id);
        }
    }

    fn finish_turn(&mut self, pager: &mut PagerState) {
        self.finish_agent(pager);
        for (_, id) in self.pending_tools.drain() {
            if let Some(entry) = pager.get_mut(id)
                && let RenderBlock::Tool { status, .. } = &mut entry.block
            {
                *status = ToolStatus::Failed;
            }
        }
        self.orphan_tool_updates.clear();
    }
}

fn main() {
    let mut pager = PagerState::default();
    let mut tracker = Tracker::default();

    let local_prompt_id = pager.push(RenderBlock::User {
        text: "Explain the reducer".to_owned(),
    });
    tracker.expect_user_echo();
    assert!(!tracker.apply(Update::UserChunk("Explain the reducer"), &mut pager));
    assert_eq!(pager.entries.len(), 1);
    assert_eq!(pager.entries[0].id, local_prompt_id);

    tracker.apply(
        Update::AgentChunk {
            stream: 10,
            text: "Hello",
        },
        &mut pager,
    );
    tracker.apply(
        Update::AgentChunk {
            stream: 10,
            text: " world",
        },
        &mut pager,
    );
    assert_eq!(pager.entries.len(), 2);
    assert!(matches!(
        &pager.entries[1].block,
        RenderBlock::Agent {
            text,
            running: true,
            ..
        } if text == "Hello world"
    ));

    tracker.apply(
        Update::ToolStart {
            call_id: "tool-a",
            title: "read",
        },
        &mut pager,
    );
    tracker.apply(
        Update::ToolStart {
            call_id: "tool-b",
            title: "search",
        },
        &mut pager,
    );
    tracker.apply(
        Update::ToolDelta {
            call_id: "tool-a",
            output: "A",
            status: ToolStatus::Completed,
        },
        &mut pager,
    );
    assert!(matches!(
        &pager.entries[2].block,
        RenderBlock::Tool {
            call_id: "tool-a",
            output,
            status: ToolStatus::Completed,
            ..
        } if output == "A"
    ));
    assert!(matches!(
        &pager.entries[3].block,
        RenderBlock::Tool {
            call_id: "tool-b",
            status: ToolStatus::Running,
            ..
        }
    ));

    assert!(!tracker.apply(
        Update::ToolDelta {
            call_id: "tool-race",
            output: "already done",
            status: ToolStatus::Completed,
        },
        &mut pager,
    ));
    tracker.apply(
        Update::ToolStart {
            call_id: "tool-race",
            title: "late start",
        },
        &mut pager,
    );
    assert!(matches!(
        &pager.entries[4].block,
        RenderBlock::Tool {
            call_id: "tool-race",
            output,
            status: ToolStatus::Completed,
            ..
        } if output == "already done"
    ));

    pager.user_scrolled_up();
    tracker.apply(
        Update::AgentChunk {
            stream: 11,
            text: "New stream",
        },
        &mut pager,
    );
    assert!(!pager.following);
    assert!(!pager.scroll_at_bottom);
    pager.return_to_bottom();

    tracker.finish_turn(&mut pager);
    assert!(matches!(
        &pager.entries[3].block,
        RenderBlock::Tool {
            call_id: "tool-b",
            status: ToolStatus::Failed,
            ..
        }
    ));
    assert!(matches!(
        &pager.entries[5].block,
        RenderBlock::Agent { running: false, .. }
    ));
    assert!(pager.following);
    assert!(pager.scroll_at_bottom);

    println!("stream merge + optimistic echo + tool ID routing + follow state");
}
