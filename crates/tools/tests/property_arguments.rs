//! M9.6 (docs/52 "Security gates": fuzzer for tool argument validation;
//! "Shell injection" attack test). Every registered direct tool's input
//! schema is a real validator that accepts or refuses arbitrary JSON without
//! panicking, refuses a document with a key the schema does not declare
//! (`additionalProperties: false` on every tool), and bounds every string
//! the schema bounds. The shell tools take `argv`, never a command line, so
//! a hostile argument reaches the process as one literal token.

use modbit_tools::ToolRegistry;
use proptest::prelude::*;
use serde_json::{Value, json};

fn registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    modbit_tools::direct::register_direct(&mut registry).unwrap();
    registry
}

fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| json!(n)),
        any::<f64>()
            .prop_filter("finite", |f| f.is_finite())
            .prop_map(|f| json!(f)),
        "[ -~]{0,64}".prop_map(Value::String),
        prop::collection::vec(any::<u8>(), 0..64)
            .prop_map(|b| Value::String(String::from_utf8_lossy(&b).into_owned())),
    ];
    leaf.prop_recursive(4, 64, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            prop::collection::btree_map("[a-z_]{1,12}", inner, 0..6)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn every_tool_schema_validates_arbitrary_json_without_panicking(v in json_value(), pick in any::<prop::sample::Index>()) {
        let registry = registry();
        let specs = registry.specs();
        let spec = pick.get(&specs);
        let validator = jsonschema::validator_for(&spec.input_schema).unwrap();
        // Accept or refuse; never panic.
        let _ = validator.validate(&v);
        // A document with an undeclared key is refused by every tool.
        if let Value::Object(mut m) = v {
            m.insert("__undeclared_by_any_tool__".into(), json!(1));
            prop_assert!(validator.validate(&Value::Object(m)).is_err(), "{} accepts an undeclared key", spec.name);
        }
    }

    #[test]
    fn declared_string_bounds_hold_for_every_tool(over in 1usize..512, pick in any::<prop::sample::Index>()) {
        let registry = registry();
        let specs = registry.specs();
        let spec = pick.get(&specs);
        let validator = jsonschema::validator_for(&spec.input_schema).unwrap();
        if let Some(props) = spec.input_schema["properties"].as_object() {
            for (name, p) in props {
                if let Some(max) = p["maxLength"].as_u64() {
                    let too_long = json!({ name: "x".repeat(max as usize + over) });
                    prop_assert!(validator.validate(&too_long).is_err(), "{}.{name} accepts {} chars over maxLength {max}", spec.name, over);
                }
            }
        }
    }
}

/// Every direct tool declares `additionalProperties: false` at the top level:
/// the property above relies on it, and a tool that forgot would accept
/// smuggled fields (docs/52 "Malicious skill/plugin/MCP").
#[test]
fn every_direct_tool_closes_its_top_level_schema() {
    for spec in registry().specs() {
        assert_eq!(
            spec.input_schema["additionalProperties"],
            json!(false),
            "{} does not close its input schema",
            spec.name
        );
        assert_eq!(spec.input_schema["type"], json!("object"), "{}", spec.name);
    }
}
