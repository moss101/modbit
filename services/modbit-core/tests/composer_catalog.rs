//! REQ-PX-056 (QUAL-PX-056), Core side, on the real Core over its real socket:
//! the model picker's facts. `ListModelVariants` serves the variants each
//! catalog model offers (the picker never computes a combination), marks the
//! model the organisation policy blocks and the typed code a pin of it would
//! get, and `SetExecutionPreference` really refuses that pin with the same
//! typed code and records nothing. No model is called.
#![cfg(unix)]

mod px_common;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    ExecutionPreference, ExecutionPreferenceSet, GetTaskPosture, Id, ListModelVariants,
    ModelVariantList, ObjectiveProfile, SetExecutionPreference, TaskPostureView,
};
use prost::Message;
use px_common::*;

async fn variants(c: &mut Client, task: Option<&Id>) -> Result<ModelVariantList, ClientError> {
    let ack = c
        .command(envelope(
            rand_id(),
            "ListModelVariants",
            ListModelVariants {
                task_id: task.cloned(),
            }
            .encode_to_vec(),
        ))
        .await?;
    Ok(Client::result(&ack).unwrap())
}

async fn pin(
    c: &mut Client,
    g: Option<u64>,
    task: &Id,
    model: &str,
) -> Result<ExecutionPreferenceSet, String> {
    let r = c
        .command(envelope_fenced(
            rand_id(),
            "SetExecutionPreference",
            SetExecutionPreference {
                task_id: Some(task.clone()),
                preference: Some(ExecutionPreference {
                    objective: ObjectiveProfile::Unspecified as i32,
                    pin_endpoint: "openai".into(),
                    pin_model: model.into(),
                    ..Default::default()
                }),
            }
            .encode_to_vec(),
            g,
        ))
        .await;
    match r {
        Ok(ack) => Ok(Client::result(&ack).unwrap()),
        Err(ClientError::Rejected { code, .. }) => Err(code),
        Err(e) => panic!("{e}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn qual_px_056_the_catalog_serves_variants_and_the_policy_block_with_its_typed_code() {
    let (_repo, root) = plain_repo(&[("NOTES.md", "# notes\n")]);
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", "http://127.0.0.1:9"),
            ("MODBIT_MODEL_POLICY", "block=openai/gpt-5-mini"),
        ],
    );
    let mut c = core.client().await;
    let list = variants(&mut c, None).await.unwrap();
    assert_eq!(list.objectives, ["COST", "BALANCE", "INTELLIGENCE"]);
    assert_eq!(list.default_objective, "BALANCE");

    // A reasoning model offers one variant per effort; the default is marked and what raises cost is too.
    let gpt5 = list
        .models
        .iter()
        .find(|m| m.model == "gpt-5")
        .expect("gpt-5");
    assert!(gpt5.reasoning && gpt5.pin_allowed && gpt5.pin_refusal_code.is_empty());
    let efforts: Vec<_> = gpt5.variants.iter().map(|v| v.effort.as_str()).collect();
    assert_eq!(efforts, ["low", "medium", "high"]);
    let default: Vec<_> = gpt5
        .variants
        .iter()
        .filter(|v| v.is_default)
        .map(|v| v.effort.as_str())
        .collect();
    assert_eq!(default, ["medium"]);
    let raises: Vec<_> = gpt5
        .variants
        .iter()
        .filter(|v| v.raises_cost)
        .map(|v| v.effort.as_str())
        .collect();
    assert_eq!(raises, ["high"]);

    // A model with no reasoning has the one standard variant and no effort to choose.
    let plain = list
        .models
        .iter()
        .find(|m| m.model == "gpt-4.1")
        .expect("gpt-4.1");
    assert!(!plain.reasoning);
    assert_eq!(plain.variants.len(), 1);
    assert_eq!(plain.variants[0].effort, "");

    // The policy-blocked model carries the rule and the code a pin would get.
    let blocked = list
        .models
        .iter()
        .find(|m| m.model == "gpt-5-mini")
        .expect("gpt-5-mini");
    assert!(!blocked.pin_allowed);
    assert_eq!(blocked.pin_refusal_code, "POLICY_BLOCKED");
    assert_eq!(blocked.blocked_by_policy, "block=openai/gpt-5-mini");

    // The command refuses exactly that pin, with that code, and records nothing.
    let (session, g) = session_with_lease(&mut c, 0x60).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x61,
        "local_trusted",
        "pick a model",
    )
    .await;
    assert_eq!(
        pin(&mut c, g, &task, "gpt-5-mini").await.unwrap_err(),
        "POLICY_BLOCKED"
    );
    let ack = c
        .command(envelope(
            rand_id(),
            "GetTaskPosture",
            GetTaskPosture {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let posture: TaskPostureView = Client::result(&ack).unwrap();
    let recorded = posture.preference.expect("preference view");
    assert_eq!(
        (recorded.pin_model.as_str(), recorded.offset),
        ("", 0),
        "a refused pin leaves no record"
    );

    // An allowed pin is recorded, and the task-scoped listing still allows it.
    let set = pin(&mut c, g, &task, "gpt-5").await.unwrap();
    assert_eq!(set.preference.unwrap().pin_model, "gpt-5");
    assert!(
        variants(&mut c, Some(&task))
            .await
            .unwrap()
            .models
            .iter()
            .any(|m| m.model == "gpt-5" && m.pin_allowed)
    );

    // A task that does not exist is a typed refusal, not an empty list.
    match variants(&mut c, Some(&id16(0x99))).await {
        Err(ClientError::Rejected { code, .. }) => assert_eq!(code, "UNKNOWN_TASK"),
        other => panic!("expected UNKNOWN_TASK, got {other:?}"),
    }
}
