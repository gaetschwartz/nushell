use base64::{Engine, engine::general_purpose};
use kdl::{KdlDocument, KdlEntry, KdlIdentifier, KdlNode, KdlValue};
use nu_engine::command_prelude::*;
use nu_protocol::{PipelineMetadata, ast::PathMember};

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
        "Converts table data into KDL text."
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

        let kdl_document = value_to_kdl_document(engine_state, &value, span)?;

        let kdl_result = if raw {
            kdl_document.to_string().trim().to_string()
        } else {
            kdl_document.to_string()
        };

        let res = Value::string(kdl_result, span);
        let metadata = PipelineMetadata {
            data_source: nu_protocol::DataSource::None,
            content_type: Some("application/kdl".to_string()),
        };
        Ok(PipelineData::value(res, Some(metadata)))
    }

    fn examples(&self) -> Vec<Example<'_>> {
        vec![
            Example {
                description: "Outputs a KDL string representing the contents of this table",
                example: r#"{name: "node", value: "data"} | to kdl"#,
                result: Some(Value::test_string("name node\nvalue data\n")),
            },
            Example {
                description: "Outputs a KDL string representing a nested structure",
                example: r#"{parent: {child: "value"}} | to kdl"#,
                result: Some(Value::test_string("parent child=value\n")),
            },
            Example {
                description: "Outputs an unformatted KDL string",
                example: r#"{a: 1, b: 2} | to kdl --raw"#,
                result: Some(Value::test_string("a 1\nb 2")),
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

    match v {
        Value::Record { val, .. } => {
            for (key, value) in &**val {
                let node = value_to_kdl_node(engine_state, key, value, call_span)?;
                document.nodes_mut().push(node);
            }
        }
        Value::List { vals, .. } => {
            for (index, value) in vals.iter().enumerate() {
                let node_name = format!("item_{index}");
                let node = value_to_kdl_node(engine_state, &node_name, value, call_span)?;
                document.nodes_mut().push(node);
            }
        }
        _ => {
            // For scalar values, create a single node named "value"
            let node = value_to_kdl_node(engine_state, "value", v, call_span)?;
            document.nodes_mut().push(node);
        }
    }

    Ok(document)
}

fn value_to_kdl_node(
    engine_state: &EngineState,
    name: &str,
    value: &Value,
    call_span: Span,
) -> Result<KdlNode, ShellError> {
    let node_name = KdlIdentifier::from(name);
    let mut node = KdlNode::new(node_name);

    match value {
        Value::Record { val, .. } => {
            for (key, val) in &**val {
                if key == "children" {
                    // Handle children specially
                    match val {
                        Value::Record {
                            val: children_record,
                            ..
                        } => {
                            let mut children_doc = KdlDocument::new();
                            for (child_key, child_val) in &**children_record {
                                let child_node = value_to_kdl_node(
                                    engine_state,
                                    child_key,
                                    child_val,
                                    call_span,
                                )?;
                                children_doc.nodes_mut().push(child_node);
                            }
                            node.set_children(children_doc);
                        }
                        _ => {
                            return Err(ShellError::CantConvert {
                                to_type: "KDL".into(),
                                from_type: "children must be a record".into(),
                                span: call_span,
                                help: None,
                            });
                        }
                    }
                } else if key == "_args" {
                    // Handle arguments specially
                    match val {
                        Value::List { vals, .. } => {
                            for arg_val in vals {
                                let kdl_val = value_to_kdl_value(engine_state, arg_val, call_span)?;
                                let entry = KdlEntry::new(kdl_val);
                                node.entries_mut().push(entry);
                            }
                        }
                        _ => {
                            let kdl_val = value_to_kdl_value(engine_state, val, call_span)?;
                            let entry = KdlEntry::new(kdl_val);
                            node.entries_mut().push(entry);
                        }
                    }
                } else {
                    // Regular property
                    let kdl_val = value_to_kdl_value(engine_state, val, call_span)?;
                    let prop_name = KdlIdentifier::from(key.as_str());
                    let entry = KdlEntry::new_prop(prop_name, kdl_val);
                    node.entries_mut().push(entry);
                }
            }
        }
        Value::List { vals, .. } => {
            // List becomes arguments
            for val in vals {
                let kdl_val = value_to_kdl_value(engine_state, val, call_span)?;
                let entry = KdlEntry::new(kdl_val);
                node.entries_mut().push(entry);
            }
        }
        _ => {
            // Scalar value becomes an argument
            let kdl_val = value_to_kdl_value(engine_state, value, call_span)?;
            let entry = KdlEntry::new(kdl_val);
            node.entries_mut().push(entry);
        }
    }

    Ok(node)
}

#[allow(clippy::used_underscore_binding)]
fn value_to_kdl_value(
    _engine_state: &EngineState,
    v: &Value,
    call_span: Span,
) -> Result<KdlValue, ShellError> {
    let span = v.span();
    Ok(match v {
        Value::Bool { val, .. } => KdlValue::Bool(*val),
        Value::Int { val, .. } => KdlValue::Integer(*val as i128),
        Value::Float { val, .. } => KdlValue::Float(*val),
        Value::String { val, .. } => KdlValue::String(val.clone()),
        Value::Glob { val, .. } => KdlValue::String(val.to_string()),
        Value::Nothing { .. } => KdlValue::Null,
        Value::Filesize { val, .. } => KdlValue::Integer(val.get() as i128),
        Value::Duration { val, .. } => KdlValue::Integer(*val as i128),
        Value::Date { val, .. } => KdlValue::String(val.to_string()),
        Value::Binary { val, .. } => {
            // Convert binary to base64 string
            KdlValue::String(general_purpose::STANDARD.encode(val))
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
            KdlValue::String(path_str)
        }
        Value::Range { .. } => {
            return Err(ShellError::UnsupportedInput {
                msg: "ranges are not supported in KDL".into(),
                input: "value originates from here".into(),
                msg_span: call_span,
                input_span: span,
            });
        }
        Value::Closure { .. } => {
            return Err(ShellError::UnsupportedInput {
                msg: "closures are not supported in KDL".into(),
                input: "value originates from here".into(),
                msg_span: call_span,
                input_span: span,
            });
        }
        Value::Record { .. } | Value::List { .. } => {
            return Err(ShellError::UnsupportedInput {
                msg: "nested structures should be handled at node level".into(),
                input: "value originates from here".into(),
                msg_span: call_span,
                input_span: span,
            });
        }
        Value::Error { error, .. } => return Err(*error.clone()),
        Value::Custom { val, .. } => {
            let collected = val.to_base_value(span)?;
            return value_to_kdl_value(_engine_state, &collected, call_span);
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
