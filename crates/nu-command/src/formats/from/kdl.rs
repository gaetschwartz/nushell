use kdl::{KdlDocument, KdlNode, KdlValue};
use nu_engine::command_prelude::*;

#[derive(Clone)]
pub struct FromKdl;

impl Command for FromKdl {
    fn name(&self) -> &str {
        "from kdl"
    }

    fn description(&self) -> &str {
        "Convert from kdl to structured data."
    }

    fn signature(&self) -> nu_protocol::Signature {
        Signature::build("from kdl")
            .input_output_types(vec![(Type::String, Type::Any)])
            .category(Category::Formats)
    }

    fn examples(&self) -> Vec<Example<'_>> {
        vec![
            Example {
                example: r#"'node "value"' | from kdl"#,
                description: "Converts kdl formatted string to table",
                result: Some(Value::test_record(record! {
                    "node" => Value::test_string("value"),
                })),
            },
            Example {
                example: r#"'node key="value"' | from kdl"#,
                description: "Converts kdl with properties to table",
                result: Some(Value::test_record(record! {
                    "node" => Value::test_record(record! {
                        "key" => Value::test_string("value"),
                    }),
                })),
            },
            Example {
                example: r#"'node {
    child "value"
}' | from kdl"#,
                description: "Converts kdl with children to table",
                result: Some(Value::test_record(record! {
                    "node" => Value::test_record(record! {
                        "children" => Value::test_record(record! {
                            "child" => Value::test_string("value"),
                        }),
                    }),
                })),
            },
        ]
    }

    fn run(
        &self,
        _engine_state: &EngineState,
        _stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let span = call.head;
        let (string_input, span, ..) = input.collect_string_strict(span)?;

        if string_input.is_empty() {
            return Ok(Value::nothing(span).into_pipeline_data());
        }

        convert_string_to_value(&string_input, span).map(|value| value.into_pipeline_data())
    }
}

fn convert_string_to_value(string_input: &str, span: Span) -> Result<Value, ShellError> {
    match string_input.parse::<KdlDocument>() {
        Ok(document) => Ok(convert_kdl_document_to_value(document, span)),
        Err(err) => Err(ShellError::CantConvert {
            to_type: format!("structured kdl data ({err})"),
            from_type: "string".into(),
            span,
            help: None,
        }),
    }
}

fn convert_kdl_document_to_value(document: KdlDocument, span: Span) -> Value {
    let mut record = indexmap::IndexMap::new();

    for node in document.nodes() {
        let node_name = node.name().value();
        let node_value = convert_kdl_node_to_value(node, span);

        // Handle multiple nodes with the same name by creating a list
        match record.get_mut(node_name) {
            Some(existing_value) => match existing_value {
                Value::List { vals, .. } => {
                    vals.push(node_value);
                }
                _ => {
                    let old_value = existing_value.clone();
                    *existing_value = Value::list(vec![old_value, node_value], span);
                }
            },
            None => {
                record.insert(node_name.to_string(), node_value);
            }
        }
    }

    Value::record(record.into_iter().collect(), span)
}

fn convert_kdl_node_to_value(node: &KdlNode, span: Span) -> Value {
    let has_properties = node.entries().iter().any(|entry| entry.name().is_some());
    let has_children = node.children().is_some();
    let has_arguments = node.entries().iter().any(|entry| entry.name().is_none());

    if !has_properties && !has_children {
        if has_arguments {
            // Just arguments
            let args: Vec<Value> = node
                .entries()
                .iter()
                .filter(|entry| entry.name().is_none())
                .map(|entry| convert_kdl_value_to_value(entry.value(), span))
                .collect();

            if args.len() == 1 {
                args.into_iter()
                    .next()
                    .expect("verified args has one element")
            } else {
                Value::list(args, span)
            }
        } else {
            // Empty node
            Value::nothing(span)
        }
    } else if has_properties && !has_children {
        // Properties only
        let mut record = indexmap::IndexMap::new();

        for entry in node.entries() {
            if let Some(name) = entry.name() {
                record.insert(
                    name.value().to_string(),
                    convert_kdl_value_to_value(entry.value(), span),
                );
            }
        }

        // Add arguments as well if they exist
        let args: Vec<Value> = node
            .entries()
            .iter()
            .filter(|entry| entry.name().is_none())
            .map(|entry| convert_kdl_value_to_value(entry.value(), span))
            .collect();

        if !args.is_empty() {
            record.insert("_args".to_string(), Value::list(args, span));
        }

        Value::record(record.into_iter().collect(), span)
    } else if !has_properties && has_children {
        // Children only
        let mut record = indexmap::IndexMap::new();

        // Add arguments
        let args: Vec<Value> = node
            .entries()
            .iter()
            .map(|entry| convert_kdl_value_to_value(entry.value(), span))
            .collect();

        if !args.is_empty() {
            record.insert("_args".to_string(), Value::list(args, span));
        }

        if let Some(children) = node.children() {
            record.insert(
                "children".to_string(),
                convert_kdl_document_to_value(children.clone(), span),
            );
        }

        Value::record(record.into_iter().collect(), span)
    } else {
        // Both properties and children
        let mut record = indexmap::IndexMap::new();

        // Add properties
        for entry in node.entries() {
            if let Some(name) = entry.name() {
                record.insert(
                    name.value().to_string(),
                    convert_kdl_value_to_value(entry.value(), span),
                );
            }
        }

        // Add arguments
        let args: Vec<Value> = node
            .entries()
            .iter()
            .filter(|entry| entry.name().is_none())
            .map(|entry| convert_kdl_value_to_value(entry.value(), span))
            .collect();

        if !args.is_empty() {
            record.insert("_args".to_string(), Value::list(args, span));
        }

        if let Some(children) = node.children() {
            record.insert(
                "children".to_string(),
                convert_kdl_document_to_value(children.clone(), span),
            );
        }

        Value::record(record.into_iter().collect(), span)
    }
}

fn convert_kdl_value_to_value(value: &KdlValue, span: Span) -> Value {
    match value {
        KdlValue::String(s) => Value::string(s, span),
        KdlValue::Integer(n) => {
            // Convert i128 to i64, handling overflow
            if *n >= i64::MIN as i128 && *n <= i64::MAX as i128 {
                Value::int(*n as i64, span)
            } else {
                Value::error(
                    ShellError::CantConvert {
                        to_type: "i64 sized integer".into(),
                        from_type: "value larger than i64".into(),
                        span,
                        help: None,
                    },
                    span,
                )
            }
        }
        KdlValue::Float(f) => Value::float(*f, span),
        KdlValue::Bool(b) => Value::bool(*b, span),
        KdlValue::Null => Value::nothing(span),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_examples() {
        use crate::test_examples;

        test_examples(FromKdl {})
    }
}
