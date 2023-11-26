use std::{
    io::{Read, Write},
    thread::JoinHandle,
};

use nu_protocol::{CustomValue, PipelineData, ShellError, Span, Spanned, StreamDataType, Value};
use serde::{Deserialize, Serialize};

pub use self::pipe_impl::OsPipe;

trait OsPipeTrait: Read + Write + Send + Sync + Serialize + Deserialize<'static> {
    fn create(span: Span) -> Result<Self, PipeError>;
    fn close(&mut self) -> Result<(), PipeError>;
}

use super::CallInput;
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(unix, path = "unix.rs")]
mod pipe_impl;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct StreamCustomValue {
    pub span: Span,
    pub os_pipe: OsPipe,
    vec: Option<Vec<u8>>,
}

impl StreamCustomValue {
    pub fn new(os_pipe: OsPipe, span: Span) -> Self {
        Self {
            span,
            os_pipe,
            vec: None,
        }
    }

    pub fn read_pipe_to_end(&mut self) -> Result<&Vec<u8>, ShellError> {
        if let None = self.vec {
            let mut vec = Vec::new();
            _ = self.os_pipe.clone().read_to_end(&mut vec)?;
            self.vec = Some(vec);
        }
        if let Some(vec) = &self.vec {
            Ok(vec)
        } else {
            unreachable!()
        }
    }
}

impl CustomValue for StreamCustomValue {
    fn clone_value(&self, span: Span) -> Value {
        Value::custom_value(Box::new(self.clone()), span)
    }

    fn value_string(&self) -> String {
        self.to_base_value(self.span)
            .map(|v| v.as_string().unwrap_or_default())
            .unwrap_or_default()
    }

    fn to_base_value(&self, span: Span) -> Result<Value, ShellError> {
        match self.os_pipe.datatype {
            StreamDataType::Binary => {
                let val = Vec::new();
                _ = self.os_pipe.clone().read_to_end(&mut val.clone())?;
                Ok(Value::binary(val, span))
            }
            StreamDataType::Text => {
                let mut vec = Vec::new();
                _ = self.os_pipe.clone().read_to_end(&mut vec)?;
                let string = String::from_utf8_lossy(&vec);
                Ok(Value::string(string, span))
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn span(&self) -> Span {
        self.span
    }

    #[doc(hidden)]
    fn typetag_name(&self) -> &'static str {
        match self.os_pipe.datatype {
            StreamDataType::Binary => "StreamCustomValue::Binary",
            StreamDataType::Text => "StreamCustomValue::Text",
        }
    }

    #[doc(hidden)]
    fn typetag_deserialize(&self) {
        unimplemented!("typetag_deserialize")
    }

    fn as_binary(&self) -> Result<&[u8], ShellError> {
        let vec = self.vec.as_ref().ok_or_else(|| ShellError::CantConvert {
            to_type: "binary".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })?;
        Ok(vec.as_slice())
    }

    fn as_string(&self) -> Result<String, ShellError> {
        self.as_binary()
            .map(|b| String::from_utf8_lossy(b).to_string())
    }

    fn as_spanned_string(&self) -> Result<nu_protocol::Spanned<String>, ShellError> {
        self.as_binary()
            .map(|b| String::from_utf8_lossy(b).to_string())
            .map(|s| Spanned {
                item: s,
                span: self.span,
            })
    }
}

impl std::io::Read for StreamCustomValue {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.os_pipe.read(buf)
    }
}

#[derive(Debug)]
pub enum PipeError {
    InvalidPipeName(String),
    UnexpectedInvalidPipeHandle,
    FailedToCreatePipe(OSError),
    UnsupportedPlatform,
    FailedToClose(Option<OSError>),
}

impl From<PipeError> for ShellError {
    fn from(error: PipeError) -> Self {
        match error {
            PipeError::InvalidPipeName(name) => ShellError::IncorrectValue {
                msg: format!("Invalid pipe name: {}", name),
                val_span: Span::unknown(),
                call_span: Span::unknown(),
            },
            PipeError::UnexpectedInvalidPipeHandle => {
                ShellError::IOError("Unexpected invalid pipe handle".to_string())
            }
            PipeError::FailedToCreatePipe(error) => {
                ShellError::IOError(format!("Failed to create pipe: {}", error.0.to_string()))
            }
            PipeError::UnsupportedPlatform => {
                ShellError::IOError("Unsupported platform for pipes".to_string())
            }
            PipeError::FailedToClose(e) => match e {
                Some(e) => {
                    ShellError::IOError(format!("Failed to close pipe: {}", e.0.to_string()))
                }
                None => ShellError::IOError("Failed to close pipe".to_string()),
            },
        }
    }
}

impl OsPipe {
    /// Starts a new thread that will pipe the stdout stream to the os pipe.
    ///
    /// Returns a handle to the thread if the input is a pipe and the output is an external stream.
    pub fn start_pipe(input: &mut CallInput) -> Result<Option<JoinHandle<()>>, ShellError> {
        match input {
            CallInput::Pipe(os_pipe, Some(PipelineData::ExternalStream { stdout, .. })) => {
                let handle = {
                    // unsafely move the stdout stream to the new thread by casting to a void pointer
                    let stdout = std::mem::replace(stdout, None);
                    let Some(stdout) = stdout else {
                        return Ok(None);
                    };
                    let os_pipe = os_pipe.clone();

                    std::thread::spawn(move || {
                        let mut os_pipe = os_pipe;
                        let stdout = stdout;
                        os_pipe.datatype = stdout.datatype;

                        for e in stdout.stream {
                            if let Ok(ref e) = e {
                                let _ = os_pipe.write(e.as_slice());
                            }
                        }
                    })
                };

                Ok(Some(handle))
            }
            _ => Ok(None),
        }
    }
}

#[derive(Debug)]
pub struct OSError(
    #[cfg(windows)] windows::core::Error,
    #[cfg(not(windows))] std::io::Error,
);

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn test_pipe() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        // write hello world to the pipe
        let written = pipe.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        let mut buf = [0u8; 11];

        let read = pipe.read(&mut buf).unwrap();

        assert_eq!(read, 11);
        assert_eq!(buf, "hello world".as_bytes());

        pipe.close().unwrap();
    }

    #[test]
    fn test_serialized_pipe() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        // write hello world to the pipe
        let written = pipe.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        // serialize the pipe
        let serialized = serde_json::to_string(&pipe).unwrap();
        // deserialize the pipe
        let mut deserialized: OsPipe = serde_json::from_str(&serialized).unwrap();

        let mut buf = [0u8; 11];

        let read = deserialized.read(&mut buf).unwrap();

        assert_eq!(read, 11);
        assert_eq!(buf, "hello world".as_bytes());

        pipe.close().unwrap();
    }

    #[test]
    fn test_pipe_in_another_thread() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        // write hello world to the pipe
        let written = pipe.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        // serialize the pipe
        let serialized = serde_json::to_string(&pipe).unwrap();
        // spawn a new process
        std::thread::spawn(move || {
            // deserialize the pipe
            let mut deserialized: OsPipe = serde_json::from_str(&serialized).unwrap();

            let mut buf = [0u8; 11];

            let read = deserialized.read(&mut buf).unwrap();

            assert_eq!(read, 11);
            assert_eq!(buf, "hello world".as_bytes());

            deserialized.close().unwrap();
        });
    }

    #[test]
    fn test_pipe_in_another_process() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        // write hello world to the pipe
        let written = pipe.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        // serialize the pipe
        let serialized = serde_json::to_string(&pipe).unwrap();
        // spawn a new process
        let res = std::process::Command::new("cargo")
            .arg("run")
            .arg("-q")
            .arg("--bin")
            .arg("nu_plugin_pipe_echoer")
            .arg(serialized)
            .output()
            .unwrap();

        if !res.status.success() {
            eprintln!("stderr: {}", String::from_utf8_lossy(res.stderr.as_slice()));
            assert!(false);
        }

        assert_eq!(res.status.success(), true);
        assert_eq!(
            String::from_utf8_lossy(res.stdout.as_slice()),
            "hello world\n"
        );
    }
}
