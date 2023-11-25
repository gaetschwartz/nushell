use std::{
    io::{Read, Write},
    thread::JoinHandle,
};

use nu_protocol::{CustomValue, PipelineData, ShellError, Span, Value};
use serde::{Deserialize, Serialize};

use super::CallInput;

#[cfg(windows)]
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct OsPipe {
    pub span: Span,

    #[serde(with = "windows_handle_serialization")]
    read_handle: Option<windows::Win32::Foundation::HANDLE>,

    #[serde(skip)]
    write_handle: Option<windows::Win32::Foundation::HANDLE>,
}

#[cfg(unix)]
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct OsPipe {
    pub span: Span,

    read_fd: libc::c_int,
    write_fd: libc::c_int,
}

impl OsPipe {
    pub fn create(span: Span) -> Result<Self, PipeError> {
        #[cfg(unix)]
        {
            use libc::pipe;

            let mut fds: [libc::c_int; 2] = [0; 2];
            let result = unsafe { pipe(fds.as_mut_ptr()) };
            if result == 0 {
                Ok(OsPipe {
                    span,
                    read_fd: fds[0],
                    write_fd: fds[1],
                })
            } else {
                Err(PipeError::UnexpectedInvalidPipeHandle)
            }
        }
        #[cfg(windows)]
        {
            use windows::Win32::Security::SECURITY_ATTRIBUTES;
            use windows::Win32::System::Pipes::CreatePipe;

            let mut read_handle = windows::Win32::Foundation::INVALID_HANDLE_VALUE;
            let mut write_handle = windows::Win32::Foundation::INVALID_HANDLE_VALUE;

            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: std::ptr::null_mut(),
                bInheritHandle: true.into(),
            };

            unsafe { CreatePipe(&mut read_handle, &mut write_handle, Some(&attributes), 0) }
                .map_err(|e| PipeError::FailedToCreatePipe(OSError(e)))?;

            Ok(OsPipe {
                span,
                read_handle: Some(read_handle),
                write_handle: Some(write_handle),
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(PipeError::UnsupportedPlatform)
        }
    }

    pub fn close(&mut self) -> Result<(), PipeError> {
        #[cfg(unix)]
        {
            use libc::close;

            let (read_res, write_res) = unsafe { (close(self.read_fd), close(self.write_fd)) };

            if read_res < 0 || write_res < 0 {
                return Err(PipeError::FailedToClose);
            }

            Ok(())
        }
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::CloseHandle;

            let read_res = self
                .read_handle
                .map(|handle| unsafe { CloseHandle(handle) });
            let write_res = self
                .write_handle
                .map(|handle| unsafe { CloseHandle(handle) });

            if let Some(e) = vec![read_res, write_res]
                .iter()
                .find_map(|res| res.as_ref().map(|e| e.as_ref().err()).flatten())
            {
                return Err(PipeError::FailedToClose(Some(OSError(e.clone()))));
            }

            Ok(())
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(PipeError::UnsupportedPlatform)
        }
    }
}

impl std::io::Read for OsPipe {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        #[cfg(unix)]
        {
            use libc::read;

            let result = unsafe { read(self.read_fd, buf.as_mut_ptr() as *mut _, buf.len()) };
            if result < 0 {
                return Err(std::io::Error::last_os_error());
            }

            Ok(result as usize)
        }
        #[cfg(windows)]
        {
            let mut bytes_read = 0;

            let Some(read_handle) = self.read_handle else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Missing read handle",
                ));
            };

            unsafe {
                windows::Win32::Storage::FileSystem::ReadFile(
                    read_handle,
                    Some(buf),
                    Some(&mut bytes_read),
                    None,
                )
            }
            .map_err(|e| std::io::Error::from(e))?;

            Ok(bytes_read as usize)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(PipeError::UnsupportedPlatform)
        }
    }
}

impl std::io::Write for OsPipe {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        #[cfg(unix)]
        {
            use libc::write;

            // https://stackoverflow.com/a/24099738
            // fifo is blocking

            let result = unsafe { write(self.write_fd, buf.as_ptr() as *const _, buf.len()) };
            if result < 0 {
                return Err(std::io::Error::last_os_error());
            }

            Ok(result as usize)
        }
        #[cfg(windows)]
        {
            let mut bytes_written = 0;

            let Some(write_handle) = self.write_handle else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Missing write handle",
                ));
            };

            unsafe {
                windows::Win32::Storage::FileSystem::WriteFile(
                    write_handle,
                    Some(buf),
                    Some(&mut bytes_written),
                    None,
                )
            }
            .map_err(|e| std::io::Error::from(e))?;

            Ok(bytes_written as usize)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(PipeError::UnsupportedPlatform)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct StreamCustomValue {
    pub span: Span,
    pub os_pipe: OsPipe,
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
        let val = Vec::new();
        _ = self.os_pipe.clone().read_to_end(&mut val.clone())?;
        Ok(Value::binary(val, span))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    #[doc(hidden)]
    fn typetag_name(&self) -> &'static str {
        "StreamCustomValue"
    }

    #[doc(hidden)]
    fn typetag_deserialize(&self) {
        unimplemented!("typetag_deserialize")
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

#[derive(Debug)]
pub struct OSError(
    #[cfg(windows)] windows::core::Error,
    #[cfg(not(windows))] std::io::Error,
);

#[cfg(windows)]
impl From<windows::core::Error> for OSError {
    fn from(error: windows::core::Error) -> Self {
        OSError(error)
    }
}

#[cfg(windows)]
pub mod windows_handle_serialization {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Debug, Deserialize, Serialize)]
    struct WrappedHandle(
        #[serde(
            deserialize_with = "deserialize_handle",
            serialize_with = "serialize_handle"
        )]
        windows::Win32::Foundation::HANDLE,
    );

    pub fn deserialize_handle<'de, D>(
        deserializer: D,
    ) -> Result<windows::Win32::Foundation::HANDLE, D::Error>
    where
        D: Deserializer<'de>,
    {
        let handle = <isize>::deserialize(deserializer)?;
        Ok(windows::Win32::Foundation::HANDLE(handle))
    }

    pub fn serialize_handle<S>(
        handle: &windows::Win32::Foundation::HANDLE,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        handle.0.serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<Option<windows::Win32::Foundation::HANDLE>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<WrappedHandle>::deserialize(deserializer).map(
            |opt_wrapped: Option<WrappedHandle>| {
                opt_wrapped.map(|wrapped: WrappedHandle| wrapped.0)
            },
        )
    }

    pub fn serialize<S>(
        handle: &Option<windows::Win32::Foundation::HANDLE>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let opt_wrapped = handle
            .as_ref()
            .map(|handle: &windows::Win32::Foundation::HANDLE| WrappedHandle(handle.clone()));
        opt_wrapped.serialize(serializer)
    }
}

impl CallInput {
    pub fn pipe(&mut self) -> Result<Option<JoinHandle<()>>, ShellError> {
        match self {
            CallInput::Pipe(os_pipe, Some(PipelineData::ExternalStream { stdout, .. })) => {
                let handle = {
                    // unsafely move the stdout stream to the new thread by casting to a void pointer
                    let stdout = std::mem::replace(stdout, None).unwrap();
                    let os_pipe = os_pipe.clone();

                    std::thread::spawn(move || {
                        let mut os_pipe = os_pipe;
                        let stdout = stdout;

                        for e in stdout.stream {
                            let _ = os_pipe.write(e.unwrap().as_slice());
                        }
                    })
                };

                Ok(Some(handle))
            }
            _ => Ok(None),
        }
    }
}

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
