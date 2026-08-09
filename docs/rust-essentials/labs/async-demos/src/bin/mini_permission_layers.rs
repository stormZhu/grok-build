#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolInput {
    ListDir(&'static str),
    Edit(&'static str),
    Execute(&'static str),
    WebFetch(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccessKind {
    Read(&'static str),
    Edit(&'static str),
    Bash(&'static str),
    WebFetch(&'static str),
}

impl From<ToolInput> for AccessKind {
    fn from(input: ToolInput) -> Self {
        match input {
            ToolInput::ListDir(path) => Self::Read(path),
            ToolInput::Edit(path) => Self::Edit(path),
            ToolInput::Execute(command) => Self::Bash(command),
            ToolInput::WebFetch(url) => Self::WebFetch(url),
        }
    }
}

impl AccessKind {
    fn blocked_in_plan(self) -> bool {
        matches!(self, Self::Edit(_) | Self::Bash(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionMode {
    Plan,
    Act,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PolicyRule {
    Allow,
    Ask,
    Deny(&'static str),
    UnknownBashShape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    Allow,
    Ask,
    FollowupMessage(&'static str),
    Reject(&'static str),
    PolicyDeny(&'static str),
    Cancelled,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PermissionModes {
    yolo: bool,
    auto: bool,
    managed_yolo_pin: bool,
}

impl PermissionModes {
    fn set_yolo(&mut self, enabled: bool) {
        self.yolo = enabled && !self.managed_yolo_pin;
        if self.yolo {
            self.auto = false;
        }
    }

    fn set_auto(&mut self, enabled: bool) {
        self.auto = enabled;
        if enabled {
            self.yolo = false;
        }
    }

    fn decide(&self, rule: PolicyRule) -> Decision {
        match rule {
            PolicyRule::Allow => Decision::Allow,
            PolicyRule::Deny(reason) => Decision::PolicyDeny(reason),
            PolicyRule::Ask | PolicyRule::UnknownBashShape if self.yolo => Decision::Allow,
            PolicyRule::Ask | PolicyRule::UnknownBashShape => Decision::Ask,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KernelOperation {
    Read,
    Write,
    Network,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SandboxProfile {
    allow_write: bool,
    allow_network: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Executed,
    PlanDenied,
    AwaitingUser,
    ModelCanContinue(&'static str),
    TurnRejected(&'static str),
    TurnCancelled,
    TurnFollowup(&'static str),
    SandboxDenied,
}

fn run_tool(
    mode: SessionMode,
    access: AccessKind,
    decision: Decision,
    operation: KernelOperation,
    sandbox: SandboxProfile,
) -> Outcome {
    if mode == SessionMode::Plan && access.blocked_in_plan() {
        return Outcome::PlanDenied;
    }

    match decision {
        Decision::Ask => return Outcome::AwaitingUser,
        Decision::PolicyDeny(reason) => return Outcome::ModelCanContinue(reason),
        Decision::Reject(reason) => return Outcome::TurnRejected(reason),
        Decision::Cancelled => return Outcome::TurnCancelled,
        Decision::FollowupMessage(message) => return Outcome::TurnFollowup(message),
        Decision::Allow => {}
    }

    let kernel_allows = match operation {
        KernelOperation::Read => true,
        KernelOperation::Write => sandbox.allow_write,
        KernelOperation::Network => sandbox.allow_network,
    };
    if kernel_allows {
        Outcome::Executed
    } else {
        Outcome::SandboxDenied
    }
}

fn main() {
    assert_eq!(
        AccessKind::from(ToolInput::ListDir("src")),
        AccessKind::Read("src")
    );
    assert_eq!(
        AccessKind::from(ToolInput::Edit("src/main.rs")),
        AccessKind::Edit("src/main.rs")
    );
    assert_eq!(
        AccessKind::from(ToolInput::Execute("cargo test")),
        AccessKind::Bash("cargo test")
    );
    assert_eq!(
        AccessKind::from(ToolInput::WebFetch("https://example.test")),
        AccessKind::WebFetch("https://example.test")
    );

    let mut modes = PermissionModes::default();
    assert_eq!(modes.decide(PolicyRule::UnknownBashShape), Decision::Ask);
    modes.set_yolo(true);
    assert_eq!(modes.decide(PolicyRule::Ask), Decision::Allow);
    assert_eq!(
        modes.decide(PolicyRule::Deny("managed policy")),
        Decision::PolicyDeny("managed policy")
    );
    modes.set_auto(true);
    assert!(!modes.yolo);
    assert!(modes.auto);

    let mut pinned = PermissionModes {
        managed_yolo_pin: true,
        ..PermissionModes::default()
    };
    pinned.set_yolo(true);
    assert!(!pinned.yolo);

    let permissive = SandboxProfile {
        allow_write: true,
        allow_network: true,
    };
    assert_eq!(
        run_tool(
            SessionMode::Plan,
            AccessKind::Edit("src/main.rs"),
            Decision::Allow,
            KernelOperation::Write,
            permissive,
        ),
        Outcome::PlanDenied
    );
    assert_eq!(
        run_tool(
            SessionMode::Act,
            AccessKind::WebFetch("https://example.test"),
            Decision::Allow,
            KernelOperation::Network,
            SandboxProfile {
                allow_write: true,
                allow_network: false,
            },
        ),
        Outcome::SandboxDenied
    );
    assert_eq!(
        run_tool(
            SessionMode::Act,
            AccessKind::Read("README.md"),
            Decision::PolicyDeny("outside policy"),
            KernelOperation::Read,
            permissive,
        ),
        Outcome::ModelCanContinue("outside policy")
    );
    assert_eq!(
        run_tool(
            SessionMode::Act,
            AccessKind::Read("README.md"),
            Decision::Reject("user rejected"),
            KernelOperation::Read,
            permissive,
        ),
        Outcome::TurnRejected("user rejected")
    );
    assert_eq!(
        run_tool(
            SessionMode::Act,
            AccessKind::Read("README.md"),
            Decision::Cancelled,
            KernelOperation::Read,
            permissive,
        ),
        Outcome::TurnCancelled
    );
    assert_eq!(
        run_tool(
            SessionMode::Act,
            AccessKind::Read("README.md"),
            Decision::FollowupMessage("use another file"),
            KernelOperation::Read,
            permissive,
        ),
        Outcome::TurnFollowup("use another file")
    );
    assert_eq!(
        run_tool(
            SessionMode::Act,
            AccessKind::Read("README.md"),
            modes.decide(PolicyRule::Allow),
            KernelOperation::Read,
            permissive,
        ),
        Outcome::Executed
    );

    println!("semantic access + plan gate + permission decisions + kernel sandbox");
}
