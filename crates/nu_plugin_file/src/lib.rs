use std::io::Read;

use nu_plugin::{EvaluatedCall, LabeledError, Plugin, PluginPipelineData};
use nu_protocol::{record, Category, PluginSignature, Type, Value};

pub struct FileCmd;

impl Plugin for FileCmd {
    fn signature(&self) -> Vec<PluginSignature> {
        vec![PluginSignature::build("file")
            .input_output_types(vec![(Type::String, Type::Record(vec![]))])
            .plugin_examples(vec![])
            .category(Category::Formats)
            .supports_pipelined_input(true)]
    }

    fn run(
        &mut self,
        _name: &str,
        call: &EvaluatedCall,
        input: PluginPipelineData,
    ) -> Result<Value, LabeledError> {
        let PluginPipelineData::ExternalStream(val) = input else {
            return Err(LabeledError {
                label: "ERROR from plugin".into(),
                msg: "expected external stream".into(),
                span: Some(call.head),
            });
        };
        let mut reader = val.open().map_err(|e| LabeledError {
            label: "ERROR from plugin".into(),
            msg: format!("failed to open pipe: {}", e),
            span: Some(call.head),
        })?;
        let mut vec = vec![];
        loop {
            let mut buf = [0; 4096];
            let n = reader.read(&mut buf).map_err(|e| LabeledError {
                label: "ERROR from plugin".into(),
                msg: format!("failed to read pipe: {}", e),
                span: Some(call.head),
            })?;
            if n == 0 {
                break;
            }
            vec.extend_from_slice(&buf[..n]);
        }
        let len = vec.len();
        let record = record!(
            "size" => Value::int(len as i64, call.head),
        );
        Ok(Value::record(record, call.head))
    }
}
