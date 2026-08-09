use std::collections::{BTreeMap, BTreeSet};

const COALESCE_WINDOW_MS: u64 = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum EventKind {
    TransportClosed,
    HandshakeFailed,
    ToolsChanged,
    Ready,
    ConfigAdded,
    ConfigRemoved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    TransportClosed {
        server: &'static str,
        client_id: u64,
    },
    HandshakeFailed {
        server: &'static str,
        reason: &'static str,
    },
    ToolsChanged {
        server: &'static str,
        revision: u64,
    },
    Ready {
        server: &'static str,
    },
    ConfigAdded {
        server: &'static str,
    },
    ConfigRemoved {
        server: &'static str,
    },
    ConfigDiff {
        added: Vec<&'static str>,
        removed: Vec<&'static str>,
    },
}

impl Event {
    fn server(&self) -> Option<&'static str> {
        match self {
            Self::TransportClosed { server, .. }
            | Self::HandshakeFailed { server, .. }
            | Self::ToolsChanged { server, .. }
            | Self::Ready { server }
            | Self::ConfigAdded { server }
            | Self::ConfigRemoved { server } => Some(server),
            Self::ConfigDiff { .. } => None,
        }
    }

    fn kind(&self) -> EventKind {
        match self {
            Self::TransportClosed { .. } => EventKind::TransportClosed,
            Self::HandshakeFailed { .. } => EventKind::HandshakeFailed,
            Self::ToolsChanged { .. } => EventKind::ToolsChanged,
            Self::Ready { .. } => EventKind::Ready,
            Self::ConfigAdded { .. } => EventKind::ConfigAdded,
            Self::ConfigRemoved { .. } => EventKind::ConfigRemoved,
            Self::ConfigDiff { .. } => unreachable!("ConfigDiff must fan out before keying"),
        }
    }
}

#[derive(Debug, Default)]
struct Window {
    buf: BTreeMap<(&'static str, EventKind), Event>,
    closed: BTreeMap<&'static str, BTreeSet<u64>>,
}

impl Window {
    fn insert(&mut self, event: Event) {
        match event {
            Event::ConfigDiff { added, removed } => {
                for server in added {
                    self.insert(Event::ConfigAdded { server });
                }
                for server in removed {
                    self.insert(Event::ConfigRemoved { server });
                }
            }
            Event::TransportClosed { server, client_id } => {
                self.closed.entry(server).or_default().insert(client_id);
                self.buf.insert(
                    (server, EventKind::TransportClosed),
                    Event::TransportClosed { server, client_id },
                );
            }
            event => {
                let server = event.server().unwrap();
                self.buf.insert((server, event.kind()), event);
            }
        }
    }
}

fn collect_window(events: &[(u64, Event)], from: usize) -> (Window, usize) {
    let mut window = Window::default();
    let deadline = events[from].0 + COALESCE_WINDOW_MS;
    let mut index = from;
    while index < events.len() && events[index].0 < deadline {
        window.insert(events[index].1.clone());
        index += 1;
    }
    (window, index)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transport {
    Stdio,
    Http,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Client {
    id: u64,
    transport: Transport,
}

fn remove_dead_stdio(
    window: &mut Window,
    clients: &mut BTreeMap<&'static str, Client>,
) -> Vec<&'static str> {
    let mut stale = Vec::new();
    for (&server, closed_ids) in &window.closed {
        let Some(current) = clients.get(server).copied() else {
            continue;
        };
        if current.transport == Transport::Http {
            continue;
        }
        if closed_ids.contains(&current.id) {
            clients.remove(server);
        } else {
            stale.push(server);
        }
    }
    for server in &stale {
        window.buf.remove(&(*server, EventKind::TransportClosed));
    }
    stale
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Recovery {
    RestartStdio,
    ResetHttp,
}

fn recovery_for(server: &str, clients: &BTreeMap<&'static str, Client>) -> Recovery {
    match clients.get(server).map(|client| client.transport) {
        Some(Transport::Http) => Recovery::ResetHttp,
        Some(Transport::Stdio) | None => Recovery::RestartStdio,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Ready,
    Initializing,
    Unavailable,
    NeedsAuth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reason {
    Initialized,
    ConfigAdded,
    ConfigRemoved,
    ConfigChanged,
    TransportClosed,
    HandshakeFailed,
    AuthExpired,
}

fn payload(event: &Event) -> (Status, Reason) {
    match event {
        Event::Ready { .. } => (Status::Ready, Reason::Initialized),
        Event::ConfigAdded { .. } => (Status::Initializing, Reason::ConfigAdded),
        Event::ConfigRemoved { .. } => (Status::Unavailable, Reason::ConfigRemoved),
        Event::ToolsChanged { .. } => (Status::Ready, Reason::ConfigChanged),
        Event::TransportClosed { .. } => (Status::Unavailable, Reason::TransportClosed),
        Event::HandshakeFailed { server, reason }
            if server.starts_with("grok_com_")
                && (reason.contains("401") || reason.contains("unauthorized")) =>
        {
            (Status::NeedsAuth, Reason::AuthExpired)
        }
        Event::HandshakeFailed { .. } => (Status::Unavailable, Reason::HandshakeFailed),
        Event::ConfigDiff { .. } => unreachable!("ConfigDiff is not a wire event"),
    }
}

#[derive(Default)]
struct ShutdownState {
    shutting_down: BTreeSet<&'static str>,
}

impl ShutdownState {
    fn observe(&mut self, event: &Event) {
        match event {
            Event::ConfigRemoved { server } => {
                self.shutting_down.insert(server);
            }
            Event::Ready { server } => {
                self.shutting_down.remove(server);
            }
            _ => {}
        }
    }
}

fn main() {
    let events = [
        (
            0,
            Event::ToolsChanged {
                server: "github",
                revision: 1,
            },
        ),
        (
            10,
            Event::ToolsChanged {
                server: "github",
                revision: 2,
            },
        ),
        (
            20,
            Event::ConfigDiff {
                added: vec!["new"],
                removed: vec!["old"],
            },
        ),
        (60, Event::Ready { server: "github" }),
    ];
    let (first, next) = collect_window(&events, 0);
    assert_eq!(next, 3);
    assert_eq!(first.buf.len(), 3);
    assert!(matches!(
        first.buf.get(&("github", EventKind::ToolsChanged)),
        Some(Event::ToolsChanged { revision: 2, .. })
    ));
    assert!(first.buf.contains_key(&("new", EventKind::ConfigAdded)));
    assert!(first.buf.contains_key(&("old", EventKind::ConfigRemoved)));
    let (second, done) = collect_window(&events, next);
    assert_eq!(done, events.len());
    assert_eq!(second.buf.len(), 1);

    let mut close_window = Window::default();
    close_window.insert(Event::TransportClosed {
        server: "local",
        client_id: 7,
    });
    close_window.insert(Event::TransportClosed {
        server: "local",
        client_id: 6,
    });
    assert!(matches!(
        close_window.buf.get(&("local", EventKind::TransportClosed)),
        Some(Event::TransportClosed { client_id: 6, .. })
    ));
    assert_eq!(close_window.closed["local"], BTreeSet::from([6, 7]));

    let mut clients = BTreeMap::from([
        (
            "local",
            Client {
                id: 7,
                transport: Transport::Stdio,
            },
        ),
        (
            "remote",
            Client {
                id: 9,
                transport: Transport::Http,
            },
        ),
    ]);
    assert!(remove_dead_stdio(&mut close_window, &mut clients).is_empty());
    assert!(!clients.contains_key("local"));
    assert_eq!(recovery_for("local", &clients), Recovery::RestartStdio);
    assert_eq!(recovery_for("remote", &clients), Recovery::ResetHttp);

    let mut stale_window = Window::default();
    stale_window.insert(Event::TransportClosed {
        server: "replacement",
        client_id: 11,
    });
    clients.insert(
        "replacement",
        Client {
            id: 12,
            transport: Transport::Stdio,
        },
    );
    assert_eq!(
        remove_dead_stdio(&mut stale_window, &mut clients),
        ["replacement"]
    );
    assert!(clients.contains_key("replacement"));
    assert!(
        !stale_window
            .buf
            .contains_key(&("replacement", EventKind::TransportClosed))
    );

    assert_eq!(
        payload(&Event::HandshakeFailed {
            server: "grok_com_linear",
            reason: "401 unauthorized",
        }),
        (Status::NeedsAuth, Reason::AuthExpired)
    );
    assert_eq!(
        payload(&Event::HandshakeFailed {
            server: "local",
            reason: "401 unauthorized",
        }),
        (Status::Unavailable, Reason::HandshakeFailed)
    );

    let mut shutdown = ShutdownState::default();
    let close = Event::TransportClosed {
        server: "local",
        client_id: 7,
    };
    shutdown.observe(&close);
    assert!(!shutdown.shutting_down.contains("local"));
    shutdown.observe(&Event::ConfigRemoved { server: "local" });
    assert!(shutdown.shutting_down.contains("local"));
    shutdown.observe(&Event::Ready { server: "local" });
    assert!(!shutdown.shutting_down.contains("local"));

    println!("50ms coalescing + close identities + stale eviction + recovery routing");
}
