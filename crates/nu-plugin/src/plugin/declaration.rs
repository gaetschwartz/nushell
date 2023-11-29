use crate::{EvaluatedCall, OsPipe};

use super::{call_plugin, create_command, get_plugin_encoding};
use crate::protocol::{
    CallInfo, CallInput, PluginCall, PluginCustomValue, PluginData, PluginResponse,
};
use std::path::{Path, PathBuf};

use log::trace;
use nu_protocol::engine::{Command, EngineState, Stack};
use nu_protocol::{ast::Call, PluginSignature, Signature};
use nu_protocol::{Example, PipelineData, ShellError, Value};

#[doc(hidden)] // Note: not for plugin authors / only used in nu-parser
#[derive(Clone)]
pub struct PluginDeclaration {
    name: String,
    signature: PluginSignature,
    filename: PathBuf,
    shell: Option<PathBuf>,
}

impl PluginDeclaration {
    pub fn new(filename: PathBuf, signature: PluginSignature, shell: Option<PathBuf>) -> Self {
        Self {
            name: signature.sig.name.clone(),
            signature,
            filename,
            shell,
        }
    }

    fn make_call_input(&self, input: PipelineData, call: &Call) -> Result<CallInput, ShellError> {
        if let PipelineData::ExternalStream {
            stdout: Some(_), ..
        } = input
        {
            match OsPipe::create(call.head) {
                Ok(os_pipe) => return Ok(CallInput::Pipe(os_pipe, Some(input))),
                Err(e) => {
                    trace!("Unable to create pipe for plugin {}: {}", self.name, e);
                }
            }
        }
        let input = input.into_value(call.head);
        let span = input.span();
        let input = match input {
            Value::CustomValue { val, .. } => {
                match val.as_any().downcast_ref::<PluginCustomValue>() {
                    Some(plugin_data) if plugin_data.filename == self.filename => {
                        CallInput::Data(PluginData {
                            data: plugin_data.data.clone(),
                            span,
                        })
                    }
                    _ => {
                        let custom_value_name = val.value_string();
                        return Err(ShellError::GenericError(
                            format!(
                                "Plugin {} can not handle the custom value {}",
                                self.name, custom_value_name
                            ),
                            format!("custom value {custom_value_name}"),
                            Some(span),
                            None,
                            Vec::new(),
                        ));
                    }
                }
            }
            Value::LazyRecord { val, .. } => CallInput::Value(val.collect()?),
            value => CallInput::Value(value),
        };
        Ok(input)
    }
}

impl Command for PluginDeclaration {
    fn name(&self) -> &str {
        &self.name
    }

    fn signature(&self) -> Signature {
        self.signature.sig.clone()
    }

    fn usage(&self) -> &str {
        self.signature.sig.usage.as_str()
    }

    fn extra_usage(&self) -> &str {
        self.signature.sig.extra_usage.as_str()
    }

    fn search_terms(&self) -> Vec<&str> {
        self.signature
            .sig
            .search_terms
            .iter()
            .map(|term| term.as_str())
            .collect()
    }

    fn examples(&self) -> Vec<Example> {
        let mut res = vec![];
        for e in self.signature.examples.iter() {
            res.push(Example {
                example: &e.example,
                description: &e.description,
                result: e.result.clone(),
            })
        }
        res
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        // Call the command with self path
        // Decode information from plugin
        // Create PipelineData
        let source_file = Path::new(&self.filename);
        let mut plugin_cmd = create_command(source_file, self.shell.as_deref());
        // We need the current environment variables for `python` based plugins
        // Or we'll likely have a problem when a plugin is implemented in a virtual Python environment.
        let current_envs = nu_engine::env::env_to_strings(engine_state, stack).unwrap_or_default();
        plugin_cmd.envs(current_envs);

        let mut call_input = self.make_call_input(input, call)?;
        let join_handle = OsPipe::start_pipe(&mut call_input)?;

        let mut child = plugin_cmd.spawn().map_err(|err| {
            let decl = engine_state.get_decl(call.decl_id);
            ShellError::GenericError {
                error: format!("Unable to spawn plugin for {}", decl.name()),
                msg: format!("{err}"),
                span: Some(call.head),
                help: None,
                inner: vec![],
            }
        })?;

        let plugin_call = PluginCall::CallInfo(CallInfo {
            name: self.name.clone(),
            call: EvaluatedCall::try_from_call(call, engine_state, stack)?,
            input: call_input.clone(),
        });

        let encoding = {
            let stdout_reader = match &mut child.stdout {
                Some(out) => out,
                None => {
                    return Err(ShellError::PluginFailedToLoad {
                        msg: "Plugin missing stdout reader".into(),
                    })
                }
            };
            get_plugin_encoding(stdout_reader)?
        };

        let response = call_plugin(&mut child, plugin_call, &encoding, call.head).map_err(|err| {
            let decl = engine_state.get_decl(call.decl_id);
            ShellError::GenericError {
                error: format!("Unable to decode call for {}", decl.name()),
                msg: err.to_string(),
                span: Some(call.head),
                help: None,
                inner: vec![],
            }
        });

        let pipeline_data = match response {
            Ok(PluginResponse::Value(value)) => {
                Ok(PipelineData::Value(value.as_ref().clone(), None))
            }
            Ok(PluginResponse::PluginData(name, plugin_data)) => Ok(PipelineData::Value(
                Value::custom_value(
                    Box::new(PluginCustomValue {
                        name,
                        data: plugin_data.data,
                        filename: self.filename.clone(),
                        shell: self.shell.clone(),
                        source: engine_state.get_decl(call.decl_id).name().to_owned(),
                    }),
                    plugin_data.span,
                ),
                None,
            )),
            Ok(PluginResponse::Error(err)) => Err(err.into()),
            Ok(PluginResponse::Signature(..)) => Err(ShellError::GenericError {
                error: "Plugin missing value".into(),
                msg: "Received a signature from plugin instead of value".into(),
                span: Some(call.head),
                help: None,
                inner: vec![],
            }),
            Err(err) => Err(err),
        };

        let time = std::time::Instant::now();
        if let Some(join_handle) = join_handle {
            _ = join_handle.join();
        }
        println!(
            "Plugin {} took {} ms",
            self.name,
            time.elapsed().as_micros() as f64 / 1000.0
        );

        // We need to call .wait() on the child, or we'll risk summoning the zombie horde
        let _ = child.wait();

        pipeline_data
    }

    fn is_plugin(&self) -> Option<(&Path, Option<&Path>)> {
        Some((&self.filename, self.shell.as_deref()))
    }
}
