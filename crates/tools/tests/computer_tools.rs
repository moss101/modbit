//! The `computer.*` tools through the real pipeline (PX-069, PX-070, PX-076): what the pipeline asks
//! the kernel, how a tool's bound intent becomes part of the intent hash, how a frame reaches the
//! media pipeline, and what happens with no port at all. The port here answers from a script; the
//! same paths run over a real actuator process in `modbit-core`'s `px_computer` tests.

use std::sync::{Arc, Mutex};

use modbit_computer::actuator::BoxFuture;
use modbit_computer::frame::{Frame, FrameMemo};
use modbit_computer::ops::Op;
use modbit_computer::{Answer, CallInfo, ComputerPort, Failure, FrameArtifact, Refusal};
use modbit_domain::{TaskId, ToolCallId};
use modbit_tools::{
    CapabilityPort, InvokeContext, ObjectSink, PolicyDecision, PolicyRequest, ProfilePolicy,
    ToolRegistry, ToolRuntime, ToolStatus,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

struct MemSink(Mutex<Vec<(String, Vec<u8>)>>);
impl ObjectSink for MemSink {
    fn put(&self, bytes: &[u8]) -> modbit_tools::Result<String> {
        let h = hex::encode(Sha256::digest(bytes));
        self.0.lock().unwrap().push((h.clone(), bytes.to_vec()));
        Ok(h)
    }
}

/// What the kernel stub saw, and what it answers.
struct Kernel {
    seen: Mutex<Vec<PolicyRequest>>,
}
impl CapabilityPort for Kernel {
    fn decide(&self, req: &PolicyRequest) -> PolicyDecision {
        self.seen.lock().unwrap().push(req.clone());
        PolicyDecision::ApprovalRequired {
            reason: "asks".into(),
            scope_json: "{}".into(),
        }
    }
}

/// A port whose binding and answer the test sets.
#[derive(Default)]
struct Port {
    binding: Mutex<Value>,
    refuse: Mutex<Option<Refusal>>,
    answer: Mutex<Option<Result<Answer, Failure>>>,
    prepared: Mutex<Vec<String>>,
}
impl ComputerPort for Port {
    fn offered(&self) -> bool {
        true
    }
    fn prepare<'a>(
        &'a self,
        _info: &'a CallInfo,
        tool: &'a str,
        op: &'a Op,
    ) -> BoxFuture<'a, Result<Option<Value>, Refusal>> {
        Box::pin(async move {
            self.prepared.lock().unwrap().push(tool.to_owned());
            if let Some(r) = self.refuse.lock().unwrap().clone() {
                return Err(r);
            }
            Ok(op
                .needs_approval()
                .then(|| self.binding.lock().unwrap().clone()))
        })
    }
    fn invoke<'a>(
        &'a self,
        _info: &'a CallInfo,
        _tool: &'a str,
        _op: Op,
    ) -> BoxFuture<'a, Result<Answer, Failure>> {
        Box::pin(async move {
            self.answer.lock().unwrap().take().unwrap_or(Ok(Answer {
                output: json!({"ok": true}),
                frame: None,
            }))
        })
    }
}

fn ctx(sink: Arc<MemSink>, port: Option<Arc<Port>>, kernel: Option<Arc<Kernel>>) -> InvokeContext {
    InvokeContext {
        task_id: TaskId::new(),
        execution_profile: "local_trusted".into(),
        capability_lease_id: None,
        workspace: None,
        workspace_root: None,
        exec: None,
        sink,
        output_budget_bytes: 64 * 1024,
        kernel: kernel.map(|k| k as Arc<dyn CapabilityPort>),
        search: None,
        language: None,
        artifacts: None,
        tool_call_id: None,
        journal: None,
        forge: None,
        forge_ledger: None,
        browser: None,
        sandbox: None,
        effect_class: None,
        secrets_in_custody: vec![],
        environment: None,
        memory: None,
        skills: None,
        external: None,
        cancel: None,
        hooks: None,
        git_state: None,
        computer: port.map(|p| p as Arc<dyn ComputerPort>),
        approval_id: None,
        intent_hash: None,
    }
}

fn runtime() -> ToolRuntime {
    let mut r = ToolRegistry::new();
    modbit_tools::computer::register_computer(&mut r).unwrap();
    ToolRuntime::new(r, Arc::new(ProfilePolicy))
}

const PRESS: &str = r#"{"element":"s1/n4"}"#;

#[tokio::test]
async fn a_bound_intent_is_part_of_the_hash_the_kernel_is_asked_about() {
    let rt = runtime();
    let port = Arc::new(Port::default());
    let kernel = Arc::new(Kernel {
        seen: Mutex::default(),
    });
    let c = ctx(
        Arc::new(MemSink(Mutex::default())),
        Some(port.clone()),
        Some(kernel.clone()),
    );
    let mut hashes = vec![];
    for pid in [100, 100, 101] {
        *port.binding.lock().unwrap() =
            json!({"computer": {"application": {"bundle_id": "a.b", "pid": pid}}});
        let out = rt
            .invoke(&c, ToolCallId::new(), "computer.press", PRESS)
            .await;
        assert_eq!(
            out.result.status,
            ToolStatus::ApprovalPending,
            "{:?}",
            out.result
        );
        hashes.push(out.result.arguments_hash.clone());
    }
    // The same facts, the same intent; another process, another intent.
    assert_eq!(hashes[0], hashes[1]);
    assert_ne!(hashes[0], hashes[2]);
    // The arguments alone are not the intent.
    assert_ne!(hashes[0], modbit_tools::arguments_hash(PRESS).unwrap());
    // The kernel was asked about the same hash, and told what was bound.
    let seen = kernel.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[0].intent_hash, hashes[0]);
    assert_eq!(
        seen[2].bound_intent.as_ref().unwrap()["computer"]["application"]["pid"],
        101
    );
    assert_eq!(
        seen[0].required_capabilities,
        vec!["computer.act".to_owned()]
    );
}

#[tokio::test]
async fn a_refusal_before_the_kernel_asks_nobody_and_carries_its_code_and_escalation() {
    let rt = runtime();
    let port = Arc::new(Port::default());
    *port.refuse.lock().unwrap() = Some(Refusal::new(modbit_computer::Code::TargetStale, "gone"));
    let kernel = Arc::new(Kernel {
        seen: Mutex::default(),
    });
    let c = ctx(
        Arc::new(MemSink(Mutex::default())),
        Some(port),
        Some(kernel.clone()),
    );
    let out = rt
        .invoke(&c, ToolCallId::new(), "computer.press", PRESS)
        .await;
    assert_eq!(out.result.status, ToolStatus::InvalidArguments);
    assert_eq!(out.result.error_code.as_deref(), Some("TARGET_STALE"));
    assert_eq!(out.result.structured_output["escalation"], "re_observe");
    assert_eq!(out.result.structured_output["actuated"], false);
    assert!(
        kernel.seen.lock().unwrap().is_empty(),
        "no approval is asked for what cannot be done"
    );
    // A misspelt argument: the advertised names, before the port or the kernel.
    let port2 = Arc::new(Port::default());
    let c = ctx(
        Arc::new(MemSink(Mutex::default())),
        Some(port2.clone()),
        Some(kernel.clone()),
    );
    let out = rt
        .invoke(&c, ToolCallId::new(), "computer.press", r#"{"elemnt":"x"}"#)
        .await;
    assert_eq!(out.result.error_code.as_deref(), Some("INVALID_ARGUMENTS"));
    assert_eq!(
        out.result.structured_output["advertised"],
        json!(["element", "action"])
    );
    assert!(
        port2.prepared.lock().unwrap().is_empty(),
        "the port never saw the call"
    );
}

#[tokio::test]
async fn with_no_port_every_call_answers_actuator_unavailable() {
    let rt = runtime();
    let c = ctx(Arc::new(MemSink(Mutex::default())), None, None);
    for (tool, args) in [
        ("computer.apps", "{}"),
        ("computer.press", PRESS),
        ("computer.start", r#"{"application":"x"}"#),
    ] {
        let out = rt.invoke(&c, ToolCallId::new(), tool, args).await;
        assert_eq!(out.result.status, ToolStatus::InfraFailure, "{tool}");
        assert_eq!(
            out.result.error_code.as_deref(),
            Some("ACTUATOR_UNAVAILABLE"),
            "{tool}"
        );
    }
}

#[tokio::test]
async fn an_unknown_outcome_is_the_calls_status_and_a_frame_goes_through_the_media_pipeline() {
    let rt = runtime();
    let sink = Arc::new(MemSink(Mutex::default()));
    // An approval-free tool (computer.state) so the pipeline runs the effector under the stub kernel.
    struct Allow;
    impl CapabilityPort for Allow {
        fn decide(&self, _: &PolicyRequest) -> PolicyDecision {
            PolicyDecision::Allow {
                rule: "t".into(),
                approval_id: None,
            }
        }
    }
    let port = Arc::new(Port::default());
    let mut c = ctx(sink.clone(), Some(port.clone()), None);
    c.kernel = Some(Arc::new(Allow));
    // An unknown outcome.
    *port.answer.lock().unwrap() = Some(Err(Failure::Unknown {
        reason: "TIMEOUT".into(),
        output: json!({"code": "OUTCOME_UNKNOWN", "latched": true}),
    }));
    let out = rt
        .invoke(&c, ToolCallId::new(), "computer.state", "{}")
        .await;
    assert_eq!(out.result.status, ToolStatus::UnknownOutcome);
    assert_eq!(out.result.structured_output["latched"], true);
    // A frame: stored, with provenance, and named by the digest the audit would record.
    let mut frame = Frame::solid(1280, 800, [240, 240, 240, 255]);
    frame.mask(&[modbit_computer::model::Rect {
        x: 10,
        y: 10,
        width: 20,
        height: 20,
    }]);
    let png = {
        let mut memo = FrameMemo::default();
        modbit_computer::frame::prepare(&encode(&frame), &[], 0, &mut memo).unwrap()
    };
    let object_digest = hex::encode(Sha256::digest(&png.encoded.bytes));
    *port.answer.lock().unwrap() = Some(Ok(Answer {
        output: json!({"frame": "f1"}),
        frame: Some(FrameArtifact {
            encoded: png.encoded.clone(),
            digest: png.digest.clone(),
            object_digest: object_digest.clone(),
            reused: false,
            source: "computer:a.b#Main@w1".into(),
        }),
    }));
    let out = rt
        .invoke(&c, ToolCallId::new(), "computer.screenshot", "{}")
        .await;
    assert_eq!(out.result.status, ToolStatus::Success, "{:?}", out.result);
    let o = &out.result.structured_output;
    assert_eq!(o["artifact"]["digest"], object_digest);
    assert_eq!(o["media"]["provenance"]["source"], "computer:a.b#Main@w1");
    assert_eq!(o["media"]["width"], 1280);
    assert!(
        sink.0
            .lock()
            .unwrap()
            .iter()
            .any(|(h, _)| *h == object_digest)
    );
    // A digest the audit did not record is refused, not stored under another name.
    *port.answer.lock().unwrap() = Some(Ok(Answer {
        output: json!({}),
        frame: Some(FrameArtifact {
            encoded: png.encoded.clone(),
            digest: png.digest.clone(),
            object_digest: "0".repeat(64),
            reused: false,
            source: "x".into(),
        }),
    }));
    let out = rt
        .invoke(&c, ToolCallId::new(), "computer.screenshot", "{}")
        .await;
    assert_eq!(
        out.result.error_code.as_deref(),
        Some("ARTIFACT_DIGEST_MISMATCH")
    );
}

fn encode(f: &Frame) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, f.width, f.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&f.rgba).unwrap();
    }
    out
}
