use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfigSource {
    Requirements,
    Cli,
    Env,
    Config,
    Remote,
    Default,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Resolved<T> {
    value: T,
    source: ConfigSource,
}

fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(&key) {
                    Some(existing) => deep_merge(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

#[derive(Default)]
struct Inputs<T> {
    requirements: Option<T>,
    cli: Option<T>,
    env: Option<T>,
    config: Option<T>,
    remote: Option<T>,
    default: T,
}

fn resolve<T>(inputs: Inputs<T>) -> Resolved<T> {
    let candidates = [
        (inputs.requirements, ConfigSource::Requirements),
        (inputs.cli, ConfigSource::Cli),
        (inputs.env, ConfigSource::Env),
        (inputs.config, ConfigSource::Config),
        (inputs.remote, ConfigSource::Remote),
        (Some(inputs.default), ConfigSource::Default),
    ];
    let (value, source) = candidates
        .into_iter()
        .find_map(|(value, source)| value.map(|value| (value, source)))
        .expect("default is always present");
    Resolved { value, source }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuntimeConfig {
    model: Resolved<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct Session {
    config: RuntimeConfig,
}

fn main() {
    let mut merged = serde_json::json!({
        "sampling": { "temperature": 0.2, "max_tokens": 1000 },
        "models": ["base-a", "base-b"],
        "feature": true
    });
    deep_merge(
        &mut merged,
        serde_json::json!({
            "sampling": { "max_tokens": 2000 },
            "models": ["overlay-only"],
            "feature": { "mode": "strict" }
        }),
    );
    assert_eq!(merged["sampling"]["temperature"], 0.2);
    assert_eq!(merged["sampling"]["max_tokens"], 2000);
    assert_eq!(merged["models"], serde_json::json!(["overlay-only"]));
    assert_eq!(merged["feature"], serde_json::json!({ "mode": "strict" }));

    let from_cli = resolve(Inputs {
        cli: Some("cli-model".to_owned()),
        env: Some("env-model".to_owned()),
        config: Some("config-model".to_owned()),
        remote: Some("remote-model".to_owned()),
        default: "default-model".to_owned(),
        ..Inputs::default()
    });
    assert_eq!(from_cli.value, "cli-model");
    assert_eq!(from_cli.source, ConfigSource::Cli);

    let pinned = resolve(Inputs {
        requirements: Some(false),
        cli: Some(true),
        env: Some(true),
        config: Some(true),
        remote: Some(true),
        default: true,
    });
    assert_eq!(
        pinned,
        Resolved {
            value: false,
            source: ConfigSource::Requirements
        }
    );

    let mut global = RuntimeConfig {
        model: Resolved {
            value: "model-v1".to_owned(),
            source: ConfigSource::Config,
        },
    };
    let old_session = Session {
        config: global.clone(),
    };
    global.model = Resolved {
        value: "model-v2".to_owned(),
        source: ConfigSource::Remote,
    };
    let new_session = Session {
        config: global.clone(),
    };
    assert_eq!(old_session.config.model.value, "model-v1");
    assert_eq!(new_session.config.model.value, "model-v2");
    assert_eq!(new_session.config.model.source, ConfigSource::Remote);

    println!("deep merge + source-aware priority + immutable session snapshot");
}
