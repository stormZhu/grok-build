use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthMethod {
    ApiKey,
    CachedToken,
    Oidc,
}

impl AuthMethod {
    fn is_session_based(self) -> bool {
        matches!(self, Self::CachedToken | Self::Oidc)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelByok {
    Byok,
    NotByok,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Endpoint {
    FirstParty,
    ThirdParty,
}

fn session_token_auth_gate(method: AuthMethod, model_byok: ModelByok, endpoint: Endpoint) -> bool {
    method.is_session_based()
        && match model_byok {
            ModelByok::NotByok => true,
            ModelByok::Byok => false,
            ModelByok::Unknown => endpoint == Endpoint::FirstParty,
        }
}

struct Secret(&'static str);

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CredentialSource {
    Session,
    ModelByok,
}

struct ModelEntry {
    alias: &'static str,
    display_name: &'static str,
    wire_model: &'static str,
    endpoint: Endpoint,
    byok: ModelByok,
    model_key: Option<Secret>,
}

#[derive(Debug, PartialEq, Eq)]
struct SamplerConfig {
    wire_model: &'static str,
    endpoint: Endpoint,
    credential_source: CredentialSource,
    generation: u64,
}

fn build_sampler_config(
    model: &ModelEntry,
    method: AuthMethod,
    generation: u64,
) -> Result<SamplerConfig, &'static str> {
    let credential_source = match (&model.model_key, model.byok) {
        (Some(_), ModelByok::Byok) => CredentialSource::ModelByok,
        (None, ModelByok::NotByok)
            if model.endpoint == Endpoint::FirstParty
                && session_token_auth_gate(method, model.byok, model.endpoint) =>
        {
            CredentialSource::Session
        }
        _ => return Err("no credential allowed for model/endpoint"),
    };

    Ok(SamplerConfig {
        wire_model: model.wire_model,
        endpoint: model.endpoint,
        credential_source,
        generation,
    })
}

#[derive(Debug, PartialEq, Eq)]
enum Recovery {
    RefreshAndResubmit(SamplerConfig),
    SurfaceUnauthorized,
}

struct TurnAuthRecovery {
    used: bool,
    refresh_calls: usize,
    sampler_generation: u64,
}

impl TurnAuthRecovery {
    fn handle_401(
        &mut self,
        model: &ModelEntry,
        method: AuthMethod,
        refresh_succeeds: bool,
    ) -> Recovery {
        let eligible = session_token_auth_gate(method, model.byok, model.endpoint);
        if !eligible || self.used {
            return Recovery::SurfaceUnauthorized;
        }

        self.used = true;
        self.refresh_calls += 1;
        if !refresh_succeeds {
            return Recovery::SurfaceUnauthorized;
        }

        self.sampler_generation += 1;
        Recovery::RefreshAndResubmit(
            build_sampler_config(model, method, self.sampler_generation).unwrap(),
        )
    }
}

fn main() {
    assert!(session_token_auth_gate(
        AuthMethod::Oidc,
        ModelByok::NotByok,
        Endpoint::FirstParty
    ));
    assert!(!session_token_auth_gate(
        AuthMethod::ApiKey,
        ModelByok::NotByok,
        Endpoint::FirstParty
    ));
    assert!(!session_token_auth_gate(
        AuthMethod::CachedToken,
        ModelByok::Byok,
        Endpoint::ThirdParty
    ));
    assert!(session_token_auth_gate(
        AuthMethod::CachedToken,
        ModelByok::Unknown,
        Endpoint::FirstParty
    ));
    assert!(!session_token_auth_gate(
        AuthMethod::CachedToken,
        ModelByok::Unknown,
        Endpoint::ThirdParty
    ));

    let unsafe_route = ModelEntry {
        alias: "misconfigured",
        display_name: "Misconfigured model",
        wire_model: "third-party-model",
        endpoint: Endpoint::ThirdParty,
        byok: ModelByok::NotByok,
        model_key: None,
    };
    assert_eq!(
        build_sampler_config(&unsafe_route, AuthMethod::Oidc, 1).unwrap_err(),
        "no credential allowed for model/endpoint"
    );

    let first_party = ModelEntry {
        alias: "fast",
        display_name: "Fast model",
        wire_model: "grok-4-real-id",
        endpoint: Endpoint::FirstParty,
        byok: ModelByok::NotByok,
        model_key: None,
    };
    let initial = build_sampler_config(&first_party, AuthMethod::Oidc, 7).unwrap();
    assert_eq!(first_party.alias, "fast");
    assert_eq!(first_party.display_name, "Fast model");
    assert_eq!(initial.wire_model, "grok-4-real-id");
    assert_eq!(initial.credential_source, CredentialSource::Session);

    let mut turn = TurnAuthRecovery {
        used: false,
        refresh_calls: 0,
        sampler_generation: initial.generation,
    };
    let recovered = turn.handle_401(&first_party, AuthMethod::Oidc, true);
    assert!(matches!(
        recovered,
        Recovery::RefreshAndResubmit(SamplerConfig {
            generation: 8,
            credential_source: CredentialSource::Session,
            ..
        })
    ));
    assert_eq!(turn.refresh_calls, 1);
    assert_eq!(
        turn.handle_401(&first_party, AuthMethod::Oidc, true),
        Recovery::SurfaceUnauthorized
    );
    assert_eq!(turn.refresh_calls, 1);

    let third_party = ModelEntry {
        alias: "custom",
        display_name: "Custom gateway",
        wire_model: "vendor-model-v2",
        endpoint: Endpoint::ThirdParty,
        byok: ModelByok::Byok,
        model_key: Some(Secret("third-party-secret")),
    };
    let config = build_sampler_config(&third_party, AuthMethod::Oidc, 1).unwrap();
    assert_eq!(config.credential_source, CredentialSource::ModelByok);
    let mut third_party_turn = TurnAuthRecovery {
        used: false,
        refresh_calls: 0,
        sampler_generation: 1,
    };
    assert_eq!(
        third_party_turn.handle_401(&third_party, AuthMethod::Oidc, true),
        Recovery::SurfaceUnauthorized
    );
    assert_eq!(third_party_turn.refresh_calls, 0);

    let secret_debug = format!("{:?}", third_party.model_key.as_ref().unwrap());
    assert_eq!(secret_debug, "<redacted>");
    assert!(!secret_debug.contains(third_party.model_key.as_ref().unwrap().0));

    println!("wire model + endpoint gate + one recovery budget + redacted credentials");
}
