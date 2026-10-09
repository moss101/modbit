//! `browser.fill_form` (PX-123): the derived action that fills a form from
//! typed values and credential handles, in the order the form needs, and
//! checks what it did. A raw click-and-type sequence fails on a form whose
//! second field stays disabled until the first holds a value; this fills
//! in document order, reading the page again before each field and waiting
//! for it to become available, and then verifies the result against the page
//! it produced. It never types a secret: a password or card field is filled
//! by a credential handle the person bound to the origin, or not at all. A
//! fill that cannot be verified is reported as unverified, never as success.
//!
//! Every input is an ordinary host `Act` under the same lease, stop and
//! latch as `browser.act`; the call's effect class is judged before it runs
//! (a submitting fill is the class of the submit control, a credential fill
//! is a protected effect), and checked again against the page as it is.

use std::sync::Arc;
use std::time::{Duration, Instant};

use modbit_browser::compiler::{ActionRisk, Entity, EntityKind, PageEntities};
use modbit_browser::semantic::{self, FormView};
use modbit_browser::{BrowserPort, HostRequest, HostResponse};
use serde_json::{Value, json};

use crate::EffectClass;
use crate::browser::{
    MAX_SNAPSHOT_NODES, PROFILES, PROVENANCE, check_expect, check_latch, compiled_page,
    credential_for, effect_for, entity_json, finish, form_risk, known_form, send_input, session_of,
    spec, state_json, typed_fail,
};
use crate::pipeline::InvokeContext;
use crate::registry::{BoxFuture, Idempotency, Tool, ToolOutcome, ToolSpec};

/// How long a field that is not yet available is waited for.
const FIELD_WAIT: Duration = Duration::from_millis(3_000);
/// Time between reads while waiting.
const FIELD_POLL: Duration = Duration::from_millis(150);

struct FillForm(ToolSpec);

impl Tool for FillForm {
    fn spec(&self) -> &ToolSpec {
        &self.0
    }

    fn effect_of(&self, args: &Value) -> EffectClass {
        let form = args["form"].as_str().unwrap_or_default();
        let submit = args["submit"].as_bool() == Some(true);
        let credentials = args["credentials"]
            .as_object()
            .is_some_and(|c| !c.is_empty());
        let mut class = if credentials {
            // Putting a credential into a page is a protected effect, whatever the form.
            EffectClass::ExternalSideEffect
        } else {
            EffectClass::ReversibleWrite
        };
        if submit {
            let submitted = match known_form(form) {
                Some(f) => effect_for(f.risk),
                // A form this Core has not compiled: a submission is protected until it is read.
                None => EffectClass::ExternalSideEffect,
            };
            class = class.max(submitted);
        }
        class
    }

    fn invoke<'a>(&'a self, ctx: &'a InvokeContext, args: Value) -> BoxFuture<'a, ToolOutcome> {
        Box::pin(async move { finish(fill_form(ctx, args).await) })
    }
}

/// `browser.fill_form`.
pub(crate) fn tool() -> Arc<dyn Tool> {
    let mut s = spec(
        "browser.fill_form",
        "Fill a form of the task's live page from values and submit it, as one verified action. `form` is a form ref from the snapshot's `forms`; `values` maps a field (its ref or its exact label) to the text to enter — a check box takes \"true\" or \"false\", a combo box an option's text. Fields are filled in document order, the page read again before each one and a field that is not yet enabled waited for, which a raw sequence of fills cannot do on a dynamic form. A field that takes a secret (a password, a card number) is never filled from `values`: name a credential handle the person bound to this origin in `credentials` (the desktop fills it; you never see the value). With `submit: true` the form's submit control is activated (a protected action that needs approval, a link-free destination check applies). The answer says what was filled and whether the page shows it: `verified` is true only when the fields hold what was entered and, on submit, the declared `expect` (or a visible change of the page) held; otherwise the outcome is UNVERIFIED, never success.",
        json!({"type":"object","properties":{
            "form":{"type":"string","minLength":12,"maxLength":12},
            "values":{"type":"object","additionalProperties":{"type":"string","maxLength":4096},"maxProperties":40},
            "credentials":{"type":"object","additionalProperties":{"type":"string","minLength":5,"maxLength":64},"maxProperties":8},
            "submit":{"type":"boolean"},
            "expect":{"type":"object","properties":{
                "url_contains":{"type":"string"},
                "text_contains":{"type":"string"},
                "changed":{"type":"boolean"}
            },"additionalProperties":false}
        },"required":["form"],"additionalProperties":false}),
        120_000,
    );
    s.effect_class = EffectClass::ReversibleWrite;
    s.idempotency = Idempotency::NonIdempotent;
    s.execution_profiles = PROFILES.iter().map(|p| (*p).to_owned()).collect();
    Arc::new(FillForm(s))
}

/// One field to fill, resolved against the form.
struct Step {
    field: semantic::FormField,
    value: String,
    credential: Option<String>,
}

fn truthy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "true" | "yes" | "on" | "1" | "checked"
    )
}

/// The form on a freshly compiled page, by the reference the model holds.
fn find_form(page: &PageEntities, reference: &str) -> Option<FormView> {
    semantic::forms_of(page)
        .into_iter()
        .find(|f| f.reference == reference)
}

fn field_list(form: &FormView) -> String {
    form.fields
        .iter()
        .map(|f| format!("{} “{}”", f.reference, f.name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A requested field (a ref or a label) as the form's field.
fn resolve_field(
    form: &FormView,
    key: &str,
) -> std::result::Result<semantic::FormField, ToolOutcome> {
    if let Some(f) = form.fields.iter().find(|f| f.reference == key) {
        return Ok(f.clone());
    }
    let by_name: Vec<&semantic::FormField> = form
        .fields
        .iter()
        .filter(|f| f.name.eq_ignore_ascii_case(key.trim()))
        .collect();
    if by_name.len() == 1 {
        return Ok(by_name[0].clone());
    }
    Err(typed_fail(
        "FORM_FIELD_UNKNOWN",
        format!(
            "`{key}` is {} in the form; its fields: {}",
            if by_name.is_empty() {
                "not a field"
            } else {
                "ambiguous (several fields share that label; use the ref)"
            },
            field_list(form)
        ),
    ))
}

/// Read the page until `reference` is an enabled field, or give up.
async fn available_field(
    port: &Arc<dyn BrowserPort>,
    session: modbit_browser::BrowserSessionId,
    reference: &str,
    first: Option<&PageEntities>,
) -> std::result::Result<(PageEntities, Entity), ToolOutcome> {
    let started = Instant::now();
    let mut page = match first {
        Some(p) => p.clone(),
        None => compiled_page(port, session, MAX_SNAPSHOT_NODES).await?,
    };
    loop {
        match page.entities.iter().find(|e| e.reference == reference) {
            Some(e) if !e.disabled && e.backend_dom_node_id.is_some() => {
                let e = e.clone();
                return Ok((page, e));
            }
            other => {
                if started.elapsed() >= FIELD_WAIT {
                    return Err(typed_fail(
                        "FORM_FIELD_UNAVAILABLE",
                        match other {
                            Some(e) => format!("{} “{}” stayed disabled", e.role, e.name),
                            None => format!("field {reference} is no longer on the page"),
                        },
                    ));
                }
            }
        }
        tokio::time::sleep(FIELD_POLL).await;
        page = compiled_page(port, session, MAX_SNAPSHOT_NODES).await?;
    }
}

async fn fill_form(ctx: &InvokeContext, args: Value) -> ToolOutcome {
    let form_ref = args["form"].as_str().unwrap_or_default().to_owned();
    let submit = args["submit"].as_bool() == Some(true);
    let (port, session) = match session_of(ctx).await {
        Ok(x) => x,
        Err(o) => return o,
    };
    if let Some(refusal) = check_latch(&port, session).await {
        return refusal;
    }
    let observed = port.lease(session).await.map(|l| l.generation);
    let before = match compiled_page(&port, session, MAX_SNAPSHOT_NODES).await {
        Ok(p) => p,
        Err(o) => return o,
    };
    let forms = semantic::forms_of(&before);
    let Some(form) = forms.iter().find(|f| f.reference == form_ref).cloned() else {
        let mut o = typed_fail(
            "FORM_NOT_FOUND",
            format!(
                "no form {form_ref} on the page at state version {}; forms now: {}",
                before.state.state_version,
                if forms.is_empty() {
                    "none".to_owned()
                } else {
                    forms
                        .iter()
                        .map(|f| format!("{} “{}”", f.reference, f.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ),
        );
        o.structured_output = json!({"form": form_ref, "forms": forms.iter().map(|f| &f.reference).collect::<Vec<_>>(), "provenance": PROVENANCE});
        return o;
    };
    // Resolve what to fill before anything is touched: unknown fields and
    // typed values for secret fields are refused up front.
    let mut steps: Vec<Step> = Vec::new();
    let empty = serde_json::Map::new();
    let values = args["values"].as_object().unwrap_or(&empty);
    let creds = args["credentials"].as_object().unwrap_or(&empty);
    for (key, v) in values {
        let field = match resolve_field(&form, key) {
            Ok(f) => f,
            Err(o) => return o,
        };
        if field.secret {
            return typed_fail(
                "SECRET_FIELD_REQUIRES_CREDENTIAL",
                format!(
                    "{} “{}” takes a secret and a typed value is never entered into it",
                    field.role, field.name
                ),
            );
        }
        steps.push(Step {
            field,
            value: v.as_str().unwrap_or_default().to_owned(),
            credential: None,
        });
    }
    for (key, h) in creds {
        let field = match resolve_field(&form, key) {
            Ok(f) => f,
            Err(o) => return o,
        };
        if steps.iter().any(|s| s.field.reference == field.reference) {
            return typed_fail(
                "FORM_FIELD_UNKNOWN",
                format!(
                    "{} “{}” is given both a value and a credential",
                    field.role, field.name
                ),
            );
        }
        let Some(entity) = before
            .entities
            .iter()
            .find(|e| e.reference == field.reference)
        else {
            return typed_fail(
                "FORM_FIELD_UNAVAILABLE",
                format!("field {} is not on the page", field.reference),
            );
        };
        // The handle is checked against the field's own origin before anything is sent.
        match credential_for(
            &port,
            &before,
            entity,
            h.as_str().unwrap_or_default(),
            &format!("task:{}", ctx.task_id),
        )
        .await
        {
            Ok(handle) => steps.push(Step {
                field,
                value: String::new(),
                credential: Some(handle),
            }),
            Err(o) => return o,
        }
    }
    // Document order: the order the form needs its fields in.
    steps.sort_by_key(|s| {
        form.fields
            .iter()
            .position(|f| f.reference == s.field.reference)
            .unwrap_or(usize::MAX)
    });
    // The class this call was judged under must cover what it is about to do.
    let submit_entity = form
        .submit
        .iter()
        .find_map(|r| {
            before
                .entities
                .iter()
                .find(|e| &e.reference == r && !e.disabled)
        })
        .cloned();
    if submit && submit_entity.is_none() {
        return typed_fail(
            "FORM_FIELD_UNAVAILABLE",
            format!("form {form_ref} has no enabled control that submits it"),
        );
    }
    let mut needed = ActionRisk::PageOnly;
    if steps.iter().any(|s| s.credential.is_some()) {
        needed = needed.max(ActionRisk::Protected);
    }
    if submit {
        needed = needed.max(form_risk(&before, &form).risk);
    }
    if needed > ActionRisk::PageOnly && ctx.effect_class.is_some_and(|c| c < effect_for(needed)) {
        return typed_fail(
            "ACTION_UNSAFE",
            format!(
                "filling form {form_ref} as asked is a protected action (a credential, or a submission) and this call was judged below that class: read the page (browser.snapshot) and call again so the approval can be asked"
            ),
        );
    }
    let fingerprint_before = modbit_browser::compiler::state_fingerprint(&before);
    let mut filled: Vec<Value> = Vec::new();
    let mut current = Some(before.clone());
    for step in &steps {
        let (_page, entity) =
            match available_field(&port, session, &step.field.reference, current.as_ref()).await {
                Ok(x) => x,
                Err(mut o) => {
                    if !o.structured_output.is_object() {
                        o.structured_output = json!({});
                    }
                    o.structured_output["filled_so_far"] = json!(filled);
                    return o;
                }
            };
        current = None;
        let Some(node) = entity.backend_dom_node_id else {
            continue;
        };
        let (action, value) = match entity.role.as_str() {
            "checkbox" | "switch" | "radio" => (
                if truthy(&step.value) || entity.role == "radio" {
                    "check"
                } else {
                    "uncheck"
                },
                String::new(),
            ),
            "combobox" => ("select", step.value.clone()),
            _ => ("fill", step.value.clone()),
        };
        let (action, value) = if step.credential.is_some() {
            ("fill_credential", String::new())
        } else {
            (action, value)
        };
        let mut sent = send_input(
            ctx,
            &port,
            session,
            observed,
            HostRequest::Act {
                backend_dom_node_id: node,
                action: action.to_owned(),
                value: value.clone(),
                key: String::new(),
                at: None,
                credential_handle: step.credential.clone(),
                frame: entity.frame.clone(),
            },
            ("browser.fill_form", action, &step.field.reference),
        )
        .await;
        // A combo box that is a text field with suggestions takes text.
        if action == "select"
            && let Err(o) = &sent
            && o.error_code.as_deref() == Some("NOT_SELECT")
        {
            sent = send_input(
                ctx,
                &port,
                session,
                observed,
                HostRequest::Act {
                    backend_dom_node_id: node,
                    action: "fill".into(),
                    value,
                    key: String::new(),
                    at: None,
                    credential_handle: None,
                    frame: entity.frame.clone(),
                },
                ("browser.fill_form", "fill", &step.field.reference),
            )
            .await;
        }
        match sent {
            Ok(HostResponse::Acted { detail, .. }) => {
                filled.push(json!({"ref": step.field.reference, "name": step.field.name, "action": action, "detail": detail, "by_credential": step.credential.is_some()}));
            }
            Ok(other) => {
                return ToolOutcome::infra(
                    "BROWSER_PROTOCOL",
                    format!("unexpected host answer {other:?}"),
                );
            }
            Err(mut o) => {
                if !o.structured_output.is_object() {
                    o.structured_output = json!({});
                }
                o.structured_output["filled_so_far"] = json!(filled);
                o.structured_output["failed_field"] =
                    json!({"ref": step.field.reference, "name": step.field.name});
                return o;
            }
        }
    }
    // Submit: the form's own control, a click as a person would.
    let mut submitted = false;
    let mut navigated = false;
    if submit {
        let page = match compiled_page(&port, session, MAX_SNAPSHOT_NODES).await {
            Ok(p) => p,
            Err(o) => return o,
        };
        let Some(target) = submit_entity
            .as_ref()
            .and_then(|s| page.entities.iter().find(|e| e.reference == s.reference))
            .filter(|e| !e.disabled && e.backend_dom_node_id.is_some())
        else {
            return typed_fail(
                "FORM_FIELD_UNAVAILABLE",
                "the form's submit control is no longer enabled on the page".to_owned(),
            );
        };
        let node = target.backend_dom_node_id.unwrap_or_default();
        match send_input(
            ctx,
            &port,
            session,
            observed,
            HostRequest::Act {
                backend_dom_node_id: node,
                action: "click".into(),
                value: String::new(),
                key: String::new(),
                at: None,
                credential_handle: None,
                frame: target.frame.clone(),
            },
            ("browser.fill_form", "submit", &target.reference),
        )
        .await
        {
            Ok(HostResponse::Acted { navigated: n, .. }) => {
                submitted = true;
                navigated = n;
            }
            Ok(other) => {
                return ToolOutcome::infra(
                    "BROWSER_PROTOCOL",
                    format!("unexpected host answer {other:?}"),
                );
            }
            Err(mut o) => {
                if !o.structured_output.is_object() {
                    o.structured_output = json!({});
                }
                o.structured_output["filled_so_far"] = json!(filled);
                return o;
            }
        }
    }
    let after = match compiled_page(&port, session, MAX_SNAPSHOT_NODES).await {
        Ok(p) => p,
        Err(mut o) => {
            if !o.structured_output.is_object() {
                o.structured_output = json!({});
            }
            o.structured_output["action_performed"] = json!(true);
            o.structured_output["filled_so_far"] = json!(filled);
            return o;
        }
    };
    let delta = modbit_browser::compiler::diff(&before, &after);
    let fingerprint_after = delta.to_fingerprint.clone();
    // What the page shows of what was entered (a secret's value is not readable).
    let mut field_checks: Vec<Value> = Vec::new();
    let mut fields_ok = true;
    let mut verifiable = 0usize;
    let after_form = find_form(&after, &form_ref);
    for step in &steps {
        if step.credential.is_some() {
            field_checks.push(json!({"ref": step.field.reference, "name": step.field.name, "by_credential": true, "held": Value::Null, "note": "a credential's value is not readable back"}));
            continue;
        }
        let now = after
            .entities
            .iter()
            .find(|e| e.reference == step.field.reference);
        let (held, shown) = match (step.field.role.as_str(), now) {
            ("checkbox" | "switch" | "radio", Some(e)) => {
                let want = truthy(&step.value) || step.field.role == "radio";
                let is = e.checked.as_deref() == Some("true");
                (Some(is == want), json!(e.checked))
            }
            (_, Some(e)) => {
                // A select shows the option's text; a text field its value.
                let held = e.value == step.value || e.value.eq_ignore_ascii_case(step.value.trim());
                (Some(held), json!(e.value))
            }
            // After a submission the form is expected to be gone.
            (_, None) => (if submitted { None } else { Some(false) }, Value::Null),
        };
        if let Some(h) = held {
            verifiable += 1;
            fields_ok &= h;
        }
        field_checks.push(json!({"ref": step.field.reference, "name": step.field.name, "expected": step.value, "held": held, "now": shown}));
    }
    // After a submission: the declared postcondition, or a visible change.
    let mut submit_checks = Vec::new();
    let mut submit_ok = true;
    let mut submit_verified = false;
    if submitted {
        let (ok, mut checks) = check_expect(
            args.get("expect"),
            &fingerprint_before,
            &fingerprint_after,
            &after,
        );
        submit_ok = ok;
        submit_verified = !checks.is_empty();
        if !submit_verified {
            // No declared postcondition: the page must at least have changed
            // (a new URL or title, or the form gone).
            let gone = after_form.is_none();
            let changed = fingerprint_before != fingerprint_after;
            let url_changed = before.state.url != after.state.url;
            let held = gone || changed || url_changed || navigated;
            checks.push(json!({"derived": "the page changed or the form is gone", "held": held}));
            submit_ok = held;
            submit_verified = true;
        }
        submit_checks = checks;
    }
    let verified = fields_ok && submit_ok && (verifiable > 0 || submit_verified);
    let ok = fields_ok && submit_ok;
    port.note_delivered(session, &fingerprint_after).await;
    let mut v = state_json(&after.state);
    v["action"] = json!("fill_form");
    v["ref"] = json!(form_ref);
    v["target"] = json!({"role": "form", "name": form.name, "path": []});
    v["form"] = json!({"ref": form.reference, "name": form.name, "kind": form.kind.label(), "destination": form.destination, "cross_origin": form.cross_origin});
    v["filled"] = json!(filled);
    v["submitted"] = json!(submitted);
    v["verified"] = json!(verified && ok);
    v["outcome"] = json!(if !ok {
        "FAILED"
    } else if verified {
        "VERIFIED"
    } else {
        "UNVERIFIED"
    });
    v["verification"] = json!({
        "fields": field_checks,
        "submit": submit_checks,
        "note": if verified { "what was entered is on the page" } else { "nothing on the page could be read back to confirm this: treat it as done but unconfirmed, and check before relying on it" },
    });
    if submitted
        && let Some(e) = args
            .get("expect")
            .filter(|e| e.is_object() && !e.as_object().is_some_and(serde_json::Map::is_empty))
    {
        let _ = e;
        v["postcondition"] = json!({"held": submit_ok, "checks": v["verification"]["submit"]});
    }
    v["navigated"] = json!(navigated || delta.url.is_some());
    v["fingerprint_before"] = json!(fingerprint_before);
    v["state_fingerprint"] = json!(fingerprint_after);
    v["changed"] = json!(fingerprint_before != fingerprint_after);
    v["delta"] = json!({
        "added": delta.added.iter().map(entity_json).collect::<Vec<_>>(),
        "removed": delta.removed,
        "changed": delta.changed,
        "text_added": delta.text_added,
        "text_removed": delta.text_removed,
        "new_url": delta.url,
        "new_title": delta.title,
    });
    v["entity_count"] = json!(after.entities.len());
    let _ = EntityKind::Field;
    port.remember_transition(
        session,
        modbit_browser::KnownTransition {
            from_fingerprint: fingerprint_before.clone(),
            reference: form_ref.clone(),
            action: "fill_form".into(),
            to_fingerprint: fingerprint_after.clone(),
            to_url: after.state.url.clone(),
            verified: Some(verified && ok),
            times: 1,
        },
    )
    .await;
    if ok {
        ToolOutcome::ok(v)
    } else {
        let mut o = ToolOutcome::fail(
            "POSTCONDITION_FAILED",
            format!(
                "form {form_ref} was filled, but the page does not show what was entered{}",
                if submitted {
                    " (or the submission's postcondition did not hold)"
                } else {
                    ""
                }
            ),
        );
        o.structured_output = v;
        o
    }
}
