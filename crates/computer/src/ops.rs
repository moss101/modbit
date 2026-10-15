//! The tool calls of native control, parsed strictly (docs/66 CUC-B01).
//!
//! Arguments are validated against what each tool advertises: an unknown or
//! misspelt argument is refused `INVALID_ARGUMENTS` with the advertised
//! names, never dropped, and no default action is substituted for a call the
//! model got wrong. The schemas the tool registry publishes are loose on
//! purpose (types only, nothing `required`), so a misspelling reaches this
//! check - and its answer - instead of a generic schema error.

use serde_json::{Value, json};

use crate::canvas::{CANVAS_HEIGHT, CANVAS_WIDTH};
use crate::model::Scope;
use crate::taxonomy::{Code, Refusal};

/// Every tool of the family, in the order the contract lists them.
pub const TOOLS: &[&str] = &[
    "computer.apps",
    "computer.resolve",
    "computer.start",
    "computer.start_screen",
    "computer.release",
    "computer.state",
    "computer.screenshot",
    "computer.wait",
    "computer.press",
    "computer.set_value",
    "computer.click",
    "computer.move",
    "computer.drag",
    "computer.type",
    "computer.key",
    "computer.scroll",
];

/// The arguments a tool advertises.
#[must_use]
pub fn advertised(tool: &str) -> Option<&'static [&'static str]> {
    Some(match tool {
        "computer.apps" | "computer.release" => &[],
        "computer.resolve" => &["application", "pid"],
        "computer.start" | "computer.start_screen" => &["application", "pid", "window", "reason"],
        "computer.state" => &["window", "max_nodes"],
        "computer.screenshot" => &["window"],
        "computer.wait" => &["ms", "until_change"],
        "computer.press" => &["element", "action"],
        "computer.set_value" => &["element", "value"],
        "computer.click" => &["frame", "x", "y", "button", "count"],
        "computer.move" => &["frame", "x", "y"],
        "computer.drag" => &["frame", "from", "to"],
        "computer.type" => &["text", "then"],
        "computer.key" => &["keys", "allow_destructive"],
        "computer.scroll" => &["frame", "x", "y", "dx", "dy"],
        _ => return None,
    })
}

/// A handle to an element: the snapshot of the tree it came from and its id
/// within it (`s3a9f1c/e12`). Valid only while that snapshot is current.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementRef {
    /// The snapshot id.
    pub snapshot: String,
    /// The element id within it.
    pub id: String,
}

impl ElementRef {
    /// The text form.
    #[must_use]
    pub fn text(&self) -> String {
        format!("{}/{}", self.snapshot, self.id)
    }
}

/// An input.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// An element action.
    Press {
        /// The element.
        element: ElementRef,
        /// The action (`press` by default).
        action: String,
    },
    /// Set a settable element's value.
    SetValue {
        /// The element.
        element: ElementRef,
        /// The value (shown in full on the approval).
        value: String,
    },
    /// A click at canvas coordinates against a screenshot.
    Click {
        /// The frame token.
        frame: String,
        /// Canvas x.
        x: f64,
        /// Canvas y.
        y: f64,
        /// `left` | `right` | `middle`.
        button: String,
        /// Click count.
        count: u32,
    },
    /// Move the pointer (SCREEN scope).
    Move {
        /// The frame token.
        frame: String,
        /// Canvas x.
        x: f64,
        /// Canvas y.
        y: f64,
    },
    /// Drag between two canvas points.
    Drag {
        /// The frame token.
        frame: String,
        /// From.
        from: (f64, f64),
        /// To.
        to: (f64, f64),
    },
    /// Type text to the verified focus.
    Type {
        /// The text (no control characters; shown in full on the approval).
        text: String,
        /// A separate key after the text: `enter` | `tab`.
        then: Option<String>,
    },
    /// Press keys.
    Key {
        /// Key chords (`cmd+a`, `tab`).
        keys: Vec<String>,
        /// The call names destructive keys and the approval shows them.
        allow_destructive: bool,
    },
    /// Scroll at a canvas point.
    Scroll {
        /// The frame token.
        frame: String,
        /// Canvas x.
        x: f64,
        /// Canvas y.
        y: f64,
        /// Horizontal amount.
        dx: f64,
        /// Vertical amount.
        dy: f64,
    },
}

impl Act {
    /// The tool-name suffix and audit kind.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Act::Press { .. } => "press",
            Act::SetValue { .. } => "set_value",
            Act::Click { .. } => "click",
            Act::Move { .. } => "move",
            Act::Drag { .. } => "drag",
            Act::Type { .. } => "type",
            Act::Key { .. } => "key",
            Act::Scroll { .. } => "scroll",
        }
    }

    /// The rung of the action ladder the tool starts on (CUC-A04).
    #[must_use]
    pub const fn modality(&self) -> &'static str {
        match self {
            Act::Press { .. } | Act::SetValue { .. } => "ax",
            Act::Click { .. } | Act::Move { .. } | Act::Drag { .. } | Act::Scroll { .. } => {
                "coordinate"
            }
            Act::Type { .. } | Act::Key { .. } => "raw",
        }
    }

    /// The action and its arguments in full, as the approval shows them.
    #[must_use]
    pub fn arguments(&self) -> Value {
        match self {
            Act::Press { element, action } => json!({"element": element.text(), "action": action}),
            Act::SetValue { element, value } => json!({"element": element.text(), "value": value}),
            Act::Click {
                frame,
                x,
                y,
                button,
                count,
            } => {
                json!({"frame": frame, "x": x, "y": y, "button": button, "count": count})
            }
            Act::Move { frame, x, y } => json!({"frame": frame, "x": x, "y": y}),
            Act::Drag { frame, from, to } => json!({
                "frame": frame,
                "from": {"x": from.0, "y": from.1},
                "to": {"x": to.0, "y": to.1},
            }),
            Act::Type { text, then } => json!({"text": text, "then": then}),
            Act::Key {
                keys,
                allow_destructive,
            } => {
                json!({"keys": keys, "allow_destructive": allow_destructive})
            }
            Act::Scroll {
                frame,
                x,
                y,
                dx,
                dy,
            } => {
                json!({"frame": frame, "x": x, "y": y, "dx": dx, "dy": dy})
            }
        }
    }

    /// The frame token a coordinate action names.
    #[must_use]
    pub fn frame(&self) -> Option<&str> {
        match self {
            Act::Click { frame, .. }
            | Act::Move { frame, .. }
            | Act::Drag { frame, .. }
            | Act::Scroll { frame, .. } => Some(frame),
            _ => None,
        }
    }

    /// The element an action names.
    #[must_use]
    pub fn element(&self) -> Option<&ElementRef> {
        match self {
            Act::Press { element, .. } | Act::SetValue { element, .. } => Some(element),
            _ => None,
        }
    }

    /// Whether the action moves the real pointer or needs the display, which
    /// only SCREEN scope has.
    #[must_use]
    pub const fn needs_screen(&self) -> bool {
        matches!(self, Act::Move { .. })
    }
}

/// A call to a tool of the family.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// List running applications.
    Apps,
    /// Resolve one without activating it.
    Resolve {
        /// Name or bundle identifier.
        application: String,
        /// A process id, to tell look-alikes apart.
        pid: Option<u32>,
    },
    /// Open a control session (an observation grant).
    Start {
        /// Name or bundle identifier.
        application: String,
        /// A process id.
        pid: Option<u32>,
        /// A window id; default the main window.
        window: Option<String>,
        /// Why, shown on the approval.
        reason: String,
        /// APP for `computer.start`, SCREEN for `computer.start_screen`.
        scope: Scope,
    },
    /// End the session.
    Release,
    /// Read the accessibility tree.
    State {
        /// A window id.
        window: Option<String>,
        /// Most nodes.
        max_nodes: Option<u32>,
    },
    /// Capture the window.
    Screenshot {
        /// A window id.
        window: Option<String>,
    },
    /// Wait.
    Wait {
        /// Milliseconds, at most 10 000.
        ms: u64,
        /// Return early when the tree changes.
        until_change: bool,
    },
    /// An input.
    Act(Act),
}

impl Op {
    /// Whether the call is an input or a start - what a person approves.
    #[must_use]
    pub const fn needs_approval(&self) -> bool {
        matches!(self, Op::Start { .. } | Op::Act(_))
    }
}

fn invalid(tool: &str, message: impl Into<String>) -> Refusal {
    Refusal::new(Code::InvalidArguments, message).with_facts(json!({
        "tool": tool,
        "advertised": advertised(tool).unwrap_or(&[]),
    }))
}

fn str_arg(tool: &str, args: &Value, key: &str, max: usize) -> Result<Option<String>, Refusal> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.chars().count() <= max => Ok(Some(s.clone())),
        Some(Value::String(_)) => Err(invalid(
            tool,
            format!("`{key}` is longer than {max} characters"),
        )),
        Some(_) => Err(invalid(tool, format!("`{key}` must be a string"))),
    }
}

fn need_str(tool: &str, args: &Value, key: &str, max: usize) -> Result<String, Refusal> {
    match str_arg(tool, args, key, max)? {
        Some(s) if !s.is_empty() => Ok(s),
        _ => Err(invalid(tool, format!("`{key}` is required"))),
    }
}

fn num_arg(tool: &str, args: &Value, key: &str) -> Result<Option<f64>, Refusal> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|f| f.is_finite())
            .map(Some)
            .ok_or_else(|| invalid(tool, format!("`{key}` must be a number"))),
    }
}

fn need_canvas_point(tool: &str, args: &Value, xk: &str, yk: &str) -> Result<(f64, f64), Refusal> {
    let (Some(x), Some(y)) = (num_arg(tool, args, xk)?, num_arg(tool, args, yk)?) else {
        return Err(invalid(tool, format!("`{xk}` and `{yk}` are required")));
    };
    canvas_point(tool, x, y)
}

fn canvas_point(tool: &str, x: f64, y: f64) -> Result<(f64, f64), Refusal> {
    if (0.0..=f64::from(CANVAS_WIDTH)).contains(&x) && (0.0..=f64::from(CANVAS_HEIGHT)).contains(&y)
    {
        Ok((x, y))
    } else {
        Err(invalid(
            tool,
            format!(
                "({x}, {y}) is outside the {CANVAS_WIDTH} x {CANVAS_HEIGHT} canvas; coordinates are pixels of the screenshot, origin top left"
            ),
        ))
    }
}

fn point_obj(tool: &str, args: &Value, key: &str) -> Result<(f64, f64), Refusal> {
    let Some(o) = args.get(key).filter(|v| v.is_object()) else {
        return Err(invalid(tool, format!("`{key}` must be an object {{x, y}}")));
    };
    if let Some(extra) = o
        .as_object()
        .and_then(|m| m.keys().find(|k| *k != "x" && *k != "y"))
    {
        return Err(invalid(
            tool,
            format!("`{key}` has an unknown member `{extra}`; it is {{x, y}}"),
        ));
    }
    need_canvas_point(tool, o, "x", "y")
}

/// Parse an element reference `snapshot/id`.
fn element_ref(tool: &str, args: &Value) -> Result<ElementRef, Refusal> {
    let text = need_str(tool, args, "element", 200)?;
    match text.split_once('/') {
        Some((s, id)) if !s.is_empty() && !id.is_empty() => Ok(ElementRef {
            snapshot: s.to_owned(),
            id: id.to_owned(),
        }),
        _ => Err(invalid(
            tool,
            "`element` is the id exactly as computer.state lists it: `<snapshot>/<element>`",
        )),
    }
}

/// Key chords whose effect cannot be taken back, normalised: lower case,
/// modifiers sorted. A chord whose last key is a delete key is destructive too.
const DESTRUCTIVE_CHORDS: &[&str] = &[
    "cmd+q",
    "cmd+w",
    "cmd+x",
    "cmd+backspace",
    "cmd+delete",
    "alt+cmd+esc",
    "cmd+ctrl+q",
    "alt+f4",
    "ctrl+w",
    "ctrl+x",
    "ctrl+q",
    "ctrl+alt+delete",
    "cmd+shift+q",
    "cmd+shift+backspace",
];

/// Whether a key chord is destructive (CUC-B05: delete, backspace on a
/// selection, quit, close, cut, lock).
#[must_use]
pub fn destructive_key(chord: &str) -> bool {
    let norm = normalise_chord(chord);
    if DESTRUCTIVE_CHORDS.contains(&norm.as_str()) {
        return true;
    }
    let last = norm.rsplit('+').next().unwrap_or("");
    matches!(last, "delete" | "backspace" | "forwarddelete" | "del")
}

fn normalise_chord(chord: &str) -> String {
    let mut parts: Vec<String> = chord
        .split('+')
        .map(|p| {
            let p = p.trim().to_ascii_lowercase();
            match p.as_str() {
                "command" | "meta" | "super" | "win" => "cmd".to_owned(),
                "control" => "ctrl".to_owned(),
                "option" => "alt".to_owned(),
                _ => p,
            }
        })
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() > 1 {
        let last = parts.pop().unwrap_or_default();
        parts.sort();
        parts.push(last);
    }
    parts.join("+")
}

/// Parse a call.
///
/// # Errors
/// `UNKNOWN_TOOL` for a name outside the family; `INVALID_ARGUMENTS` for an
/// unknown, missing, mistyped or out-of-range argument, with the advertised
/// names; `ACTION_UNSAFE` for a destructive key the call did not name.
pub fn parse(tool: &str, args: &Value) -> Result<Op, Refusal> {
    let Some(allowed) = advertised(tool) else {
        return Err(Refusal::new(
            Code::UnknownTool,
            format!("`{tool}` is not a computer tool"),
        )
        .with_facts(json!({"tools": TOOLS})));
    };
    let Some(map) = args.as_object() else {
        return Err(invalid(tool, "the arguments must be an object"));
    };
    let unknown: Vec<&String> = map
        .keys()
        .filter(|k| !allowed.contains(&k.as_str()))
        .collect();
    if !unknown.is_empty() {
        return Err(invalid(
            tool,
            format!(
                "unknown argument(s) {}; `{tool}` takes {}",
                unknown
                    .iter()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                if allowed.is_empty() {
                    "none".to_owned()
                } else {
                    allowed
                        .iter()
                        .map(|k| format!("`{k}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ),
        )
        .with_facts(json!({
            "tool": tool,
            "advertised": allowed,
            "unknown": unknown,
        })));
    }
    match tool {
        "computer.apps" => Ok(Op::Apps),
        "computer.release" => Ok(Op::Release),
        "computer.resolve" => Ok(Op::Resolve {
            application: need_str(tool, args, "application", 200)?,
            pid: pid_arg(tool, args)?,
        }),
        "computer.start" | "computer.start_screen" => Ok(Op::Start {
            application: need_str(tool, args, "application", 200)?,
            pid: pid_arg(tool, args)?,
            window: str_arg(tool, args, "window", 200)?.filter(|w| !w.is_empty()),
            reason: str_arg(tool, args, "reason", 300)?.unwrap_or_default(),
            scope: if tool == "computer.start" {
                Scope::App
            } else {
                Scope::Screen
            },
        }),
        "computer.state" => Ok(Op::State {
            window: str_arg(tool, args, "window", 200)?.filter(|w| !w.is_empty()),
            max_nodes: match num_arg(tool, args, "max_nodes")? {
                None => None,
                Some(n) if (1.0..=2000.0).contains(&n) && n.fract() == 0.0 => Some(n as u32),
                Some(_) => {
                    return Err(invalid(
                        tool,
                        "`max_nodes` is a whole number from 1 to 2000",
                    ));
                }
            },
        }),
        "computer.screenshot" => Ok(Op::Screenshot {
            window: str_arg(tool, args, "window", 200)?.filter(|w| !w.is_empty()),
        }),
        "computer.wait" => {
            let ms = num_arg(tool, args, "ms")?.unwrap_or(1000.0);
            if !(0.0..=10_000.0).contains(&ms) {
                return Err(invalid(tool, "`ms` is 0 to 10000"));
            }
            let until_change = match args.get("until_change") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(b)) => *b,
                Some(_) => return Err(invalid(tool, "`until_change` must be true or false")),
            };
            Ok(Op::Wait {
                ms: ms as u64,
                until_change,
            })
        }
        "computer.press" => Ok(Op::Act(Act::Press {
            element: element_ref(tool, args)?,
            action: str_arg(tool, args, "action", 64)?
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| "press".to_owned()),
        })),
        "computer.set_value" => {
            let element = element_ref(tool, args)?;
            let Some(value) = str_arg(tool, args, "value", 4000)? else {
                return Err(invalid(
                    tool,
                    "`value` is required (an empty string clears the field)",
                ));
            };
            Ok(Op::Act(Act::SetValue { element, value }))
        }
        "computer.click" => {
            let frame = need_str(tool, args, "frame", 64)?;
            let (x, y) = need_canvas_point(tool, args, "x", "y")?;
            let button = str_arg(tool, args, "button", 16)?.unwrap_or_else(|| "left".to_owned());
            if !["left", "right", "middle"].contains(&button.as_str()) {
                return Err(invalid(tool, "`button` is left, right or middle"));
            }
            let count = match num_arg(tool, args, "count")? {
                None => 1,
                Some(n) if [1.0, 2.0, 3.0].contains(&n) => n as u32,
                Some(_) => return Err(invalid(tool, "`count` is 1, 2 or 3")),
            };
            Ok(Op::Act(Act::Click {
                frame,
                x,
                y,
                button,
                count,
            }))
        }
        "computer.move" => {
            let frame = need_str(tool, args, "frame", 64)?;
            let (x, y) = need_canvas_point(tool, args, "x", "y")?;
            Ok(Op::Act(Act::Move { frame, x, y }))
        }
        "computer.drag" => Ok(Op::Act(Act::Drag {
            frame: need_str(tool, args, "frame", 64)?,
            from: point_obj(tool, args, "from")?,
            to: point_obj(tool, args, "to")?,
        })),
        "computer.type" => {
            let Some(text) = str_arg(tool, args, "text", 2000)? else {
                return Err(invalid(tool, "`text` is required"));
            };
            if text.chars().any(|c| c.is_control()) {
                return Err(invalid(
                    tool,
                    "`text` carries a newline, tab or other control character; those are separate: give the text, and `then: \"enter\"` or `then: \"tab\"`",
                ));
            }
            let then = str_arg(tool, args, "then", 16)?.filter(|t| !t.is_empty());
            if let Some(t) = &then
                && !["enter", "tab"].contains(&t.as_str())
            {
                return Err(invalid(tool, "`then` is enter or tab"));
            }
            if text.is_empty() && then.is_none() {
                return Err(invalid(tool, "`text` is empty and `then` names no key"));
            }
            Ok(Op::Act(Act::Type { text, then }))
        }
        "computer.key" => {
            let keys: Vec<String> = match args.get("keys") {
                Some(Value::Array(a)) if !a.is_empty() && a.len() <= 16 => a
                    .iter()
                    .map(|k| {
                        k.as_str()
                            .filter(|s| !s.is_empty() && s.len() <= 48)
                            .map(str::to_owned)
                    })
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| {
                        invalid(
                            tool,
                            "`keys` are non-empty strings such as `cmd+a` or `tab`",
                        )
                    })?,
                _ => return Err(invalid(tool, "`keys` is a list of 1 to 16 key chords")),
            };
            let allow_destructive = match args.get("allow_destructive") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(b)) => *b,
                Some(_) => return Err(invalid(tool, "`allow_destructive` must be true or false")),
            };
            if !allow_destructive && let Some(k) = keys.iter().find(|k| destructive_key(k)) {
                return Err(Refusal::new(
                    Code::ActionUnsafe,
                    format!(
                        "`{k}` is destructive (delete, quit, close, cut or lock); the call must name it with `allow_destructive: true` so the approval shows it"
                    ),
                ));
            }
            Ok(Op::Act(Act::Key {
                keys,
                allow_destructive,
            }))
        }
        "computer.scroll" => {
            let frame = need_str(tool, args, "frame", 64)?;
            let (x, y) = need_canvas_point(tool, args, "x", "y")?;
            let dx = num_arg(tool, args, "dx")?.unwrap_or(0.0);
            let dy = num_arg(tool, args, "dy")?.unwrap_or(0.0);
            if dx == 0.0 && dy == 0.0 {
                return Err(invalid(tool, "`dx` or `dy` must be non-zero"));
            }
            if dx.abs() > 5000.0 || dy.abs() > 5000.0 {
                return Err(invalid(tool, "`dx` and `dy` are at most 5000"));
            }
            Ok(Op::Act(Act::Scroll {
                frame,
                x,
                y,
                dx,
                dy,
            }))
        }
        _ => unreachable!("advertised() admitted the name"),
    }
}

fn pid_arg(tool: &str, args: &Value) -> Result<Option<u32>, Refusal> {
    match num_arg(tool, args, "pid")? {
        None => Ok(None),
        Some(n) if n >= 1.0 && n.fract() == 0.0 && n <= f64::from(u32::MAX) => Ok(Some(n as u32)),
        Some(_) => Err(invalid(tool, "`pid` is a process id")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_advertises_its_arguments_and_unknown_names_are_refused_with_them() {
        for t in TOOLS {
            assert!(advertised(t).is_some(), "{t}");
        }
        let e = parse("computer.press", &json!({"elemnt": "s1/e1"})).unwrap_err();
        assert_eq!(e.code, Code::InvalidArguments);
        assert_eq!(e.facts["advertised"], json!(["element", "action"]));
        assert_eq!(e.facts["unknown"], json!(["elemnt"]));
        assert!(
            e.message.contains("`elemnt`") && e.message.contains("`element`"),
            "{}",
            e.message
        );
        // A missing required argument names the advertised ones too.
        let e = parse("computer.press", &json!({})).unwrap_err();
        assert_eq!(e.facts["advertised"], json!(["element", "action"]));
        assert_eq!(
            parse("computer.nope", &json!({})).unwrap_err().code,
            Code::UnknownTool
        );
        assert_eq!(
            parse("computer.apps", &json!({"x": 1})).unwrap_err().code,
            Code::InvalidArguments
        );
        assert_eq!(
            parse("computer.apps", &json!("x")).unwrap_err().code,
            Code::InvalidArguments
        );
    }

    #[test]
    fn actions_parse_with_their_defaults_and_ranges() {
        let Op::Act(Act::Press { element, action }) =
            parse("computer.press", &json!({"element": "s1/e12"})).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            (
                element.snapshot.as_str(),
                element.id.as_str(),
                action.as_str()
            ),
            ("s1", "e12", "press")
        );
        assert!(parse("computer.press", &json!({"element": "e12"})).is_err());
        let Op::Act(Act::Click {
            x,
            y,
            button,
            count,
            ..
        }) = parse(
            "computer.click",
            &json!({"frame": "f1", "x": 10, "y": 20.5}),
        )
        .unwrap()
        else {
            panic!()
        };
        assert_eq!((x, y, button.as_str(), count), (10.0, 20.5, "left", 1));
        assert!(parse("computer.click", &json!({"frame": "f1", "x": 1281, "y": 5})).is_err());
        assert!(
            parse("computer.click", &json!({"x": 1, "y": 5})).is_err(),
            "a coordinate needs its frame"
        );
        assert!(
            parse(
                "computer.drag",
                &json!({"frame": "f", "from": {"x": 1, "y": 2}, "to": {"x": 3, "y": 4, "z": 1}})
            )
            .is_err()
        );
        assert!(matches!(
            parse("computer.wait", &json!({})).unwrap(),
            Op::Wait {
                ms: 1000,
                until_change: false
            }
        ));
        assert!(parse("computer.wait", &json!({"ms": 20000})).is_err());
        assert!(matches!(
            parse("computer.start_screen", &json!({"application": "Notes"})).unwrap(),
            Op::Start {
                scope: Scope::Screen,
                ..
            }
        ));
    }

    #[test]
    fn newlines_tabs_and_destructive_keys_are_separate_arguments() {
        assert!(parse("computer.type", &json!({"text": "a\nb"})).is_err());
        assert!(parse("computer.type", &json!({"text": "a\tb"})).is_err());
        assert!(matches!(
            parse("computer.type", &json!({"text": "hello", "then": "enter"})).unwrap(),
            Op::Act(Act::Type { then: Some(_), .. })
        ));
        assert!(parse("computer.type", &json!({"text": "x", "then": "space"})).is_err());
        for k in [
            "Delete",
            "backspace",
            "cmd+q",
            "Command+W",
            "cmd+shift+backspace",
            "alt+F4",
            "ctrl+cmd+q",
        ] {
            assert!(destructive_key(k), "{k}");
            let e = parse("computer.key", &json!({"keys": [k]})).unwrap_err();
            assert_eq!(e.code, Code::ActionUnsafe, "{k}");
            assert!(
                parse(
                    "computer.key",
                    &json!({"keys": [k], "allow_destructive": true})
                )
                .is_ok()
            );
        }
        for k in ["tab", "cmd+a", "enter", "shift+tab", "cmd+c"] {
            assert!(!destructive_key(k), "{k}");
            assert!(parse("computer.key", &json!({"keys": [k]})).is_ok());
        }
    }
}
