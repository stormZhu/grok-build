use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::{Mutex, Notify};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClientState {
    Pending,
    Initializing,
    Ready(&'static str),
}

struct MiniMcpClient {
    id: u64,
    state: Mutex<ClientState>,
    init_done: Notify,
    handshake_started: Notify,
    allow_handshake: Notify,
    handshake_count: AtomicUsize,
}

impl MiniMcpClient {
    fn new(id: u64) -> Arc<Self> {
        Arc::new(Self {
            id,
            state: Mutex::new(ClientState::Pending),
            init_done: Notify::new(),
            handshake_started: Notify::new(),
            allow_handshake: Notify::new(),
            handshake_count: AtomicUsize::new(0),
        })
    }

    async fn ensure_initialized(self: &Arc<Self>) -> &'static str {
        loop {
            let notified = self.init_done.notified();
            tokio::pin!(notified);
            let mut state = self.state.lock().await;
            match *state {
                ClientState::Ready(service) => return service,
                ClientState::Initializing => {
                    drop(state);
                    notified.as_mut().await;
                }
                ClientState::Pending => {
                    *state = ClientState::Initializing;
                    break;
                }
            }
        }

        let mut guard = InitGuard {
            client: self.clone(),
            armed: true,
        };
        self.handshake_count.fetch_add(1, Ordering::SeqCst);
        self.handshake_started.notify_waiters();
        self.allow_handshake.notified().await;

        guard.armed = false;
        *self.state.lock().await = ClientState::Ready("tools-service");
        self.init_done.notify_waiters();
        "tools-service"
    }
}

struct InitGuard {
    client: Arc<MiniMcpClient>,
    armed: bool,
}

impl Drop for InitGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Ok(mut state) = self.client.state.try_lock()
            && *state == ClientState::Initializing
        {
            *state = ClientState::Pending;
        }
        self.client.init_done.notify_waiters();
    }
}

#[derive(Default)]
struct ClientPool {
    clients: BTreeMap<&'static str, Arc<MiniMcpClient>>,
}

impl ClientPool {
    fn replace(&mut self, server: &'static str, client: Arc<MiniMcpClient>) {
        self.clients.insert(server, client);
    }

    fn handle_transport_closed(&mut self, server: &str, client_id: u64) -> bool {
        let is_current = self
            .clients
            .get(server)
            .is_some_and(|client| client.id == client_id);
        if is_current {
            self.clients.remove(server);
        }
        is_current
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client = MiniMcpClient::new(1);
    let started = client.handshake_started.notified();
    tokio::pin!(started);
    let first_client = client.clone();
    let first = tokio::spawn(async move { first_client.ensure_initialized().await });
    started.as_mut().await;

    let second_client = client.clone();
    let second = tokio::spawn(async move { second_client.ensure_initialized().await });
    tokio::task::yield_now().await;
    assert_eq!(*client.state.lock().await, ClientState::Initializing);
    assert_eq!(client.handshake_count.load(Ordering::SeqCst), 1);

    client.allow_handshake.notify_one();
    assert_eq!(first.await.unwrap(), "tools-service");
    assert_eq!(second.await.unwrap(), "tools-service");
    assert_eq!(client.handshake_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        *client.state.lock().await,
        ClientState::Ready("tools-service")
    );

    let cancelled = MiniMcpClient::new(2);
    let started = cancelled.handshake_started.notified();
    tokio::pin!(started);
    let holder_client = cancelled.clone();
    let holder = tokio::spawn(async move { holder_client.ensure_initialized().await });
    started.as_mut().await;
    holder.abort();
    assert!(holder.await.unwrap_err().is_cancelled());
    assert_eq!(*cancelled.state.lock().await, ClientState::Pending);

    let retry_started = cancelled.handshake_started.notified();
    tokio::pin!(retry_started);
    let retry_client = cancelled.clone();
    let retry = tokio::spawn(async move { retry_client.ensure_initialized().await });
    retry_started.as_mut().await;
    cancelled.allow_handshake.notify_one();
    assert_eq!(retry.await.unwrap(), "tools-service");
    assert_eq!(cancelled.handshake_count.load(Ordering::SeqCst), 2);

    let old = MiniMcpClient::new(10);
    let replacement = MiniMcpClient::new(11);
    let mut pool = ClientPool::default();
    pool.replace("docs", old);
    pool.replace("docs", replacement.clone());
    assert!(!pool.handle_transport_closed("docs", 10));
    assert_eq!(pool.clients["docs"].id, 11);
    assert!(pool.handle_transport_closed("docs", 11));
    assert!(!pool.clients.contains_key("docs"));

    println!("one handshake owner; cancellation restores Pending; stale close is ignored");
}
