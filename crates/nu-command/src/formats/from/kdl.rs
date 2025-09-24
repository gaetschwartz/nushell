use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
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
                        "child" => Value::test_string("value"),
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
    let doc = string_input.parse::<KdlDocument>().map_err(|err| {
        let inner = err
            .diagnostics
            .iter()
            .map(|diag| {
                let label_span =
                    Span::new(diag.span.offset(), diag.span.offset() + diag.span.len());
                ShellError::OutsideSpannedLabeledError {
                    src: string_input.into(),
                    error: "Error while parsing KDL".into(),
                    msg: diag
                        .message
                        .clone()
                        .unwrap_or_else(|| "error while parsing KDL".into()),
                    span: label_span,
                }
            })
            .collect();
        ShellError::GenericError {
            error: "Error while parsing KDL".into(),
            msg: "error parsing KDL".into(),
            span: Some(span),
            help: None,
            inner,
        }
    })?;

    let value = convert_kdl_document_to_value(doc, string_input, span)?;

    Ok(value)
}

fn convert_kdl_document_to_value(
    document: KdlDocument,
    string_input: &str,
    span: Span,
) -> Result<Value, ShellError> {
    let mut record = indexmap::IndexMap::new();

    for node in document.nodes() {
        let node_name = node.name().value();
        let node_value = convert_kdl_node_to_value(node, string_input, span)?;

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

    Ok(Value::record(record.into_iter().collect(), span))
}

fn convert_kdl_node_to_value(
    node: &KdlNode,
    string_input: &str,
    span: Span,
) -> Result<Value, ShellError> {
    let has_entries = !node.entries().is_empty();
    let has_properties = node.entries().iter().any(|entry| entry.name().is_some());
    let has_arguments = node.entries().iter().any(|entry| entry.name().is_none());
    let has_children = node.children().is_some();

    if has_arguments && !has_properties && !has_children {
        if node.entries().len() == 1 {
            // Single argument, return as is
            convert_kdl_value_to_value(&node.entries()[0], string_input, span)
        } else {
            // Multiple arguments, return as list
            let vals = node
                .entries()
                .iter()
                .map(|entry| convert_kdl_value_to_value(entry, string_input, span))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::list(vals, span))
        }
    } else if !has_entries && !has_children {
        // Empty node
        Ok(Value::nothing(span))
    } else if has_properties && !has_children {
        let rec = node
            .entries()
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                if let Some(name) = entry.name() {
                    Ok::<_, ShellError>((
                        name.value().to_string(),
                        convert_kdl_value_to_value(entry, string_input, span)?,
                    ))
                } else {
                    Ok((
                        format!("#{i}"),
                        convert_kdl_value_to_value(entry, string_input, span)?,
                    ))
                }
            })
            .collect::<Result<Record, _>>()?;
        return Ok(Value::record(rec, span));
    } else if let Some(children) = node.children()
        && !has_properties
    {
        // Children only
        // if all the names are '-', make a list of values
        if children
            .nodes()
            .iter()
            .all(|child| child.name().value() == "-")
        {
            let mut vals = Vec::with_capacity(children.nodes().len());
            for child in children.nodes() {
                let child_value = convert_kdl_node_to_value(child, string_input, span)?;
                vals.push(child_value);
            }
            return Ok(Value::list(vals, span));
        } else {
            let record = children
                .nodes()
                .iter()
                .map(|child| {
                    let name = child.name().value().to_string();
                    let value = convert_kdl_node_to_value(child, string_input, span)?;
                    Ok((name, value))
                })
                .collect::<Result<Record, ShellError>>()?;
            return Ok(Value::record(record, span));
        }
    } else {
        // Both properties and children
        let mut record = indexmap::IndexMap::new();

        // Add properties
        for entry in node.entries() {
            if let Some(name) = entry.name() {
                record.insert(
                    name.value().to_string(),
                    convert_kdl_value_to_value(entry, string_input, span)?,
                );
            }
        }

        // Add arguments
        let args = node
            .entries()
            .iter()
            .filter(|entry| entry.name().is_none())
            .map(|entry| convert_kdl_value_to_value(entry, string_input, span))
            .collect::<Result<Vec<_>, _>>()?;

        if !args.is_empty() {
            record.insert("_args".to_string(), Value::list(args, span));
        }

        if let Some(children) = node.children() {
            record.insert(
                "children".to_string(),
                convert_kdl_document_to_value(children.clone(), string_input, span)?,
            );
        }

        Ok(Value::record(record.into_iter().collect(), span))
    }
}

fn convert_kdl_value_to_value(
    entry: &KdlEntry,
    string_input: &str,
    span: Span,
) -> Result<Value, ShellError> {
    match entry.value() {
        KdlValue::String(s) => Ok(Value::string(s, span)),
        KdlValue::Integer(n) => {
            // Convert i128 to i64, handling overflow
            i64::try_from(*n).map_or_else(
                |_| {
                    let what = if *n > 0 { "large" } else { "small" };
                    Err(ShellError::GenericError {
                        error: "Error while converting KDL integer".into(),
                        msg: "error converting KDL integer".into(),
                        span: Some(span),
                        help: None,
                        inner: vec![ShellError::OutsideSpannedLabeledError {
                            src: string_input.into(),
                            error: "Integer overflow".into(),
                            msg: format!("The integer {n} is too {what} to fit in i64. Only integers between {min:.3e} and {max:.3e} are supported.", min = i64::MIN, max = i64::MAX),
                            span: Span::new(
                                entry.span().offset(),
                                entry.span().offset() + entry.span().len(),
                            ),
                        }],
                    })
                },
                |n| Ok(Value::int(n, span)),
            )
        }
        KdlValue::Float(f) => Ok(Value::float(*f, span)),
        KdlValue::Bool(b) => Ok(Value::bool(*b, span)),
        KdlValue::Null => Ok(Value::nothing(span)),
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
