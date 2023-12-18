use crate::EvaluatedCall;

use super::{call_plugin, create_command, get_plugin_encoding};
use crate::protocol::{
    CallInfo, CallInput, PluginCall, PluginCustomValue, PluginData, PluginResponse,
};
use std::path::{Path, PathBuf};

use std::thread;

use log::trace;
use nu_pipes::unidirectional::{pipe, PipeWrite};
use nu_pipes::{trace_pipe, PipeFd, PipeReader, StreamSender};
use nu_protocol::engine::{Command, EngineState, Stack};
use nu_protocol::{ast::Call, PluginSignature, Signature};
use nu_protocol::{Example, PipelineData, RawStream, ShellError, Value};

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

    fn make_call_input(
        &self,
        mut input: PipelineData,
        call: &Call,
    ) -> Result<CallInputWithOptPipe, ShellError> {
        if self.signature.supports_pipelined_input {
            if let PipelineData::ExternalStream {
                stdout: ref mut stdout @ Some(_),
                ..
            } = input
            {
                let stream = stdout.take().unwrap();
                match pipe() {
                    Ok((pr, pw)) => {
                        return Ok(CallInputWithOptPipe(
                            CallInput::Pipe(pr.into_inheritable()?, stream.datatype),
                            Some((pw, stream)),
                        ));
                    }
                    Err(e) => {
                        trace!(
                            "Unable to create pipe for plugin {}: {}, falling back to regular data",
                            self.name,
                            e
                        );
                        // restore the stream
                        *stdout = Some(stream);
                    }
                }
            }
        }

        let value = input.into_value(call.head);
        let span = value.span();
        let input = match value {
            Value::CustomValue { val, .. } => {
                match val.as_any().downcast_ref::<PluginCustomValue>() {
                    Some(plugin) if plugin.filename == self.filename => {
                        CallInput::Data(PluginData {
                            data: plugin.data.clone(),
                            span,
                        })
                    }
                    _ => {
                        let custom_value_name = val.value_string();
                        return Err(ShellError::GenericError {
                            error: format!(
                                "Plugin {} can not handle the custom value {}",
                                self.name, custom_value_name
                            ),
                            msg: format!("custom value {custom_value_name}"),
                            span: Some(span),
                            help: None,
                            inner: vec![],
                        });
                    }
                }
            }
            Value::LazyRecord { val, .. } => CallInput::Value(val.collect()?),
            value => CallInput::Value(value),
        };
        Ok(CallInputWithOptPipe(input, None))
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

    fn supports_pipelined_input(&self) -> bool {
        self.signature.supports_pipelined_input
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
        let mut plugin_cmd = create_command(&self.filename, self.shell.as_deref());
        trace_pipe!(
            "Created command for plugin: `{} {}`",
            plugin_cmd.command.get_program().to_string_lossy(),
            plugin_cmd
                .command
                .get_args()
                .map(|a| a.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        );
        // We need the current environment variables for `python` based plugins
        // Or we'll likely have a problem when a plugin is implemented in a virtual Python environment.
        let current_envs = nu_engine::env::env_to_strings(engine_state, stack).unwrap_or_default();
        plugin_cmd.command.envs(current_envs);

        let (call_input, pipe, stream) = self.make_call_input(input, call)?.spread_pipe();

        let mut child = plugin_cmd.command.spawn().map_err(|err| {
            let decl = engine_state.get_decl(call.decl_id);
            ShellError::GenericError {
                error: format!("Unable to spawn plugin for {}", decl.name()),
                msg: format!("{err}"),
                span: Some(call.head),
                help: None,
                inner: Vec::new(),
            }
        })?;

        trace_pipe!("Spawned plugin, getting encoding");

        let encoding = {
            let mut stdout_reader = PipeReader::new(&plugin_cmd.stdout);
            get_plugin_encoding(&mut stdout_reader)?
        };

        trace_pipe!("Got encoding ({:?}), calling plugin", encoding);

        thread::scope(|s| {
            let join_handle = if let (Some(pipe), Some(stream)) = (pipe, stream) {
                pipe.send_stream_scoped(s, stream)?
            } else {
                None
            };

            let plugin_call = PluginCall::CallInfo(CallInfo {
                name: self.name.clone(),
                call: EvaluatedCall::try_from_call(call, engine_state, stack)?,
                input: call_input,
            });

            let response =
                call_plugin(&plugin_cmd, plugin_call, &encoding, call.head).map_err(|err| {
                    let decl = engine_state.get_decl(call.decl_id);
                    ShellError::GenericError {
                        error: format!("Unable to decode call for {}", decl.name()),
                        msg: err.to_string(),
                        span: Some(call.head),
                        help: None,
                        inner: Vec::new(),
                    }
                });

            trace_pipe!("Got response from plugin");

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
                    inner: Vec::new(),
                }),
                Err(err) => Err(err),
            };

            if let Some(join_handle) = join_handle {
                join_handle.join().map_err(|_| ShellError::GenericError {
                    error: format!("Unable to join thread for {}", &self.name),
                    msg: "Unable to join thread".into(),
                    span: Some(call.head),
                    help: None,
                    inner: Vec::new(),
                })?;
            }

            // We need to call .wait() on the child, or we'll risk summoning the zombie horde
            let _ = child.wait();

            pipeline_data
        })
    }

    fn is_plugin(&self) -> Option<(&Path, Option<&Path>)> {
        Some((&self.filename, self.shell.as_deref()))
    }
}

struct CallInputWithOptPipe(CallInput, Option<(PipeFd<PipeWrite>, RawStream)>);
impl CallInputWithOptPipe {
    fn spread_pipe(self) -> (CallInput, Option<PipeFd<PipeWrite>>, Option<RawStream>) {
        if let Some((pipe, stdout)) = self.1 {
            (self.0, Some(pipe), Some(stdout))
        } else {
            (self.0, None, None)
        }
    }
}
