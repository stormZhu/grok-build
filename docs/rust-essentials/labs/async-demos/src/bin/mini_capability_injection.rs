use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Clone)]
struct Params<T>(T);

struct State<T>(T);

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkspaceRoot(&'static str);

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReadParams {
    max_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WebFetchClient {
    policy: &'static str,
}

struct ReadCount(AtomicUsize);

#[derive(Default)]
struct Resources {
    values: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl Resources {
    fn insert<T: Any + Send + Sync>(&mut self, value: T) {
        self.insert_shared(Arc::new(value));
    }

    fn insert_shared<T: Any + Send + Sync>(&mut self, value: Arc<T>) {
        self.values.insert(TypeId::of::<T>(), value);
    }

    fn get<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.values
            .get(&TypeId::of::<T>())
            .cloned()
            .and_then(|value| value.downcast::<T>().ok())
    }

    fn require<T: Any + Send + Sync>(&self, name: &'static str) -> Result<Arc<T>, String> {
        self.get::<T>()
            .ok_or_else(|| format!("missing resource: {name}"))
    }
}

struct ToolCallContext {
    call_id: &'static str,
    cancelled: Arc<AtomicBool>,
    ephemeral_credential: Option<&'static str>,
}

fn build_toolset(
    workspace: &'static str,
    web_enabled: bool,
    read_count: Arc<State<ReadCount>>,
) -> Arc<Resources> {
    let mut resources = Resources::default();
    resources.insert(WorkspaceRoot(workspace));
    resources.insert(Params(ReadParams { max_bytes: 4096 }));
    resources.insert_shared(read_count);
    if web_enabled {
        resources.insert(WebFetchClient {
            policy: "https-only",
        });
    }
    Arc::new(resources)
}

fn run_read(context: &ToolCallContext, resources: &Resources) -> Result<String, String> {
    if context.cancelled.load(Ordering::SeqCst) {
        return Err(format!("{} cancelled", context.call_id));
    }
    let root = resources.require::<WorkspaceRoot>("WorkspaceRoot")?;
    let params = resources.require::<Params<ReadParams>>("ReadParams")?;
    let state = resources.require::<State<ReadCount>>("ReadCount")?;
    state.0.0.fetch_add(1, Ordering::SeqCst);
    Ok(format!("{}:{}", root.0, params.0.max_bytes))
}

fn run_web(resources: &Resources) -> Result<&'static str, String> {
    let client = resources.require::<WebFetchClient>("WebFetchClient")?;
    Ok(client.policy)
}

fn main() {
    let persisted_state = Arc::new(State(ReadCount(AtomicUsize::new(0))));
    let first = build_toolset("/workspace/old", true, persisted_state.clone());
    let context = ToolCallContext {
        call_id: "call-1",
        cancelled: Arc::new(AtomicBool::new(false)),
        ephemeral_credential: Some("request-only-token"),
    };
    assert_eq!(run_read(&context, &first).unwrap(), "/workspace/old:4096");
    assert_eq!(run_web(&first).unwrap(), "https-only");
    assert_eq!(persisted_state.0.0.load(Ordering::SeqCst), 1);
    assert_eq!(context.ephemeral_credential, Some("request-only-token"));
    assert!(first.get::<String>().is_none());

    let rebuilt = build_toolset("/workspace/new", true, persisted_state.clone());
    assert_eq!(run_read(&context, &rebuilt).unwrap(), "/workspace/new:4096");
    assert_eq!(run_web(&rebuilt).unwrap(), "https-only");
    assert_eq!(persisted_state.0.0.load(Ordering::SeqCst), 2);
    assert_eq!(first.get::<WorkspaceRoot>().unwrap().0, "/workspace/old");
    assert_eq!(rebuilt.get::<WorkspaceRoot>().unwrap().0, "/workspace/new");

    let without_web = build_toolset("/workspace/new", false, persisted_state);
    assert_eq!(
        run_web(&without_web).unwrap_err(),
        "missing resource: WebFetchClient"
    );

    context.cancelled.store(true, Ordering::SeqCst);
    assert_eq!(
        run_read(&context, &rebuilt).unwrap_err(),
        "call-1 cancelled"
    );

    println!("typed resources + rebuild injection + shared state + ephemeral call context");
}
