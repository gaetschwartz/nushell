use std::io::Read;

use nu_protocol::{CustomValue, ShellError, Span, Value};
use serde::{Deserialize, Serialize};

trait NamedPipeImpl: Sized {
    fn create(span: Span) -> Result<Self, PipeError>;
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct OsPipe {
    pub span: Span,

    #[serde(with = "windows_handle_serialization")]
    #[cfg(target_env = "msvc")]
    read_handle: Option<windows::Win32::Foundation::HANDLE>,

    #[serde(skip)]
    #[cfg(target_env = "msvc")]
    write_handle: Option<windows::Win32::Foundation::HANDLE>,
}

impl NamedPipeImpl for OsPipe {
    fn create(span: Span) -> Result<Self, PipeError> {
        #[cfg(target_env = "libc")]
        {
            use std::libc::mkfifo;
            use std::os::unix::ffi::OsStrExt;

            let c_name = std::ffi::CString::new(name.as_bytes()).unwrap();
            let c_mode = 0o644;
            let result = unsafe { mkfifo(c_name.as_ptr(), c_mode) };
            if result == 0 {
                Ok(OsPipe { name, span })
            } else {
                Err(())
            }
        }
        #[cfg(target_env = "msvc")]
        {
            use windows::Win32::System::Pipes::CreatePipe;

            let mut read_handle = windows::Win32::Foundation::INVALID_HANDLE_VALUE;
            let mut write_handle = windows::Win32::Foundation::INVALID_HANDLE_VALUE;

            unsafe { CreatePipe(&mut read_handle, &mut write_handle, None, 0) }
                .map_err(|e| PipeError::FailedToCreatePipe(OSError(e)))?;

            println!("Created pipe.");

            Ok(OsPipe {
                span,
                read_handle: Some(read_handle),
                write_handle: Some(write_handle),
            })
        }
    }
}

impl std::io::Read for OsPipe {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        #[cfg(target_env = "libc")]
        {
            use std::libc::{open, read, O_RDONLY};
            use std::os::unix::ffi::OsStrExt;

            let c_name = std::ffi::CString::new(self.name.as_bytes()).unwrap();
            let fd = unsafe { open(c_name.as_ptr(), O_RDONLY, 0) };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }

            let result = unsafe { read(fd, buf.as_mut_ptr() as *mut _, buf.len()) };
            if result < 0 {
                return Err(std::io::Error::last_os_error());
            }

            Ok(result as usize)
        }
        #[cfg(target_env = "msvc")]
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
    }
}

impl std::io::Write for OsPipe {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        #[cfg(target_env = "libc")]
        {
            use std::libc::{open, write, O_WRONLY};
            use std::os::unix::ffi::OsStrExt;

            let c_name = std::ffi::CString::new(self.name.as_bytes()).unwrap();
            let fd = unsafe { open(c_name.as_ptr(), O_WRONLY, 0) };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }

            let result = unsafe { write(fd, buf.as_ptr() as *const _, buf.len()) };
            if result < 0 {
                return Err(std::io::Error::last_os_error());
            }

            Ok(result as usize)
        }
        #[cfg(target_env = "msvc")]
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
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct StreamCustomValue {
    pub span: Span,
    pub named_pipe: OsPipe,
}

impl CustomValue for StreamCustomValue {
    fn clone_value(&self, span: Span) -> Value {
        Value::custom_value(Box::new(self.clone()), span)
    }

    fn value_string(&self) -> String {
        "StreamCustomValue".to_string()
    }

    fn to_base_value(&self, span: Span) -> Result<Value, ShellError> {
        let val = Vec::new();
        _ = self.named_pipe.clone().read_to_end(&mut val.clone())?;
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
        todo!()
    }
}

impl std::io::Read for StreamCustomValue {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.named_pipe.read(buf)
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PipeError {
    InvalidPipeName(String),
    UnexpectedInvalidPipeHandle,
    FailedToCreatePipe(OSError),
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OSError(
    #[cfg(target_env = "msvc")] windows::core::Error,
    #[cfg(not(target_env = "msvc"))] std::io::Error,
);

#[cfg(target_env = "msvc")]
impl From<windows::core::Error> for OSError {
    fn from(error: windows::core::Error) -> Self {
        OSError(error)
    }
}

#[cfg(target_env = "msvc")]
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
    }

    #[test]
    fn test_serialized_pipe() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        // write hello world to the pipe
        let written = pipe.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        // serialize the pipe
        let serialized = serde_json::to_string(&pipe).unwrap();
        let mut deserialized: OsPipe = serde_json::from_str(&serialized).unwrap();

        let mut buf = [0u8; 11];

        let read = deserialized.read(&mut buf).unwrap();

        assert_eq!(read, 11);
        assert_eq!(buf, "hello world".as_bytes());
    }
}
