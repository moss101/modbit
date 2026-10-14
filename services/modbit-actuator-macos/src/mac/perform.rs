//! Inputs, on the action ladder of docs/22 and CUC-A04: a structured
//! accessibility action on a resolved element first, a coordinate action
//! bound to a fresh observation second, raw synthesised input last. Each
//! response says which rung was used and why, and what the application did.
#![allow(unsafe_code)]

use std::time::Duration;

use modbit_protocol::v1 as wire;
use wire::perform_request::Action;

use super::Mac;
use super::ax::{self, Ax, Window};
use super::cgwin;
use super::ffi::{CGPoint, kAXErrorActionUnsupported};
use super::input::{Poster, Route, parse_chord};
use crate::backend::{Ctl, Fail, Halt, PerformArgs, Res};
use crate::canvas;

/// An element addressed by the snapshot its id came from.
struct Target {
    ax: Ax,
    node: wire::AxNode,
    index: usize,
}

fn stale(why: &str) -> Fail {
    Fail::nd("TARGET_STALE", why.to_owned())
}

fn element_index(id: &str) -> Option<usize> {
    id.strip_prefix('n')?.parse().ok()
}

/// Whether node `k` sits inside a sheet (the one thing a modal window lets
/// you reach).
fn inside_sheet(nodes: &[wire::AxNode], k: usize) -> bool {
    let mut cur = k;
    for _ in 0..nodes.len() {
        let n = &nodes[cur];
        if n.role == "sheet" {
            return true;
        }
        match element_index(&n.parent_id) {
            Some(p) if p < nodes.len() && p != cur => cur = p,
            _ => return false,
        }
    }
    false
}

fn resolve(mac: &Mac, pid: u32, win: &Window, snapshot_id: &str, element_id: &str) -> Res<Target> {
    let snaps = mac.snapshots.lock().unwrap_or_else(|e| e.into_inner());
    let Some(s) = snaps.get(&(pid, win.info.window_id.clone())) else {
        return Err(stale("the window has not been read; read its state first"));
    };
    if snapshot_id.is_empty() || s.id != snapshot_id {
        return Err(stale("the snapshot is not the window's current one"));
    }
    let Some(index) = element_index(element_id).filter(|i| *i < s.handles.len()) else {
        return Err(stale("no such element in the snapshot"));
    };
    let target = Target {
        ax: s.handles[index].clone(),
        node: s.nodes[index].clone(),
        index,
    };
    if win.info.modal && !inside_sheet(&s.nodes, index) {
        return Err(Fail::nd(
            "MODAL_BLOCKING",
            "a modal dialog or sheet is in front of the window",
        ));
    }
    drop(snaps);
    // The element must still exist.
    if let Err(e) = target.ax.attr(&ax::names().role) {
        return Err(ax::ax_fail(e, "the element", false));
    }
    Ok(target)
}

fn cg_point(sx: f64, sy: f64, origin: CGPoint) -> CGPoint {
    CGPoint {
        x: sx + origin.x,
        y: sy + origin.y,
    }
}

/// The canvas point as a global display point, for the scope's source.
fn to_global(
    p: &wire::Point,
    scope: wire::Scope,
    win: &wire::WindowInfo,
) -> Res<(CGPoint, canvas::Letterbox)> {
    let (origin, w, h) = if scope == wire::Scope::Screen {
        let d = cgwin::main_display();
        (d.origin, d.size.width, d.size.height)
    } else {
        let b = win.bounds.unwrap_or_default();
        (
            CGPoint {
                x: f64::from(b.x),
                y: f64::from(b.y),
            },
            f64::from(b.width),
            f64::from(b.height),
        )
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lb = canvas::fit(w.round().max(1.0) as u32, h.round().max(1.0) as u32);
    let Some((sx, sy)) = lb.to_source(p.x, p.y) else {
        return Err(Fail::nd(
            "TARGET_OCCLUDED",
            "the point is outside the window",
        ));
    };
    Ok((cg_point(sx, sy, origin), lb))
}

fn route_for(scope: wire::Scope, pid: u32) -> Route {
    if scope == wire::Scope::Screen {
        Route::Hid
    } else {
        Route::Pid(pid)
    }
}

/// Convert an input halt into the typed failure, with the delivery truth.
fn halted(h: Halt, sent: usize) -> Fail {
    Ctl::halt_fail(h, sent > 0)
}

struct Outcome {
    modality: &'static str,
    reason: String,
}

pub fn perform(mac: &Mac, a: &PerformArgs<'_>, ctl: &Ctl) -> Res<wire::PerformResponse> {
    let req = a.req;
    let Some(id) = req.application.as_ref() else {
        return Err(Fail::nd("WINDOW_UNVERIFIABLE", "no application identity"));
    };
    mac.verified(id)?;
    let pid = id.pid;
    let app = Ax::app(pid).ok_or_else(|| Fail::nd("WINDOW_UNVERIFIABLE", "no such process"))?;
    let win = ax::find_window(&app, &req.window_id)?;
    let wid = win.info.window_id.clone();
    let n = ax::names();
    let route = route_for(a.scope, pid);
    ctl.check().map_err(|h| Ctl::halt_fail(h, false))?;

    let mut outcome = Outcome {
        modality: "ax",
        reason: String::new(),
    };
    match req.action.as_ref() {
        None => return Err(Fail::nd("UNSUPPORTED_REQUEST", "no action")),
        Some(Action::Press(p)) => {
            let t = resolve(mac, pid, &win, &req.snapshot_id, &p.element_id)?;
            if !t.node.enabled {
                return Err(Fail::nd("TARGET_NOT_EDITABLE", "the element is disabled"));
            }
            if p.action == "focus" {
                t.ax.set_bool(&n.focused, true)
                    .map_err(|e| ax::ax_fail(e, "focus", true))?;
                mac.focus_hint
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert((pid, wid.clone()), t.ax.clone());
            } else {
                if !t.node.actions.contains(&p.action) {
                    return Err(Fail::nd(
                        "UNSUPPORTED_REQUEST",
                        "the element does not offer that action",
                    ));
                }
                let Some(name) = ax::ax_action(&p.action) else {
                    return Err(Fail::nd("UNSUPPORTED_REQUEST", "unknown action"));
                };
                ctl.check().map_err(|h| Ctl::halt_fail(h, false))?;
                t.ax.perform(name).map_err(|e| {
                    if e == kAXErrorActionUnsupported {
                        ax::ax_fail(e, "press", false)
                    } else {
                        ax::ax_fail(e, "press", true)
                    }
                })?;
            }
            let _ = t.index;
        }
        Some(Action::SetValue(s)) => {
            let t = resolve(mac, pid, &win, &req.snapshot_id, &s.element_id)?;
            if t.node.secure {
                return Err(Fail::nd("ACTION_UNSAFE", "a secure field is never set"));
            }
            if !t.node.settable || !t.node.enabled {
                return Err(Fail::nd(
                    "TARGET_NOT_EDITABLE",
                    "the element does not take a value",
                ));
            }
            ctl.check().map_err(|h| Ctl::halt_fail(h, false))?;
            t.ax.set_string(&n.value, &s.value)
                .map_err(|e| ax::ax_fail(e, "set value", true))?;
            if t.ax.string(&n.value).as_deref() != Some(s.value.as_str()) {
                outcome.reason = "the application normalised the value".into();
            }
        }
        Some(Action::Click(c)) => {
            outcome.modality = "coordinate";
            click(&mut outcome, &app, &win, a.scope, req, c, route, ctl)?;
        }
        Some(Action::Move(m)) => {
            outcome.modality = "coordinate";
            if a.scope != wire::Scope::Screen {
                return Err(Fail::nd(
                    "UNSUPPORTED_REQUEST",
                    "moving the pointer needs SCREEN scope",
                ));
            }
            let Some(to) = m.to.as_ref() else {
                return Err(Fail::nd("UNSUPPORTED_REQUEST", "no point"));
            };
            let (pt, _) = to_global(to, a.scope, &win.info)?;
            occlusion(a.scope, &win.info, pt)?;
            Poster::new(route).move_to(pt, ctl);
        }
        Some(Action::Drag(d)) => {
            outcome.modality = "coordinate";
            if req.window_version != win.info.version {
                return Err(stale(
                    "the window moved or was replaced since the screenshot",
                ));
            }
            let (Some(f), Some(t)) = (d.from.as_ref(), d.to.as_ref()) else {
                return Err(Fail::nd("UNSUPPORTED_REQUEST", "no points"));
            };
            let (fp, _) = to_global(f, a.scope, &win.info)?;
            let (tp, _) = to_global(t, a.scope, &win.info)?;
            occlusion(a.scope, &win.info, fp)?;
            Poster::new(route)
                .drag(fp, tp, ctl)
                .map_err(|(h, sent)| halted(h, sent))?;
            outcome.reason = "synthesised drag".into();
        }
        Some(Action::Scroll(sc)) => {
            outcome.modality = "coordinate";
            if req.window_version != win.info.version {
                return Err(stale(
                    "the window moved or was replaced since the screenshot",
                ));
            }
            let at = sc.at.unwrap_or(wire::Point { x: 640.0, y: 400.0 });
            let (pt, _) = to_global(&at, a.scope, &win.info)?;
            occlusion(a.scope, &win.info, pt)?;
            Poster::new(route).scroll(pt, sc.dx, sc.dy, ctl);
            outcome.reason = "synthesised scroll".into();
        }
        Some(Action::Type(t)) => {
            type_text(&mut outcome, mac, pid, &app, &win, route, t, ctl)?;
        }
        Some(Action::Key(k)) => {
            outcome.modality = "raw";
            outcome.reason = "keys have no element action".into();
            // Parse every chord first: an unknown key delivers nothing.
            for key in &k.keys {
                parse_chord(key).map_err(|e| Fail::nd("UNSUPPORTED_REQUEST", e))?;
            }
            let p = Poster::new(route);
            for (i, key) in k.keys.iter().enumerate() {
                ctl.check().map_err(|h| halted(h, i))?;
                p.chord(key, ctl)
                    .map_err(|e| Fail::nd("UNSUPPORTED_REQUEST", e))?;
                std::thread::sleep(Duration::from_millis(15));
            }
        }
    }

    // Let the application react, then say what changed.
    std::thread::sleep(if a.scope == wire::Scope::Screen {
        Duration::from_millis(150)
    } else {
        Duration::from_millis(90)
    });
    Ok(after(mac, pid, &app, &wid, outcome, ctl))
}

/// SCREEN scope injects at the hardware level: the point must belong to the
/// target, not to a window in front of it.
fn occlusion(scope: wire::Scope, win: &wire::WindowInfo, pt: CGPoint) -> Res<()> {
    if scope != wire::Scope::Screen {
        return Ok(());
    }
    let Ok(want) = win.window_id.parse::<u32>() else {
        return Ok(());
    };
    let top = cgwin::list(true).into_iter().find(|w| {
        w.layer == 0
            && pt.x >= w.bounds.origin.x
            && pt.y >= w.bounds.origin.y
            && pt.x < w.bounds.origin.x + w.bounds.size.width
            && pt.y < w.bounds.origin.y + w.bounds.size.height
    });
    match top {
        Some(w) if w.number == want => Ok(()),
        _ => Err(Fail::nd(
            "TARGET_OCCLUDED",
            "another window is in front of the point",
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn click(
    out: &mut Outcome,
    app: &Ax,
    win: &Window,
    scope: wire::Scope,
    req: &wire::PerformRequest,
    c: &wire::ClickAction,
    route: Route,
    ctl: &Ctl,
) -> Res<()> {
    let Some(at) = c.at.as_ref() else {
        return Err(Fail::nd("UNSUPPORTED_REQUEST", "no point"));
    };
    if req.window_version != win.info.version {
        return Err(stale(
            "the window moved or was replaced since the screenshot",
        ));
    }
    let (pt, _) = to_global(at, scope, &win.info)?;
    occlusion(scope, &win.info, pt)?;
    let button = if c.button.is_empty() {
        "left"
    } else {
        c.button.as_str()
    };
    let count = c.count.max(1);
    // Rung 2: the element under the point offers AXPress and the click is a
    // plain left click - press it without touching the pointer.
    if scope == wire::Scope::App
        && button == "left"
        && count == 1
        && let Some(el) = app.element_at(pt.x, pt.y)
        && el.actions().iter().any(|a| a == "AXPress")
        && el.boolean(&ax::names().enabled).unwrap_or(true)
    {
        el.perform("AXPress")
            .map_err(|e| ax::ax_fail(e, "press", true))?;
        out.reason = "AXPress on the element under the point; the pointer did not move".into();
        return Ok(());
    }
    Poster::new(route)
        .click(pt, button, count, ctl)
        .map_err(|(h, sent)| halted(h, sent))?;
    out.reason = if scope == wire::Scope::Screen {
        "hardware-level click at the point".into()
    } else {
        "click delivered to the process; the pointer did not move".into()
    };
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn type_text(
    out: &mut Outcome,
    mac: &Mac,
    pid: u32,
    app: &Ax,
    win: &Window,
    route: Route,
    t: &wire::TypeAction,
    ctl: &Ctl,
) -> Res<()> {
    let n = ax::names();
    out.modality = "raw";
    let hint = mac
        .focus_hint
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(pid, win.info.window_id.clone()))
        .cloned();
    // The application's own account of its focus first; the element this
    // actuator focused otherwise (a background application reports none).
    let Some(focus) = app
        .element(&n.focused_element)
        .or_else(|| hint.filter(|h| h.attr(&n.role).is_ok()))
    else {
        return Err(Fail::nd(
            "TARGET_NOT_EDITABLE",
            "nothing has the keyboard focus",
        ));
    };
    // The keyboard focus must be in the window the person approved.
    if let Some(w) = focus.element(&n.window)
        && let (Some(a), Some(b)) = (w.window_number(), win.el.window_number())
        && a != b
    {
        return Err(Fail::nd(
            "TARGET_NOT_EDITABLE",
            "the keyboard focus is in another window",
        ));
    }
    let role = focus.string(&n.role).unwrap_or_default();
    let sub = focus.string(&n.subrole).unwrap_or_default();
    if ax::role_of(&role, &sub).1 {
        return Err(Fail::nd("ACTION_UNSAFE", "the focused field is secure"));
    }
    if !t.text.is_empty() {
        let before = focus.string(&n.value);
        let via_ax = focus.settable(&n.selected_text)
            && focus.set_string(&n.selected_text, &t.text).is_ok()
            && focus.string(&n.value) != before;
        if via_ax {
            out.modality = "ax";
            out.reason = "inserted through the focused element's selected text".into();
        } else {
            out.reason = "no element action types text; synthesised key events".into();
            Poster::new(route)
                .type_text(&t.text, ctl)
                .map_err(|(h, sent)| halted(h, sent))?;
        }
    }
    match t.then.as_str() {
        "" => {}
        "enter" => {
            if focus.actions().iter().any(|a| a == "AXConfirm")
                && focus.perform("AXConfirm").is_ok()
            {
                out.reason.push_str("; confirmed through AXConfirm");
            } else {
                out.modality = "raw";
                Poster::new(route)
                    .chord("enter", ctl)
                    .map_err(|e| Fail::unknown("INPUT_FAILED", e))?;
            }
        }
        "tab" => {
            out.modality = "raw";
            Poster::new(route)
                .chord("tab", ctl)
                .map_err(|e| Fail::unknown("INPUT_FAILED", e))?;
        }
        other => {
            return Err(Fail::unknown(
                "UNSUPPORTED_REQUEST",
                format!("unknown follow-up key {other:?}"),
            ));
        }
    }
    Ok(())
}

/// Re-read the window after the effect: digest, changed elements, whether the
/// structure changed (which retires the element ids the model holds).
fn after(
    mac: &Mac,
    pid: u32,
    app: &Ax,
    wid: &str,
    out: Outcome,
    ctl: &Ctl,
) -> wire::PerformResponse {
    let mut resp = wire::PerformResponse {
        delivery: wire::Delivery::Delivered as i32,
        modality: out.modality.into(),
        modality_reason: out.reason,
        ..Default::default()
    };
    // The input is delivered; failing to look afterwards is not a failure.
    let Ok(win) = ax::find_window(app, wid) else {
        resp.structure_changed = true;
        return resp;
    };
    let before = {
        let snaps = mac.snapshots.lock().unwrap_or_else(|e| e.into_inner());
        snaps
            .get(&(pid, wid.to_owned()))
            .map(|s| (s.nodes.clone(), s.structure.clone(), s.nodes.len()))
    };
    let cap = before.as_ref().map_or(400, |b| b.2 + 64).max(400);
    // A fresh `Ctl` is not available here; reuse the call's (a Stop ends it).
    if let Ok(w) = ax::walk(&win.el, cap, Duration::from_millis(1500), ctl) {
        resp.tree_digest = ax::tree_digest(&w.nodes);
        if let Some((old, structure, _)) = before {
            let now = ax::structure_digest(&w.nodes);
            if now != structure {
                resp.structure_changed = true;
                mac.snapshots
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&(pid, wid.to_owned()));
            } else {
                for (o, nw) in old.iter().zip(w.nodes.iter()).take(old.len()) {
                    if resp.changes.len() >= 50 {
                        break;
                    }
                    let (kind, b, a) = if o.value != nw.value {
                        ("value", o.value.clone(), nw.value.clone())
                    } else if o.focused != nw.focused {
                        ("focus", String::new(), String::new())
                    } else if o.enabled != nw.enabled {
                        ("enabled", String::new(), String::new())
                    } else {
                        continue;
                    };
                    resp.changes.push(wire::ElementChange {
                        element_id: nw.element_id.clone(),
                        kind: kind.into(),
                        // A secure field has no value to report.
                        before: if o.secure { String::new() } else { b },
                        after: if nw.secure { String::new() } else { a },
                    });
                }
            }
        }
    }
    resp.window = Some(win.info);
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, parent: &str, role: &str) -> wire::AxNode {
        wire::AxNode {
            element_id: id.into(),
            parent_id: parent.into(),
            role: role.into(),
            ..Default::default()
        }
    }

    #[test]
    fn only_elements_inside_a_sheet_are_reachable_behind_one() {
        let nodes = vec![
            node("n0", "", "window"),
            node("n1", "n0", "button"),
            node("n2", "n0", "sheet"),
            node("n3", "n2", "group"),
            node("n4", "n3", "button"),
        ];
        assert!(!inside_sheet(&nodes, 1));
        assert!(inside_sheet(&nodes, 4));
        assert!(inside_sheet(&nodes, 2));
    }

    #[test]
    fn element_ids_parse_and_garbage_does_not() {
        assert_eq!(element_index("n12"), Some(12));
        assert_eq!(element_index("x"), None);
        assert_eq!(element_index(""), None);
    }
}
