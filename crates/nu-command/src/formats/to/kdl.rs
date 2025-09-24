use base64::{Engine, engine::general_purpose};
use kdl::{FormatConfigBuilder, KdlDocument, KdlEntry, KdlIdentifier, KdlNode, KdlValue};
use nu_engine::command_prelude::*;
use nu_protocol::{PipelineMetadata, Range, ast::PathMember};
use std::ops::{Bound, Deref};

#[derive(Clone)]
pub struct ToKdl;

impl Command for ToKdl {
    fn name(&self) -> &str {
        "to kdl"
    }

    fn signature(&self) -> Signature {
        Signature::build("to kdl")
            .input_output_types(vec![(Type::Any, Type::String)])
            .switch(
                "raw",
                "remove all of the whitespace and trailing line ending",
                Some('r'),
            )
            .category(Category::Formats)
    }

    fn description(&self) -> &str {
        "Converts values to KDL text following the JSON-IN-KDL specification."
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let raw = call.has_flag(engine_state, stack, "raw")?;
        let span = call.head;

        // allow ranges to expand and turn into array
        let input = input.try_expand_range()?;
        let value = input.into_value(span)?;

        let mut kdl_document = value_to_kdl_document(engine_state, &value, span)?;
        let format_config = if raw {
            FormatConfigBuilder::new()
                .indent_level(0)
                .indent("")
                .no_comments(true)
                .build()
        } else {
            FormatConfigBuilder::new()
                .indent_level(0)
                .indent("  ")
                .no_comments(false)
                .build()
        };
        kdl_document.autoformat_config(&format_config);

        let res = Value::string(kdl_document.to_string(), span);
        let metadata = PipelineMetadata {
            data_source: nu_protocol::DataSource::None,
            content_type: Some("application/kdl".to_string()),
        };
        Ok(PipelineData::value(res, Some(metadata)))
    }

    fn examples(&self) -> Vec<Example<'_>> {
        vec![
            Example {
                description: "Convert a simple object to KDL following JSON-IN-KDL spec",
                example: r#"{name: "Alice", age: 30} | to kdl"#,
                result: Some(Value::test_string("- {\n  name Alice\n  age 30\n}\n")),
            },
            Example {
                description: "Convert an array to KDL following JSON-IN-KDL spec",
                example: r#"[1, 2, 3] | to kdl"#,
                result: Some(Value::test_string("- 1 2 3\n")),
            },
            Example {
                description: "Convert nested structures to KDL following JSON-IN-KDL spec",
                example: r#"{users: [{name: "Alice", active: true}, {name: "Bob", active: false}]} | to kdl"#,
                result: Some(Value::test_string(
                    "- {\n  users {\n    - name=Alice\n    - active=#true\n    - name=Bob\n    - active=#false\n    }\n}\n",
                )),
            },
            Example {
                description: "Convert a primitive value to KDL",
                example: r#"42 | to kdl"#,
                result: Some(Value::test_string("- 42\n")),
            },
        ]
    }
}

pub fn value_to_kdl_document(
    engine_state: &EngineState,
    v: &Value,
    call_span: Span,
) -> Result<KdlDocument, ShellError> {
    let mut document = KdlDocument::new();

    // JSON-IN-KDL requires a single top-level node representing the JSON value
    let node = value_to_jik_node(engine_state, v, call_span)?;
    document.nodes_mut().push(node);

    Ok(document)
}

/// Convert a Nushell Value to a JSON-IN-KDL node
fn value_to_jik_node(
    engine_state: &EngineState,
    value: &Value,
    call_span: Span,
) -> Result<KdlNode, ShellError> {
    // Use "-" as the node name per JSON-IN-KDL specification
    let node_name = KdlIdentifier::from("-");
    let mut node = KdlNode::new(node_name);

    match value {
        Value::Record { val, .. } => {
            if val.is_empty() {
                // Empty object requires type annotation
                node.set_ty("object");
            } else {
                // Object: all properties become child nodes (per first option of JSON-IN-KDL spec)
                let mut child_doc = KdlDocument::new();

                for (key, val) in &**val {
                    match val {
                        Value::Record { .. } => {
                            // Nested object goes to child nodes
                            let mut child_node = value_to_jik_node(engine_state, val, call_span)?;
                            // Set the child node's name to the key
                            let child_name = KdlIdentifier::from(key.as_str());
                            child_node.set_name(child_name);
                            child_doc.nodes_mut().push(child_node);
                        }
                        Value::List { .. } => {
                            // Nested list goes to child nodes
                            let mut child_node = value_to_jik_node(engine_state, val, call_span)?;
                            // Set the child node's name to the key
                            let child_name = KdlIdentifier::from(key.as_str());
                            child_node.set_name(child_name);
                            child_doc.nodes_mut().push(child_node);
                        }
                        _ => {
                            // Simple value becomes a child node with the key as name and value as argument
                            let child_name = KdlIdentifier::from(key.as_str());
                            let mut child_node = KdlNode::new(child_name);
                            let entry = value_to_jik_entry(engine_state, val, call_span)?;
                            child_node.entries_mut().push(entry);
                            child_doc.nodes_mut().push(child_node);
                        }
                    }
                }

                node.set_children(child_doc);
            }
        }
        Value::List { vals, .. } => {
            if vals.is_empty() {
                // Empty array requires type annotation
                node.set_ty("array");
            } else if vals.len() == 1 {
                // Single-element array requires type annotation
                node.set_ty("array");
                add_array_element_to_node(engine_state, &mut node, &vals[0], call_span)?;
            } else {
                // Multi-element array: add elements as arguments or child nodes
                let mut child_doc = KdlDocument::new();
                let mut has_children = false;

                for val in vals {
                    match val {
                        Value::Record {
                            val: record_val, ..
                        } => {
                            // Record values in arrays become child nodes with "- property=value" format
                            for (key, value) in &**record_val {
                                let child_name = KdlIdentifier::from("-");
                                let mut child_node = KdlNode::new(child_name);

                                // Add the property as a named entry
                                let mut entry = value_to_jik_entry(engine_state, value, call_span)?;
                                let prop_name = KdlIdentifier::from(key.as_str());
                                entry.set_name(Some(prop_name));
                                child_node.entries_mut().push(entry);

                                child_doc.nodes_mut().push(child_node);
                            }
                            has_children = true;
                        }
                        Value::List { .. } => {
                            // Nested arrays go to child nodes
                            let child_node = value_to_jik_node(engine_state, val, call_span)?;
                            child_doc.nodes_mut().push(child_node);
                            has_children = true;
                        }
                        _ => {
                            // Simple values go to arguments
                            let entry = value_to_jik_entry(engine_state, val, call_span)?;
                            node.entries_mut().push(entry);
                        }
                    }
                }

                if has_children {
                    node.set_children(child_doc);
                }
            }
        }
        _ => {
            // Primitive value becomes a single argument
            let entry = value_to_jik_entry(engine_state, value, call_span)?;
            node.entries_mut().push(entry);
        }
    }

    Ok(node)
}

/// Helper function to add an array element to a node (for single-element arrays)
fn add_array_element_to_node(
    engine_state: &EngineState,
    node: &mut KdlNode,
    val: &Value,
    call_span: Span,
) -> Result<(), ShellError> {
    match val {
        Value::Record { .. } | Value::List { .. } => {
            // Complex value goes to child nodes
            let mut child_doc = KdlDocument::new();
            let child_node = value_to_jik_node(engine_state, val, call_span)?;
            child_doc.nodes_mut().push(child_node);
            node.set_children(child_doc);
        }
        _ => {
            // Simple value goes to arguments
            let entry = value_to_jik_entry(engine_state, val, call_span)?;
            node.entries_mut().push(entry);
        }
    }
    Ok(())
}

/// Convert a Nushell Value to a KDL entry (for use as argument or property value)
#[allow(clippy::only_used_in_recursion)]
fn value_to_jik_entry(
    engine_state: &EngineState,
    v: &Value,
    call_span: Span,
) -> Result<KdlEntry, ShellError> {
    let span = v.span();
    Ok(match v {
        Value::Bool { val, .. } => KdlValue::Bool(*val).into(),
        Value::Int { val, .. } => KdlValue::Integer(*val as i128).into(),
        Value::Float { val, .. } => KdlValue::Float(*val).into(),
        Value::String { val, .. } => KdlValue::String(val.clone()).into(),
        Value::Glob { val, .. } => KdlValue::String(val.to_string()).into(),
        Value::Nothing { .. } => KdlValue::Null.into(),
        Value::Filesize { val, .. } => KdlValue::Integer(val.get() as i128).into(),
        Value::Duration { val, .. } => KdlValue::Integer(*val as i128).into(),
        Value::Binary { val, .. } => {
            // Convert binary to base64 string with type annotation
            let mut entry: KdlEntry =
                KdlValue::String(general_purpose::STANDARD.encode(val)).into();
            entry.set_ty("base64");
            entry
        }
        Value::Date { val, .. } => {
            let date_str = val.to_rfc3339();
            let mut entry: KdlEntry = KdlValue::String(date_str).into();
            entry.set_ty("date-time");
            entry
        }
        Value::CellPath { val, .. } => {
            let path_str = val
                .members
                .iter()
                .map(|member| match member {
                    PathMember::String { val, .. } => val.clone(),
                    PathMember::Int { val, .. } => val.to_string(),
                })
                .collect::<Vec<_>>()
                .join(".");
            KdlValue::String(path_str).into()
        }
        Value::Range { val, .. } => {
            let range_str = match val.deref() {
                Range::IntRange(int_range) => match int_range.end() {
                    Bound::Included(e) => format!("{}..={}", int_range.start(), e),
                    Bound::Excluded(e) => format!("{}..{}", int_range.start(), e - 1),
                    Bound::Unbounded => format!("{}..", int_range.start()),
                },
                Range::FloatRange(float_range) => match float_range.end() {
                    Bound::Included(e) => format!("{}..={}", float_range.start(), e),
                    Bound::Excluded(e) => format!("{}..{}", float_range.start(), e),
                    Bound::Unbounded => format!("{}..", float_range.start()),
                },
            };
            KdlValue::String(range_str).into()
        }
        Value::Closure { .. } => {
            let str = "<closure>".to_string();
            KdlValue::String(str).into()
        }
        Value::Record { .. } | Value::List { .. } => {
            // These should be handled at the node level, not as entries
            return Err(ShellError::UnsupportedInput {
                msg: "complex structures should be handled as child nodes, not entry values".into(),
                input: "nested structure as entry".into(),
                msg_span: call_span,
                input_span: span,
            });
        }
        Value::Error { error, .. } => return Err(*error.clone()),
        Value::Custom { val, .. } => {
            let collected = val.to_base_value(span)?;
            return value_to_jik_entry(engine_state, &collected, call_span);
        }
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_examples() {
        use crate::test_examples;

        test_examples(ToKdl {})
    }
}
