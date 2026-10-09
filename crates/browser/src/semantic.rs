//! The semantic layer above the entities (PX-123; docs/22 "Semantic Browser
//! Compiler"): what kind of page this is, which controls belong together as
//! a form, which higher-level actions the page offers (fill a form, open the
//! nth result, dismiss a dialog) with the conditions each one needs and
//! promises, and the intent and scope filters that let the model ask for the
//! part of a large page it needs. Everything here is derived from the
//! compiled entities — page strings are data, rules are code — and nothing
//! here acts: the actions go through `browser.act` and `browser.fill_form`,
//! under the Kernel and the takeover lease.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::compiler::{
    ActionRisk, Entity, EntityKind, PageEntities, classify_action, is_dismiss_control,
};

/// What kind of page this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageKind {
    /// A sign-in page.
    Login,
    /// A registration page.
    Signup,
    /// A search box, no results yet.
    Search,
    /// A list of results for a query.
    SearchResults,
    /// A list of items (an inbox, a table of links).
    List,
    /// A form that is neither of the above.
    Form,
    /// A payment or checkout step.
    Checkout,
    /// Prose to read.
    Article,
    /// An error page (not found, forbidden, server error).
    Error,
    /// A page asking to prove the visitor is human.
    Captcha,
    /// Nothing recognised.
    Unknown,
}

impl PageKind {
    /// The wire label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Signup => "signup",
            Self::Search => "search",
            Self::SearchResults => "search_results",
            Self::List => "list",
            Self::Form => "form",
            Self::Checkout => "checkout",
            Self::Article => "article",
            Self::Error => "error",
            Self::Captcha => "captcha",
            Self::Unknown => "unknown",
        }
    }
}

/// The classification, with what it rests on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageClass {
    /// The kind.
    pub kind: PageKind,
    /// The name of the modal dialog that is open over the page, if one is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialog: Option<String>,
    /// The signals the rule saw (so the label can be audited, not trusted).
    pub reasons: Vec<String>,
}

fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// Whether a field takes a secret the model never types (PX-123).
#[must_use]
pub fn is_secret_field(e: &Entity) -> bool {
    if e.kind != EntityKind::Field {
        return false;
    }
    let ty = e.input_type.as_deref().map(lower).unwrap_or_default();
    let ac = e.autocomplete.as_deref().map(lower).unwrap_or_default();
    let name = lower(&e.name);
    ty == "password"
        || ac.contains("password")
        || ac.starts_with("cc-")
        || ac.contains("one-time-code")
        || name.contains("password")
        || name.contains("passcode")
        || name.contains("cvv")
        || name.contains("cvc")
        || name.contains("card number")
        || name.contains("security code")
        || name.contains("social security")
}

fn is_card_field(e: &Entity) -> bool {
    let ac = e.autocomplete.as_deref().map(lower).unwrap_or_default();
    let name = lower(&e.name);
    e.kind == EntityKind::Field
        && (ac.starts_with("cc-")
            || name.contains("card number")
            || name.contains("cvv")
            || name.contains("cvc")
            || name.contains("expiry")
            || name.contains("expiration")
            || name.contains("security code"))
}

fn is_password_field(e: &Entity) -> bool {
    e.kind == EntityKind::Field
        && (e.input_type.as_deref().map(lower).as_deref() == Some("password")
            || e.autocomplete
                .as_deref()
                .is_some_and(|a| lower(a).contains("password"))
            || lower(&e.name).contains("password"))
}

fn is_search_field(e: &Entity) -> bool {
    e.kind == EntityKind::Field
        && (e.role == "searchbox"
            || lower(&e.name).contains("search")
            || e.placeholder
                .as_deref()
                .is_some_and(|p| lower(p).contains("search"))
            || e.input_type.as_deref().map(lower).as_deref() == Some("search")
            || e.path.iter().any(|p| p.starts_with("search:")))
}

/// Words in the page's headline that mark an error page.
const ERROR_MARKERS: &[&str] = &[
    "not found",
    "access denied",
    "forbidden",
    "internal server error",
    "something went wrong",
    "service unavailable",
    "bad gateway",
    "isn't working",
    "can't be reached",
    "unauthorized",
    "gateway timeout",
];

fn starts_with_status(s: &str) -> bool {
    let t = s.trim();
    [
        "400", "401", "403", "404", "410", "500", "502", "503", "504",
    ]
    .iter()
    .any(|c| t.starts_with(c))
}

/// Classify a compiled page by rules over its entities and text.
#[must_use]
pub fn classify_page(page: &PageEntities) -> PageClass {
    let mut reasons: Vec<String> = Vec::new();
    let title = lower(&page.state.title);
    let url = lower(&page.state.url);
    let ents = &page.entities;
    let fields: Vec<&Entity> = ents
        .iter()
        .filter(|e| e.kind == EntityKind::Field)
        .collect();
    let links = ents.iter().filter(|e| e.role == "link").count();
    let buttons = ents.iter().filter(|e| e.role == "button").count();
    let dialog = ents
        .iter()
        .find(|e| e.role == "dialog" || e.role == "alertdialog")
        .map(|e| e.name.clone());
    let lines: Vec<String> = page.text.iter().map(|t| lower(t)).collect();
    let headline: Vec<String> = std::iter::once(title.clone())
        .chain(lines.iter().take(3).cloned())
        .collect();
    let finish = |kind: PageKind, reasons: Vec<String>| PageClass {
        kind,
        dialog: dialog.clone(),
        reasons,
    };

    // A challenge to prove the visitor is human.
    let captcha_hint = |s: &str| {
        s.contains("captcha")
            || s.contains("i'm not a robot")
            || s.contains("verify you are human")
            || s.contains("verify you're human")
            || s.contains("are you a robot")
    };
    if page.frames.iter().any(|f| {
        let o = lower(&f.origin);
        o.contains("captcha") || o.contains("turnstile") || o.contains("challenges.cloudflare")
    }) || ents.iter().any(|e| captcha_hint(&lower(&e.name)))
        || lines.iter().any(|l| captcha_hint(l))
        || captcha_hint(&title)
    {
        reasons.push("a captcha frame, control or text".into());
        return finish(PageKind::Captcha, reasons);
    }

    // An error page: the headline says so and there is little to do.
    if (headline.iter().any(|h| starts_with_status(h))
        || headline
            .iter()
            .any(|h| ERROR_MARKERS.iter().any(|m| h.contains(m))))
        && fields.len() + buttons <= 6
    {
        reasons.push("the title or headline names an error and the page offers little".into());
        return finish(PageKind::Error, reasons);
    }

    // Checkout: card fields, or payment words with something to fill.
    let card_fields = fields.iter().filter(|e| is_card_field(e)).count();
    let checkout_words = [
        "checkout",
        "payment",
        "billing",
        "order summary",
        "shopping cart",
        "your cart",
    ];
    let checkout_hint = checkout_words
        .iter()
        .any(|w| url.contains(w) || title.contains(w) || lines.iter().any(|l| l.contains(w)));
    if card_fields >= 1 {
        reasons.push(format!("{card_fields} card field(s)"));
        return finish(PageKind::Checkout, reasons);
    }
    if checkout_hint
        && (!fields.is_empty()
            || ents.iter().any(|e| {
                let n = lower(&e.name);
                e.role == "button"
                    && (n.contains("place order")
                        || n.contains("pay")
                        || n.contains("proceed to")
                        || n.contains("continue to"))
            }))
    {
        reasons.push("checkout or payment wording with something to fill or confirm".into());
        return finish(PageKind::Checkout, reasons);
    }

    // Sign-in and registration: password fields.
    let passwords = fields.iter().filter(|e| is_password_field(e)).count();
    let new_password = fields.iter().any(|e| {
        e.autocomplete
            .as_deref()
            .is_some_and(|a| lower(a).contains("new-password"))
    });
    if passwords >= 2 || new_password {
        reasons.push(format!(
            "{passwords} password field(s), a new one among them"
        ));
        return finish(PageKind::Signup, reasons);
    }
    if passwords == 1 {
        reasons.push("one password field".into());
        return finish(PageKind::Login, reasons);
    }

    // Search.
    let search_input = fields.iter().any(|e| is_search_field(e));
    let query_url = [
        "?q=", "&q=", "?query=", "&query=", "?search=", "&search=", "?s=", "?term=",
    ]
    .iter()
    .any(|p| url.contains(p));
    if (search_input || query_url) && links >= 5 {
        reasons.push(format!(
            "{} and {links} links",
            if search_input {
                "a search field"
            } else {
                "a query in the URL"
            }
        ));
        return finish(PageKind::SearchResults, reasons);
    }
    if search_input && fields.len() <= 2 {
        reasons.push("a search field and little else".into());
        return finish(PageKind::Search, reasons);
    }

    // A form of its own.
    let groups = form_groups(page);
    if groups
        .values()
        .any(|g| g.iter().filter(|e| e.kind == EntityKind::Field).count() >= 2)
    {
        reasons.push("a group of two or more fields".into());
        return finish(PageKind::Form, reasons);
    }

    // A list of items.
    let list_items = ents.iter().filter(|e| e.role == "link").count();
    if list_items >= 8 || ents.iter().any(|e| e.role == "list") && list_items >= 4 {
        reasons.push(format!("{list_items} links"));
        return finish(PageKind::List, reasons);
    }

    // Prose.
    let chars: usize = page.text.iter().map(|t| t.chars().count()).sum();
    if chars >= 300 && page.text.len() >= 2 && fields.len() + buttons <= 12 {
        reasons.push(format!("{chars} characters of text and few controls"));
        return finish(PageKind::Article, reasons);
    }
    finish(PageKind::Unknown, reasons)
}

/// The grouping key of an entity: the form it belongs to, by its host key
/// when the host gave one, else by the form landmark in its path; fields
/// outside any form group by the innermost region (a dialog, the main area).
fn group_key(e: &Entity) -> Option<(String, bool)> {
    if let Some(k) = &e.form {
        return Some((format!("{}#{k}", e.frame.as_deref().unwrap_or("")), false));
    }
    if let Some(i) = e.path.iter().rposition(|p| p.starts_with("form:")) {
        return Some((
            format!(
                "{}#path:{}",
                e.frame.as_deref().unwrap_or(""),
                e.path[..=i].join(">")
            ),
            false,
        ));
    }
    if e.kind == EntityKind::Landmark {
        return None;
    }
    let region = e
        .path
        .iter()
        .rposition(|p| {
            p.starts_with("dialog:")
                || p.starts_with("alertdialog:")
                || p.starts_with("main:")
                || p.starts_with("search:")
        })
        .map_or_else(String::new, |i| e.path[..=i].join(">"));
    Some((
        format!("{}#region:{region}", e.frame.as_deref().unwrap_or("")),
        true,
    ))
}

fn form_groups(page: &PageEntities) -> BTreeMap<String, Vec<&Entity>> {
    let mut m: BTreeMap<String, Vec<&Entity>> = BTreeMap::new();
    for e in &page.entities {
        if let Some((k, _)) = group_key(e) {
            m.entry(k).or_default().push(e);
        }
    }
    m
}

/// One field of a form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormField {
    /// Reference.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Role.
    pub role: String,
    /// Accessible name (untrusted).
    pub name: String,
    /// `type` of the input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,
    /// The field must be filled.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    /// Current value (never for a secret field).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    /// The field takes a secret: only a credential handle fills it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
    /// Disabled now.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    /// Checked state of a check box or radio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<String>,
    /// `autocomplete` token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autocomplete: Option<String>,
}

/// A group of controls that belong together.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormView {
    /// A reference naming the form (stable across reads of the same form).
    #[serde(rename = "ref")]
    pub reference: String,
    /// The form's accessible name, or the region it sits in.
    pub name: String,
    /// There was no `<form>`: the controls were grouped by the region they share.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub implicit: bool,
    /// What the form is for.
    pub kind: PageKind,
    /// `get` | `post`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// The origin the form sends its data to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    /// That origin is not the page's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cross_origin: bool,
    /// The fields in document order.
    pub fields: Vec<FormField>,
    /// References of the controls that submit it.
    pub submit: Vec<String>,
    /// It sits in a dialog.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub in_dialog: bool,
    /// The frame it is in (`None` = the top frame).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<String>,
}

fn form_ref(key: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"form");
    h.update([0]);
    h.update(key.as_bytes());
    hex::encode(h.finalize())[..12].to_owned()
}

/// The forms of a page, grouped, in document order of their first control.
#[must_use]
pub fn forms_of(page: &PageEntities) -> Vec<FormView> {
    let groups = form_groups(page);
    let mut views: Vec<(usize, FormView)> = Vec::new();
    for (key, members) in groups {
        let fields: Vec<&Entity> = members
            .iter()
            .copied()
            .filter(|e| e.kind == EntityKind::Field && e.role != "listbox")
            .collect();
        if fields.is_empty() {
            continue;
        }
        let implicit = group_key(members[0]).is_some_and(|(_, i)| i);
        let submit: Vec<&Entity> = members
            .iter()
            .copied()
            .filter(|e| {
                e.kind == EntityKind::Action
                    && e.role == "button"
                    && (e.submits || (implicit && !is_dismiss_control(&e.name)))
            })
            .collect();
        // An implicit group is a form only when something in it submits.
        if implicit && submit.is_empty() {
            continue;
        }
        let first = members
            .iter()
            .filter_map(|m| {
                page.entities
                    .iter()
                    .position(|e| e.reference == m.reference)
            })
            .min()
            .unwrap_or(0);
        let meta = members.iter().find_map(|m| {
            m.form.as_ref().and_then(|k| {
                page.forms
                    .iter()
                    .find(|f| &f.key == k && f.frame == m.frame)
            })
        });
        let name = meta
            .map(|f| f.name.clone())
            .filter(|n| !n.is_empty())
            .or_else(|| {
                members[0]
                    .path
                    .iter()
                    .rev()
                    .find(|p| {
                        p.starts_with("form:")
                            || p.starts_with("dialog:")
                            || p.starts_with("main:")
                            || p.starts_with("search:")
                    })
                    .cloned()
            })
            .unwrap_or_default();
        let page_origin = crate::origin_of(&page.state.url).unwrap_or_default();
        let destination = submit
            .iter()
            .find_map(|s| s.dest_origin.clone())
            .or_else(|| meta.and_then(|f| f.action.as_deref().and_then(crate::origin_of)));
        let cross_origin = destination
            .as_deref()
            .is_some_and(|d| !page_origin.is_empty() && d != page_origin);
        let secrets = fields.iter().any(|e| is_secret_field(e));
        let kind = if fields.iter().filter(|e| is_password_field(e)).count() >= 2 {
            PageKind::Signup
        } else if fields.iter().any(|e| is_password_field(e)) {
            PageKind::Login
        } else if fields.iter().any(|e| is_card_field(e)) {
            PageKind::Checkout
        } else if fields.iter().any(|e| is_search_field(e)) && fields.len() <= 2 {
            PageKind::Search
        } else {
            PageKind::Form
        };
        let _ = secrets;
        views.push((
            first,
            FormView {
                reference: form_ref(&key),
                name,
                implicit,
                kind,
                method: meta.and_then(|f| f.method.clone()),
                destination,
                cross_origin,
                fields: fields
                    .iter()
                    .map(|e| {
                        let secret = is_secret_field(e);
                        FormField {
                            reference: e.reference.clone(),
                            role: e.role.clone(),
                            name: e.name.clone(),
                            input_type: e.input_type.clone(),
                            required: e.required,
                            value: if secret {
                                String::new()
                            } else {
                                e.value.clone()
                            },
                            secret,
                            disabled: e.disabled,
                            checked: e.checked.clone(),
                            autocomplete: e.autocomplete.clone(),
                        }
                    })
                    .collect(),
                submit: submit.iter().map(|e| e.reference.clone()).collect(),
                in_dialog: fields.iter().any(|e| e.in_dialog),
                frame: members[0].frame.clone(),
            },
        ));
    }
    views.sort_by_key(|(i, _)| *i);
    views.into_iter().map(|(_, v)| v).collect()
}

/// A higher-level action the page offers: the call that performs it, what
/// must hold before and what the page should show after, and the approval
/// class the Kernel will apply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedAction {
    /// `fill_form:<ref>`, `open_result:<n>`, `dismiss_dialog`, `search`.
    pub id: String,
    /// The tool that performs it.
    pub tool: String,
    /// A call that performs it (values the model supplies are marked `<…>`).
    pub args: serde_json::Value,
    /// What is true of the page for this to apply.
    pub preconditions: Vec<String>,
    /// What the page should show after; the tool verifies it.
    pub postconditions: Vec<String>,
    /// `PAGE_ONLY` | `PROTECTED` | `DESTRUCTIVE`.
    pub risk: ActionRisk,
}

/// The actions the page offers beyond its raw controls.
#[must_use]
pub fn derive_actions(page: &PageEntities, forms: &[FormView]) -> Vec<DerivedAction> {
    let mut out: Vec<DerivedAction> = Vec::new();
    for f in forms {
        let fields: serde_json::Map<String, serde_json::Value> = f
            .fields
            .iter()
            .filter(|x| !x.secret && !matches!(x.role.as_str(), "checkbox" | "radio"))
            .take(12)
            .map(|x| {
                (
                    x.reference.clone(),
                    serde_json::json!(format!("<{}>", x.name)),
                )
            })
            .collect();
        let credentials: serde_json::Map<String, serde_json::Value> = f
            .fields
            .iter()
            .filter(|x| x.secret)
            .map(|x| {
                (
                    x.reference.clone(),
                    serde_json::json!("<credential handle>"),
                )
            })
            .collect();
        let mut args = serde_json::json!({"form": f.reference, "values": fields, "submit": !f.submit.is_empty()});
        if !credentials.is_empty() {
            args["credentials"] = serde_json::Value::Object(credentials);
        }
        let submitting = !f.submit.is_empty();
        let submit_risk = f
            .submit
            .iter()
            .filter_map(|r| page.entities.iter().find(|e| &e.reference == r))
            .map(|e| classify_action(e, "click", ""))
            .max()
            .unwrap_or(ActionRisk::PageOnly);
        let has_secret = f.fields.iter().any(|x| x.secret);
        let risk = if submitting {
            submit_risk.max(ActionRisk::Protected)
        } else if has_secret {
            ActionRisk::Protected
        } else {
            ActionRisk::PageOnly
        };
        let mut pre =
            vec!["the form is on the page and its fields are enabled in order".to_owned()];
        if has_secret {
            pre.push("secret fields are filled by credential handle only".into());
        }
        let mut post = vec!["every filled field holds the value it was given".to_owned()];
        if submitting {
            post.push("after submitting, the page changed (URL, title or the form is gone)".into());
        }
        out.push(DerivedAction {
            id: format!("fill_form:{}", f.reference),
            tool: "browser.fill_form".into(),
            args,
            preconditions: pre,
            postconditions: post,
            risk,
        });
    }
    // Open the nth result: the links of the main list on a results/list page.
    let class = classify_page(page).kind;
    if matches!(class, PageKind::SearchResults | PageKind::List) {
        let results: Vec<&Entity> = page
            .entities
            .iter()
            .filter(|e| {
                e.role == "link"
                    && !e.in_dialog
                    && !e.path.iter().any(|p| {
                        p.starts_with("navigation:")
                            || p.starts_with("banner:")
                            || p.starts_with("contentinfo:")
                    })
            })
            .collect();
        for (i, e) in results.iter().take(5).enumerate() {
            out.push(DerivedAction {
                id: format!("open_result:{}", i + 1),
                tool: "browser.act".into(),
                args: serde_json::json!({"ref": e.reference, "action": "click", "expect": {"changed": true}}),
                preconditions: vec![format!("result {} “{}” is on the page", i + 1, e.name)],
                postconditions: vec!["the page changed (the result opened)".into()],
                risk: classify_action(e, "click", ""),
            });
        }
    }
    // Dismiss the dialog that is open: the safest control that only closes it.
    if page
        .entities
        .iter()
        .any(|e| e.role == "dialog" || e.role == "alertdialog")
    {
        let closer = page
            .entities
            .iter()
            .find(|e| {
                e.kind == EntityKind::Action
                    && e.in_dialog
                    && is_dismiss_control(&e.name)
                    && !lower(&e.name).contains("cancel")
            })
            .or_else(|| {
                page.entities.iter().find(|e| {
                    e.kind == EntityKind::Action && e.in_dialog && is_dismiss_control(&e.name)
                })
            });
        if let Some(c) = closer {
            out.push(DerivedAction {
                id: "dismiss_dialog".into(),
                tool: "browser.act".into(),
                args: serde_json::json!({"ref": c.reference, "action": "click", "expect": {"changed": true}}),
                preconditions: vec!["a dialog is open over the page".into()],
                postconditions: vec!["the dialog is gone".into()],
                risk: classify_action(c, "click", ""),
            });
        }
    }
    out
}

/// What a filter kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterReport {
    /// The intent as given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// The landmark scoped to, as `role:name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Entities kept.
    pub kept: usize,
    /// Entities on the page.
    pub total: usize,
}

const STOP: &[&str] = &[
    "the", "a", "an", "to", "of", "and", "or", "in", "on", "for", "with", "my", "me", "i", "it",
    "is", "this", "that", "please", "then", "click", "press", "find",
];

fn tokens(s: &str) -> Vec<String> {
    lower(s)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2 && !STOP.contains(t))
        .map(str::to_owned)
        .collect()
}

/// Entities most relevant to `intent`, within `scope` (a landmark's
/// reference) when given. Relevance is the overlap of the intent's words
/// with the entity's name, role, placeholder and the landmarks it sits in,
/// plus the controls that share a form with a match — the model asked for
/// the part of the page it needs, not for a ranking it can trust: the
/// document order of what is kept is unchanged.
#[must_use]
pub fn filter_entities(
    page: &PageEntities,
    intent: Option<&str>,
    scope: Option<&str>,
    limit: usize,
) -> (Vec<Entity>, FilterReport) {
    let total = page.entities.len();
    let mut pool: Vec<&Entity> = page.entities.iter().collect();
    let mut scope_label = None;
    if let Some(s) = scope {
        if let Some(l) = page
            .entities
            .iter()
            .find(|e| e.reference == s && e.kind == EntityKind::Landmark)
        {
            let mut prefix = l.path.clone();
            prefix.push(format!(
                "{}:{}",
                l.role,
                l.name.chars().take(60).collect::<String>()
            ));
            scope_label = Some(format!("{}:{}", l.role, l.name));
            pool.retain(|e| {
                e.reference == l.reference
                    || (e.path.len() >= prefix.len()
                        && e.path[..prefix.len()] == prefix[..]
                        && e.frame == l.frame)
            });
        } else {
            pool.clear();
        }
    }
    let kept: Vec<Entity> = match intent.map(str::trim).filter(|i| !i.is_empty()) {
        None => pool.iter().take(limit).map(|e| (*e).clone()).collect(),
        Some(intent) => {
            let want: HashSet<String> = tokens(intent).into_iter().collect();
            let mut hit: Vec<(usize, u32)> = Vec::new();
            for (i, e) in pool.iter().enumerate() {
                let mut score = 0u32;
                for t in tokens(&e.name) {
                    if want.contains(&t) {
                        score += 3;
                    }
                }
                for t in tokens(&e.role) {
                    if want.contains(&t) {
                        score += 2;
                    }
                }
                if let Some(p) = &e.placeholder {
                    for t in tokens(p) {
                        if want.contains(&t) {
                            score += 2;
                        }
                    }
                }
                for p in &e.path {
                    for t in tokens(p) {
                        if want.contains(&t) {
                            score += 1;
                        }
                    }
                }
                if score > 0 && e.kind != EntityKind::Landmark {
                    score += 1;
                }
                if score > 0 {
                    hit.push((i, score));
                }
            }
            let mut keep: HashSet<usize> = hit.iter().map(|(i, _)| *i).collect();
            // The controls that share a form with a match.
            let hit_forms: HashSet<String> = hit
                .iter()
                .filter_map(|(i, _)| group_key(pool[*i]).map(|(k, _)| k))
                .collect();
            for (i, e) in pool.iter().enumerate() {
                if e.kind != EntityKind::Landmark
                    && group_key(e).is_some_and(|(k, implicit)| !implicit && hit_forms.contains(&k))
                {
                    keep.insert(i);
                }
            }
            // The best by score when more than the limit match.
            if keep.len() > limit {
                let mut ranked: Vec<(usize, u32)> = keep
                    .iter()
                    .map(|i| (*i, hit.iter().find(|(h, _)| h == i).map_or(0, |(_, s)| *s)))
                    .collect();
                ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                keep = ranked.into_iter().take(limit).map(|(i, _)| i).collect();
            }
            let mut idx: Vec<usize> = keep.into_iter().collect();
            idx.sort_unstable();
            idx.into_iter().map(|i| pool[i].clone()).collect()
        }
    };
    let report = FilterReport {
        intent: intent.map(str::to_owned),
        scope: scope_label,
        kept: kept.len(),
        total,
    };
    (kept, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PageState;
    use crate::compiler::{RawAxNode, compile};

    fn n(id: &str, parent: Option<&str>, role: &str, name: &str) -> RawAxNode {
        RawAxNode {
            id: id.into(),
            parent: parent.map(str::to_owned),
            role: role.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    fn field(id: &str, parent: &str, role: &str, name: &str, ty: &str) -> RawAxNode {
        RawAxNode {
            input_type: Some(ty.into()),
            ..n(id, Some(parent), role, name)
        }
    }

    fn page(url: &str, title: &str, nodes: Vec<RawAxNode>) -> PageEntities {
        let state = PageState {
            url: url.into(),
            title: title.into(),
            ready: true,
            state_version: 1,
        };
        compile(&state, &nodes, false, 200)
    }

    fn root(title: &str) -> Vec<RawAxNode> {
        vec![
            n("1", None, "RootWebArea", title),
            n("2", Some("1"), "main", ""),
        ]
    }

    fn kind(p: &PageEntities) -> PageKind {
        classify_page(p).kind
    }

    fn with(mut base: Vec<RawAxNode>, more: Vec<RawAxNode>) -> Vec<RawAxNode> {
        base.extend(more);
        base
    }

    #[test]
    fn a_labelled_set_of_pages_is_classified_by_rules_not_by_luck() {
        let login = page(
            "https://app.test/login",
            "Sign in",
            with(
                root("Sign in"),
                vec![
                    n("3", Some("2"), "form", "Login"),
                    field("4", "3", "textbox", "Email", "email"),
                    field("5", "3", "textbox", "Password", "password"),
                    n("6", Some("3"), "button", "Sign in"),
                ],
            ),
        );
        assert_eq!(kind(&login), PageKind::Login);
        let signup = page(
            "https://app.test/register",
            "Create account",
            with(
                root("Create account"),
                vec![
                    n("3", Some("2"), "form", "Register"),
                    field("4", "3", "textbox", "Email", "email"),
                    field("5", "3", "textbox", "Password", "password"),
                    field("6", "3", "textbox", "Confirm password", "password"),
                    n("7", Some("3"), "button", "Create account"),
                ],
            ),
        );
        assert_eq!(kind(&signup), PageKind::Signup);
        let checkout = page(
            "https://shop.test/pay",
            "Pay",
            with(
                root("Pay"),
                vec![
                    n("3", Some("2"), "form", "Card"),
                    RawAxNode {
                        autocomplete: Some("cc-number".into()),
                        ..field("4", "3", "textbox", "Card number", "text")
                    },
                    n("5", Some("3"), "button", "Pay now"),
                ],
            ),
        );
        assert_eq!(kind(&checkout), PageKind::Checkout);
        let mut results = with(
            root("Results for rust"),
            vec![
                n("3", Some("2"), "searchbox", "Search"),
                n("4", Some("2"), "button", "Go"),
            ],
        );
        for i in 0..6 {
            results.push(n(
                &format!("r{i}"),
                Some("2"),
                "link",
                &format!("Result {i}"),
            ));
        }
        assert_eq!(
            kind(&page(
                "https://s.test/search?q=rust",
                "Results for rust",
                results
            )),
            PageKind::SearchResults
        );
        let search = page(
            "https://s.test/",
            "Search",
            with(
                root("Search"),
                vec![
                    n("3", Some("2"), "searchbox", "Search"),
                    n("4", Some("2"), "button", "Go"),
                ],
            ),
        );
        assert_eq!(kind(&search), PageKind::Search);
        let err = page(
            "https://s.test/missing",
            "404 Not Found",
            with(
                root("404 Not Found"),
                vec![
                    n("3", Some("2"), "heading", "404 Not Found"),
                    n("4", Some("2"), "link", "Home"),
                ],
            ),
        );
        assert_eq!(kind(&err), PageKind::Error);
        let captcha = page(
            "https://s.test/verify",
            "Verify",
            with(
                root("Verify"),
                vec![
                    n("3", Some("2"), "heading", "Please verify you are human"),
                    n("4", Some("2"), "checkbox", "I'm not a robot"),
                ],
            ),
        );
        assert_eq!(kind(&captcha), PageKind::Captcha);
        let contact = page(
            "https://s.test/contact",
            "Contact",
            with(
                root("Contact"),
                vec![
                    n("3", Some("2"), "form", "Contact us"),
                    field("4", "3", "textbox", "Name", "text"),
                    field("5", "3", "textbox", "Message", "text"),
                    n("6", Some("3"), "button", "Send"),
                ],
            ),
        );
        assert_eq!(kind(&contact), PageKind::Form);
        let prose: String = "Rust is a systems programming language focused on safety. ".repeat(4);
        let article = page(
            "https://blog.test/post",
            "A post",
            with(
                root("A post"),
                vec![
                    n("3", Some("2"), "heading", "A post"),
                    n("4", Some("2"), "paragraph", &prose),
                    n(
                        "5",
                        Some("2"),
                        "paragraph",
                        &format!("Cargo is the package manager. {prose}"),
                    ),
                ],
            ),
        );
        assert_eq!(kind(&article), PageKind::Article);
        let mut inbox = root("Inbox");
        for i in 0..9 {
            inbox.push(n(
                &format!("m{i}"),
                Some("2"),
                "link",
                &format!("Message {i}"),
            ));
        }
        assert_eq!(
            kind(&page("https://m.test/", "Inbox", inbox)),
            PageKind::List
        );
    }

    #[test]
    fn page_text_that_says_login_does_not_make_a_login_page() {
        // Text is data: a paragraph claiming to be a sign-in does not classify.
        let p = page(
            "https://blog.test/x",
            "Notes",
            with(
                root("Notes"),
                vec![n(
                    "3",
                    Some("2"),
                    "paragraph",
                    "Sign in with your password to continue",
                )],
            ),
        );
        assert_ne!(kind(&p), PageKind::Login);
    }

    #[test]
    fn forms_group_fields_with_their_submit_and_secret_fields_are_marked() {
        let p = page(
            "https://app.test/login",
            "Sign in",
            with(
                root("Sign in"),
                vec![
                    n("3", Some("2"), "form", "Login"),
                    field("4", "3", "textbox", "Email", "email"),
                    RawAxNode {
                        required: true,
                        ..field("5", "3", "textbox", "Password", "password")
                    },
                    n("6", Some("3"), "button", "Sign in"),
                    n("7", Some("2"), "button", "Help"),
                ],
            ),
        );
        let forms = forms_of(&p);
        assert_eq!(forms.len(), 1, "{forms:#?}");
        let f = &forms[0];
        assert_eq!(f.kind, PageKind::Login);
        assert_eq!(f.fields.len(), 2);
        assert!(f.fields[1].secret && f.fields[1].required && !f.fields[0].secret);
        assert_eq!(f.submit.len(), 1);
        assert!(!f.implicit);
        let acts = derive_actions(&p, &forms);
        let fill = acts.iter().find(|a| a.tool == "browser.fill_form").unwrap();
        assert_eq!(fill.risk, ActionRisk::Protected);
        assert!(
            fill.args["credentials"].is_object(),
            "the password goes by handle: {fill:?}"
        );
        assert!(!fill.args["values"].to_string().contains("Password"));
    }

    #[test]
    fn controls_without_a_form_element_group_by_their_region_when_something_submits() {
        let p = page(
            "https://s.test/",
            "Search",
            with(
                root("Search"),
                vec![
                    n("3", Some("2"), "searchbox", "Search"),
                    n("4", Some("2"), "button", "Go"),
                    n("5", Some("2"), "button", "Help"),
                ],
            ),
        );
        let forms = forms_of(&p);
        assert_eq!(forms.len(), 1);
        assert!(forms[0].implicit);
        assert_eq!(forms[0].kind, PageKind::Search);
        assert_eq!(forms[0].submit.len(), 2);
    }

    #[test]
    fn a_dialog_offers_a_dismiss_action_that_is_page_only_and_never_the_destructive_button() {
        let p = page(
            "https://admin.test/users",
            "Users",
            with(
                root("Users"),
                vec![
                    n("3", Some("2"), "dialog", "Delete user?"),
                    n("4", Some("3"), "button", "Delete"),
                    n("5", Some("3"), "button", "Cancel"),
                    n("6", Some("3"), "button", "Close"),
                ],
            ),
        );
        let forms = forms_of(&p);
        let acts = derive_actions(&p, &forms);
        let d = acts
            .iter()
            .find(|a| a.id == "dismiss_dialog")
            .expect("dismiss offered");
        let target = p
            .entities
            .iter()
            .find(|e| e.reference == d.args["ref"])
            .unwrap();
        assert_eq!(target.name, "Close");
        assert_eq!(d.risk, ActionRisk::PageOnly);
        assert_eq!(classify_page(&p).dialog.as_deref(), Some("Delete user?"));
    }

    #[test]
    fn the_intent_filter_keeps_what_the_words_name_and_the_form_it_sits_in() {
        let mut nodes = with(
            root("Admin"),
            vec![
                n("3", Some("2"), "form", "Invite user"),
                field("4", "3", "textbox", "Email address", "email"),
                n("5", Some("3"), "button", "Send invite"),
            ],
        );
        for i in 0..30 {
            nodes.push(n(
                &format!("l{i}"),
                Some("2"),
                "link",
                &format!("Report {i}"),
            ));
        }
        let p = page("https://admin.test/", "Admin", nodes);
        let (kept, report) = filter_entities(&p, Some("invite a user by email"), None, 20);
        let names: Vec<&str> = kept.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"Email address") && names.contains(&"Send invite"),
            "{names:?}"
        );
        assert!(kept.len() < 10, "{names:?}");
        assert_eq!(report.total, p.entities.len());
        // Scoped to a landmark: only what is inside it.
        let form = p.entities.iter().find(|e| e.role == "form").unwrap();
        let (inside, rep) = filter_entities(&p, None, Some(&form.reference), 50);
        assert_eq!(
            inside.len(),
            3,
            "{:?}",
            inside.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert!(rep.scope.as_deref().unwrap().starts_with("form:"));
        // An unknown scope keeps nothing.
        assert!(
            filter_entities(&p, None, Some("nope00000000"), 50)
                .0
                .is_empty()
        );
    }
}
