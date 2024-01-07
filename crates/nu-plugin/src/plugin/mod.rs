mod declaration;
pub use declaration::PluginDeclaration;
use nu_engine::documentation::get_flags_section;
use nu_pipes::unidirectional::{PipeRead, PipeWrite};
use nu_pipes::{trace_pipe, PipeFd, PipeReader, PipeWriter};
use nu_protocol::plugin_protocol::{self};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsStr;

use crate::protocol::{
    CallInput, LabeledError, PluginCall, PluginData, PluginPipelineData, PluginResponse,
};
use crate::EncodingType;
use std::env;
use std::fmt::Write;
use std::io::{Error, ErrorKind, Write as WriteTrait};
use std::path::Path;
use std::process::{exit, Command as CommandSys, Stdio};

use nu_protocol::{CustomValue, PluginSignature, ShellError, Span, Value};

use super::EvaluatedCall;

/// Encoding scheme that defines a plugin's communication protocol with Nu
pub trait PluginCodec: Clone {
    /// The name of the encoder (e.g., `json`)
    fn name(&self) -> &str;

    /// Serialize a `PluginCall` in the `PluginEncoder`s format
    fn encode_call(
        &self,
        plugin_call: &PluginCall,
        writer: &mut impl std::io::Write,
    ) -> Result<(), ShellError>;

    /// Deserialize a `PluginCall` from the `PluginEncoder`s format
    fn decode_call(&self, reader: &mut impl std::io::BufRead) -> Result<PluginCall, ShellError>;

    /// Serialize a `PluginResponse` from the plugin in this `PluginEncoder`'s preferred
    /// format
    fn encode_response(
        &self,
        plugin_response: &PluginResponse,
        writer: &mut impl std::io::Write,
    ) -> Result<(), ShellError>;

    /// Deserialize a `PluginResponse` from the plugin from this `PluginEncoder`'s
    /// preferred format
    fn decode_response(
        &self,
        reader: &mut impl std::io::BufRead,
    ) -> Result<PluginResponse, ShellError>;
}

pub(crate) struct PluginCommand {
    pub(crate) command: CommandSys,
    pub(crate) stdin: PipeFd<PipeWrite>,
    pub(crate) stdout: PipeFd<PipeRead>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PluginPipes {
    pub(crate) stdin: PipeFd<PipeRead>,
    pub(crate) stdout: PipeFd<PipeWrite>,
}

pub(crate) fn create_command(
    path: &Path,
    shell: Option<&Path>,
    protocol_version: plugin_protocol::Version,
) -> PluginCommand {
    let (stdin_pipe_read, stdin_pipe_write) = nu_pipes::unidirectional::pipe().unwrap();
    let (stdout_pipe_read, stdout_pipe_write) = nu_pipes::unidirectional::pipe().unwrap();
    let plugin_pipes = PluginPipes {
        stdin: stdin_pipe_read.into_inheritable().unwrap(),
        stdout: stdout_pipe_write.into_inheritable().unwrap(),
    };

    let pipes_ser = serde_json::to_string(&plugin_pipes).unwrap();

    let mut process = match (path.extension(), shell) {
        (_, Some(shell)) => {
            let mut process = std::process::Command::new(shell);
            process.arg(path);
            process.arg(&pipes_ser);

            process
        }
        (Some(extension), None) => {
            match extension.to_str() {
                // Some("cmd") | Some("bat") => ("cmd".as_ref(), vec!["/c".as_ref(), path, pipes_ser]),
                Some("cmd") | Some("bat") => CommandBuilder::new("cmd")
                    .arg("/c")
                    .arg(path)
                    .arg(&pipes_ser)
                    .build(),
                // Some("sh") => ("sh".as_ref(), vec!["-c".as_ref(), path, pipes_ser]),
                Some("sh") => CommandBuilder::new("sh")
                    .arg("-c")
                    .arg(path)
                    .arg(&pipes_ser)
                    .build(),
                // Some("py") => ("python".as_ref(), vec![path, pipes_ser]),
                Some("py") => CommandBuilder::new("python")
                    .arg(path)
                    .arg(&pipes_ser)
                    .build(),
                // _ => (path, vec![pipes_ser]),
                _ => CommandBuilder::new(path).arg(&pipes_ser).build(),
            }
        }
        (None, None) => CommandBuilder::new(path).arg(&pipes_ser).build(),
    };

    if protocol_version.supports(plugin_protocol::Capability::Pipes) {
        process
            .stdin(plugin_pipes.stdin)
            .stdout(plugin_pipes.stdout);
    }

    PluginCommand {
        command: process,
        stdin: stdin_pipe_write,
        stdout: stdout_pipe_read,
    }
}

pub(crate) fn call_plugin(
    plugin_cmd: PluginCommand,
    plugin_call: PluginCall,
    encoding: &EncodingType,
    _span: Span,
) -> Result<PluginResponse, ShellError> {
    // If the child process fills its stdout buffer, it may end up waiting until the parent
    // reads the stdout, and not be able to read stdin in the meantime, causing a deadlock.
    // Writing from another thread ensures that stdout is being read at the same time, avoiding the problem.
    std::thread::scope(|s| {
        let encoding_clone = encoding.clone();
        let handle = s.spawn(move || {
            let mut stdin_writer = plugin_cmd.stdin.into_writer();
            encoding_clone
                .encode_call(&plugin_call, &mut stdin_writer)
                .and_then(|_| {
                    stdin_writer
                        .close()
                        .map_err(|err| ShellError::PluginFailedToLoad {
                            msg: format!("Failed to close stdin: {}", err.error()),
                        })
                })
        });

        // Deserialize response from plugin to extract the resulting value

        let mut stdout_reader = plugin_cmd.stdout.into_reader();

        let res = encoding.decode_response(&mut stdout_reader)?;

        handle.join().unwrap()?;

        stdout_reader
            .close()
            .map_err(|err| ShellError::PluginFailedToLoad {
                msg: format!("Failed to close stdout: {}", err.error()),
            })?;

        Ok(res)
    })
}

#[doc(hidden)] // Note: not for plugin authors / only used in nu-parser
/// In this function we assume the plugin is of version 1
pub fn get_signature(
    path: &Path,
    shell: Option<&Path>,
    current_envs: &HashMap<String, String>,
) -> Result<Vec<PluginSignature>, ShellError> {
    let mut plugin_cmd = create_command(path, shell, plugin_protocol::Version::V1);
    let program_name = plugin_cmd
        .command
        .get_program()
        .to_os_string()
        .into_string();

    plugin_cmd.command.envs(current_envs);
    trace_pipe!(
        "Spawning plugin using `{} {:?}`",
        plugin_cmd.command.get_program().to_string_lossy(),
        plugin_cmd
            .command
            .get_args()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let mut child = plugin_cmd.command.spawn().map_err(|err| {
        let error_msg = match err.kind() {
            ErrorKind::NotFound => match program_name {
                Ok(prog_name) => {
                    format!("Can't find {prog_name}, please make sure that {prog_name} is in PATH.")
                }
                _ => {
                    format!("Error spawning child process: {err}")
                }
            },
            _ => {
                format!("Error spawning child process: {err}")
            }
        };

        ShellError::PluginFailedToLoad { msg: error_msg }
    })?;

    let mut stdout_reader = plugin_cmd.stdout.into_reader();
    let mut stdin_writer = plugin_cmd.stdin.into_writer();
    trace_pipe!("Getting encoding from plugin...");
    let encoding = get_plugin_encoding(&mut stdout_reader)?;
    trace_pipe!("Got encoding ({:?}), calling plugin", encoding);

    // Create message to plugin to indicate that signature is required and
    // send call to plugin asking for signature
    let encoding_clone = encoding.clone();
    // If the child process fills its stdout buffer, it may end up waiting until the parent
    // reads the stdout, and not be able to read stdin in the meantime, causing a deadlock.
    // Writing from another thread ensures that stdout is being read at the same time, avoiding the problem.
    let response = std::thread::scope(|s| {
        let res = s.spawn(move || {
            encoding_clone
                .encode_call(&PluginCall::Signature, &mut stdin_writer)
                .and_then(|_| {
                    stdin_writer
                        .close()
                        .map_err(|err| ShellError::PluginFailedToLoad {
                            msg: format!("Failed to close stdin: {}", err.error()),
                        })
                })
        });
        // deserialize response from plugin to extract the signature
        trace_pipe!("Reading plugin response...");

        let decode_response = encoding.decode_response(&mut stdout_reader);

        trace_pipe!("Got plugin response: {:?}", decode_response);

        res.join().unwrap()?;

        decode_response
    })?;

    let signatures = match response {
        PluginResponse::Signature(sign) => Ok(sign),
        PluginResponse::Error(err) => Err(err.into()),
        _ => Err(ShellError::PluginFailedToLoad {
            msg: "Plugin missing signature".into(),
        }),
    }?;

    match child.wait() {
        Ok(_) => Ok(signatures),
        Err(err) => Err(ShellError::PluginFailedToLoad {
            msg: format!("{err}"),
        }),
    }
}

/// The basic API for a Nushell plugin
///
/// This is the trait that Nushell plugins must implement. The methods defined on
/// `Plugin` are invoked by [serve_plugin] during plugin registration and execution.
///
/// # Examples
/// Basic usage:
/// ```
/// # use nu_plugin::*;
/// # use nu_protocol::{PluginSignature, Type, Value};
/// struct HelloPlugin;
///
/// impl Plugin for HelloPlugin {
///     fn signature(&self) -> Vec<PluginSignature> {
///         let sig = PluginSignature::build("hello")
///             .input_output_type(Type::Nothing, Type::String);
///
///         vec![sig]
///     }
///
///     fn run(
///         &mut self,
///         name: &str,
///         call: &EvaluatedCall,
///         input: PluginPipelineData,
///     ) -> Result<Value, LabeledError> {
///         Ok(Value::string("Hello, World!".to_owned(), call.head))
///     }
/// }
/// ```
pub trait Plugin {
    /// The signature of the plugin
    ///
    /// This method returns the [PluginSignature]s that describe the capabilities
    /// of this plugin. Since a single plugin executable can support multiple invocation
    /// patterns we return a `Vec` of signatures.
    fn signature(&self) -> Vec<PluginSignature>;

    /// Perform the actual behavior of the plugin
    ///
    /// The behavior of the plugin is defined by the implementation of this method.
    /// When Nushell invoked the plugin [serve_plugin] will call this method and
    /// print the serialized returned value or error to stdout, which Nushell will
    /// interpret.
    ///
    /// The `name` is only relevant for plugins that implement multiple commands as the
    /// invoked command will be passed in via this argument. The `call` contains
    /// metadata describing how the plugin was invoked and `input` contains the structured
    /// data passed to the command implemented by this [Plugin].
    fn run(
        &mut self,
        name: &str,
        call: &EvaluatedCall,
        input: PluginPipelineData,
    ) -> Result<Value, LabeledError>;
}

#[derive(Debug, Default)]
struct PluginCli {
    pipes: Option<PluginPipes>,
    help: bool,
}

impl PluginCli {
    fn parse_args() -> Result<Self, Error> {
        let mut cli = Self::default();
        let mut pos = 0;
        for arg in env::args().skip(1) {
            match arg.as_str() {
                "-h" | "--help" => {
                    cli.help = true;
                    break;
                }
                p if !p.starts_with('-') => {
                    if pos == 0 {
                        let pipes = serde_json::from_str(p)
                            .map_err(|err| Error::new(ErrorKind::Other, err))?;
                        cli.pipes = Some(pipes);
                    }
                    pos += 1;
                }
                _ => {}
            }
        }
        Ok(cli)
    }
}

/// Function used to implement the communication protocol between
/// nushell and an external plugin.
///
/// When creating a new plugin this function is typically used as the main entry
/// point for the plugin, e.g.
///
/// ```
/// # use nu_plugin::*;
/// # use nu_protocol::{PluginSignature, Value, PluginPipelineData};
/// # struct MyPlugin;
/// # impl MyPlugin { fn new() -> Self { Self }}
/// # impl Plugin for MyPlugin {
/// #     fn signature(&self) -> Vec<PluginSignature> {todo!();}
/// #     fn run(&mut self, name: &str, call: &EvaluatedCall, input: PluginPipelineData)
/// #         -> Result<Value, LabeledError> {todo!();}
/// # }
/// fn main() {
///    serve_plugin(&mut MyPlugin::new(), MsgPackSerializer)
/// }
/// ```
///
/// The object that is expected to be received by nushell is the `PluginResponse` struct.
/// The `serve_plugin` function should ensure that it is encoded correctly and sent
/// to StdOut for nushell to decode and and present its result.
pub fn serve_plugin(plugin: &mut impl Plugin, codec: impl PluginCodec) {
    let cli = match PluginCli::parse_args() {
        Ok(cli) => cli,
        Err(err) => {
            eprintln!("Error parsing plugin arguments: {}", err);
            exit(1);
        }
    };

    if cli.help {
        print_help(plugin, codec);
        exit(0);
    }

    let plugin_pipes = cli.pipes.unwrap_or(PluginPipes {
        stdin: PipeFd::stdin(),
        stdout: PipeFd::stdout(),
    });

    let mut stdout_writer = PipeWriter::new(&plugin_pipes.stdout);
    let mut stdin_reader = PipeReader::new(&plugin_pipes.stdin);

    trace_pipe!("Sending our encoding to nushell...");

    // tell nushell encoding.
    //
    //                         1 byte
    // encoding format: |  content-length  | content    |
    {
        let encoding = codec.name();
        let length = encoding.len();
        let mut encoding_content: Vec<u8> = Vec::with_capacity(length + 1);
        encoding_content.insert(0, length as u8);
        encoding_content.extend_from_slice(encoding.as_bytes());
        stdout_writer
            .write_all(&encoding_content)
            .expect("Failed to tell nushell my encoding");
        stdout_writer
            .flush()
            .expect("Failed to tell nushell my encoding when flushing stdout");
    }

    trace_pipe!("Reading plugin call from nushell...");

    let plugin_call = codec.decode_call(&mut stdin_reader);

    trace_pipe!("Read plugin call from nushell");

    match plugin_call {
        Err(err) => {
            let response = PluginResponse::Error(err.into());
            codec
                .encode_response(&response, &mut stdout_writer)
                .expect("Error encoding response");
        }
        Ok(plugin_call) => {
            match plugin_call {
                // Sending the signature back to nushell to create the declaration definition
                PluginCall::Signature => {
                    let response = PluginResponse::Signature(plugin.signature());
                    codec
                        .encode_response(&response, &mut stdout_writer)
                        .expect("Error encoding response");
                }
                PluginCall::CallInfo(call_info) => {
                    let input = match call_info.input {
                        CallInput::Value(value) => Ok(PluginPipelineData::Value(value)),
                        CallInput::Data(plugin_data) => {
                            bincode::deserialize::<Box<dyn CustomValue>>(&plugin_data.data)
                                .map(|custom_value| {
                                    Value::custom_value(custom_value, plugin_data.span)
                                })
                                .map_err(|err| ShellError::PluginFailedToDecode {
                                    msg: err.to_string(),
                                })
                                .map(PluginPipelineData::Value)
                        }
                        CallInput::Pipe(pipe, dt) => Ok(PluginPipelineData::ExternalStream(
                            pipe.into_reader(),
                            dt,
                            call_info.call.head.into(),
                        )),
                    };

                    let value = match input {
                        Ok(input) => plugin.run(&call_info.name, &call_info.call, input),
                        Err(err) => Err(err.into()),
                    };

                    let response = match value {
                        Ok(value) => {
                            let span = value.span();
                            match value {
                                Value::CustomValue { val, .. } => match bincode::serialize(&val) {
                                    Ok(data) => {
                                        let name = val.value_string();
                                        PluginResponse::PluginData(name, PluginData { data, span })
                                    }
                                    Err(err) => PluginResponse::Error(
                                        ShellError::PluginFailedToEncode {
                                            msg: err.to_string(),
                                        }
                                        .into(),
                                    ),
                                },
                                value => PluginResponse::Value(Box::new(value)),
                            }
                        }
                        Err(err) => PluginResponse::Error(err),
                    };
                    codec
                        .encode_response(&response, &mut stdout_writer)
                        .expect("Error encoding response");
                }
                PluginCall::CollapseCustomValue(plugin_data) => {
                    let response = bincode::deserialize::<Box<dyn CustomValue>>(&plugin_data.data)
                        .map_err(|err| ShellError::PluginFailedToDecode {
                            msg: err.to_string(),
                        })
                        .and_then(|val| val.to_base_value(plugin_data.span))
                        .map(Box::new)
                        .map_err(LabeledError::from)
                        .map_or_else(PluginResponse::Error, PluginResponse::Value);

                    codec
                        .encode_response(&response, &mut stdout_writer)
                        .expect("Error encoding response");
                }
            }
        }
    }
}

fn print_help(plugin: &mut impl Plugin, encoder: impl PluginCodec) {
    println!("Nushell Plugin");
    println!("Encoder: {}", encoder.name());

    let mut help = String::new();

    plugin.signature().iter().for_each(|signature| {
        let res = write!(help, "\nCommand: {}", signature.sig.name)
            .and_then(|_| writeln!(help, "\nUsage:\n > {}", signature.sig.usage))
            .and_then(|_| {
                if !signature.sig.extra_usage.is_empty() {
                    writeln!(help, "\nExtra usage:\n > {}", signature.sig.extra_usage)
                } else {
                    Ok(())
                }
            })
            .and_then(|_| {
                let flags = get_flags_section(None, &signature.sig, |v| format!("{:#?}", v));
                write!(help, "{flags}")
            })
            .and_then(|_| writeln!(help, "\nParameters:"))
            .and_then(|_| {
                signature
                    .sig
                    .required_positional
                    .iter()
                    .try_for_each(|positional| {
                        writeln!(
                            help,
                            "  {} <{}>: {}",
                            positional.name, positional.shape, positional.desc
                        )
                    })
            })
            .and_then(|_| {
                signature
                    .sig
                    .optional_positional
                    .iter()
                    .try_for_each(|positional| {
                        writeln!(
                            help,
                            "  (optional) {} <{}>: {}",
                            positional.name, positional.shape, positional.desc
                        )
                    })
            })
            .and_then(|_| {
                if let Some(rest_positional) = &signature.sig.rest_positional {
                    writeln!(
                        help,
                        "  ...{} <{}>: {}",
                        rest_positional.name, rest_positional.shape, rest_positional.desc
                    )
                } else {
                    Ok(())
                }
            })
            .and_then(|_| writeln!(help, "======================"));

        if res.is_err() {
            println!("{res:?}")
        }
    });

    println!("{help}")
}

pub fn get_plugin_encoding(
    child_stdout: &mut impl std::io::BufRead,
) -> Result<EncodingType, ShellError> {
    let mut length_buf = [0u8; 1];
    child_stdout
        .read_exact(&mut length_buf)
        .map_err(|e| ShellError::PluginFailedToLoad {
            msg: format!("unable to get encoding from plugin: {e}"),
        })?;

    let mut buf = vec![0u8; length_buf[0] as usize];
    child_stdout
        .read_exact(&mut buf)
        .map_err(|e| ShellError::PluginFailedToLoad {
            msg: format!("unable to get encoding from plugin: {e}"),
        })?;

    EncodingType::try_from_bytes(&buf).ok_or_else(|| {
        let encoding_for_debug = String::from_utf8_lossy(&buf);
        ShellError::PluginFailedToLoad {
            msg: format!("get unsupported plugin encoding: {encoding_for_debug}"),
        }
    })
}

pub struct CommandBuilder<'a, C: AsRef<OsStr>> {
    cmd: C,
    args: Vec<&'a OsStr>,
    envs: HashMap<String, String>,
    stdin: Stdio,
    stdout: Stdio,
    stderr: Stdio,
    current_dir: Option<&'a Path>,
}

#[allow(dead_code)]
impl<'a, C: AsRef<OsStr>> CommandBuilder<'a, C> {
    pub fn new(cmd: C) -> Self {
        Self {
            cmd,
            args: vec![],
            envs: HashMap::new(),
            stdin: Stdio::inherit(),
            stdout: Stdio::inherit(),
            stderr: Stdio::inherit(),
            current_dir: None,
        }
    }

    pub fn arg<'b: 'a, S: AsRef<OsStr> + ?Sized>(mut self, arg: &'b S) -> Self {
        self.args.push(arg.as_ref());
        self
    }

    pub fn args(mut self, args: Vec<&'a OsStr>) -> Self {
        self.args.extend(args);
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.envs.insert(key.to_string(), value.to_string());
        self
    }

    pub fn envs(mut self, envs: &HashMap<String, String>) -> Self {
        self.envs.extend(envs.clone());
        self
    }

    pub fn stdin(mut self, stdin: Stdio) -> Self {
        self.stdin = stdin;
        self
    }

    pub fn stdout(mut self, stdout: Stdio) -> Self {
        self.stdout = stdout;
        self
    }

    pub fn stderr(mut self, stderr: Stdio) -> Self {
        self.stderr = stderr;
        self
    }

    pub fn current_dir(mut self, current_dir: &'a Path) -> Self {
        self.current_dir = Some(current_dir);
        self
    }

    pub fn build(self) -> CommandSys {
        let mut cmd = CommandSys::new(self.cmd);
        cmd.args(self.args);
        cmd.envs(self.envs);
        cmd.stdin(self.stdin);
        cmd.stdout(self.stdout);
        cmd.stderr(self.stderr);
        if let Some(current_dir) = self.current_dir {
            cmd.current_dir(current_dir);
        }
        cmd
    }
}
