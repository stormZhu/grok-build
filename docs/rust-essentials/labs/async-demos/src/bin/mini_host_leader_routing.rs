use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Interactive,
    Stdio,
    Headless,
    Leader,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClientMode {
    Stdio,
    Headless,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostRoute {
    EmbeddedSingleTurn,
    EmbeddedInteractive,
    DirectStdio,
    DirectRelay,
    LeaderClient(ClientMode),
    LeaderServer,
}

struct Args {
    has_single_prompt: bool,
    command: Command,
    use_leader: bool,
    has_session_auth: bool,
}

fn resolve_host(args: Args) -> Result<HostRoute, &'static str> {
    if args.has_single_prompt {
        return Ok(HostRoute::EmbeddedSingleTurn);
    }
    match (args.command, args.use_leader) {
        (Command::Interactive, false) => Ok(HostRoute::EmbeddedInteractive),
        (Command::Interactive, true) | (Command::Stdio, true) => {
            Ok(HostRoute::LeaderClient(ClientMode::Stdio))
        }
        (Command::Stdio, false) => Ok(HostRoute::DirectStdio),
        (Command::Headless, _) if !args.has_session_auth => {
            Err("headless relay requires session auth")
        }
        (Command::Headless, true) => Ok(HostRoute::LeaderClient(ClientMode::Headless)),
        (Command::Headless, false) => Ok(HostRoute::DirectRelay),
        (Command::Leader, _) => Ok(HostRoute::LeaderServer),
    }
}

fn namespace_request_id(json: &mut Value, client_id: u64) -> Option<String> {
    let original = json.get("id")?.clone();
    let namespaced = format!("{client_id}|{}", serde_json::to_string(&original).ok()?);
    json["id"] = Value::String(namespaced.clone());
    Some(namespaced)
}

fn route_response(json: &mut Value) -> Option<u64> {
    let namespaced = json.get("id")?.as_str()?.to_owned();
    let (client, original_json) = namespaced.split_once('|')?;
    let client_id = client.parse().ok()?;
    json["id"] = serde_json::from_str(original_json).ok()?;
    Some(client_id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Registration {
    code_navigation: bool,
}

#[derive(Default)]
struct Leader {
    ready: bool,
    registrations: BTreeMap<u64, Registration>,
    sessions: BTreeSet<&'static str>,
}

impl Leader {
    fn accept_request(&self) -> Result<(), &'static str> {
        self.ready.then_some(()).ok_or("leader_starting")
    }

    fn disconnect(&mut self, client_id: u64) {
        self.registrations.remove(&client_id);
    }
}

fn main() {
    assert_eq!(
        resolve_host(Args {
            has_single_prompt: false,
            command: Command::Interactive,
            use_leader: false,
            has_session_auth: false,
        }),
        Ok(HostRoute::EmbeddedInteractive)
    );
    assert_eq!(
        resolve_host(Args {
            has_single_prompt: true,
            command: Command::Headless,
            use_leader: true,
            has_session_auth: false,
        }),
        Ok(HostRoute::EmbeddedSingleTurn)
    );
    assert_eq!(
        resolve_host(Args {
            has_single_prompt: false,
            command: Command::Stdio,
            use_leader: true,
            has_session_auth: false,
        }),
        Ok(HostRoute::LeaderClient(ClientMode::Stdio))
    );
    assert_eq!(
        resolve_host(Args {
            has_single_prompt: false,
            command: Command::Stdio,
            use_leader: false,
            has_session_auth: false,
        }),
        Ok(HostRoute::DirectStdio)
    );
    assert_eq!(
        resolve_host(Args {
            has_single_prompt: false,
            command: Command::Headless,
            use_leader: false,
            has_session_auth: false,
        }),
        Err("headless relay requires session auth")
    );
    assert_eq!(
        resolve_host(Args {
            has_single_prompt: false,
            command: Command::Headless,
            use_leader: false,
            has_session_auth: true,
        }),
        Ok(HostRoute::DirectRelay)
    );
    assert_eq!(
        resolve_host(Args {
            has_single_prompt: false,
            command: Command::Headless,
            use_leader: true,
            has_session_auth: true,
        }),
        Ok(HostRoute::LeaderClient(ClientMode::Headless))
    );
    assert_eq!(
        resolve_host(Args {
            has_single_prompt: false,
            command: Command::Leader,
            use_leader: false,
            has_session_auth: true,
        }),
        Ok(HostRoute::LeaderServer)
    );

    let mut leader = Leader::default();
    leader.registrations.insert(
        7,
        Registration {
            code_navigation: true,
        },
    );
    leader.registrations.insert(
        8,
        Registration {
            code_navigation: false,
        },
    );
    leader.sessions.insert("session-shared");
    assert_eq!(leader.accept_request(), Err("leader_starting"));
    leader.ready = true;
    assert_eq!(leader.accept_request(), Ok(()));
    assert!(leader.registrations[&7].code_navigation);
    assert!(!leader.registrations[&8].code_navigation);

    let mut request_a = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "prompt" });
    let mut request_b = serde_json::json!({ "jsonrpc": "2.0", "id": "same", "method": "prompt" });
    assert_eq!(namespace_request_id(&mut request_a, 7).unwrap(), "7|1");
    assert_eq!(
        namespace_request_id(&mut request_b, 8).unwrap(),
        "8|\"same\""
    );

    let mut response_b = serde_json::json!({ "id": request_b["id"].clone(), "result": {} });
    let mut response_a = serde_json::json!({ "id": request_a["id"].clone(), "result": {} });
    assert_eq!(route_response(&mut response_b), Some(8));
    assert_eq!(response_b["id"], "same");
    assert_eq!(route_response(&mut response_a), Some(7));
    assert_eq!(response_a["id"], 1);

    leader.disconnect(7);
    assert!(!leader.registrations.contains_key(&7));
    assert!(leader.registrations.contains_key(&8));
    assert!(leader.sessions.contains("session-shared"));

    println!("entrypoint precedence + readiness + per-client capabilities + ID namespace");
}
