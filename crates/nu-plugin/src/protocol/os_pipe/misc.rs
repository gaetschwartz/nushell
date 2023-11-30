use nu_protocol::ShellError;

use crate::Handle;

#[derive(Debug)]
pub struct OSError(
    #[cfg(windows)] windows::core::Error,
    #[cfg(unix)] std::io::Error,
);

#[cfg(unix)]
impl From<std::io::Error> for OSError {
    fn from(error: std::io::Error) -> Self {
        OSError(error)
    }
}

#[derive(Debug)]
pub enum PipeError {
    UnexpectedInvalidPipeHandle,
    FailedToCreatePipe(OSError),
    UnsupportedPlatform,
    FailedToCloseHandle(Handle, OSError),
    FailedToRead(Handle, std::io::Error),
    FailedToWrite(Handle, std::io::Error),
    FailedSetNamedPipeHandleState(Handle, OSError),
}

#[allow(dead_code)]
pub type PipeResult<T> = Result<T, PipeError>;

#[cfg(windows)]
pub type InnerHandleType = windows::Win32::Foundation::HANDLE;
#[cfg(unix)]
pub type InnerHandleType = libc::c_int;

impl From<PipeError> for std::io::Error {
    fn from(error: PipeError) -> Self {
        match error {
            PipeError::UnexpectedInvalidPipeHandle => {
                std::io::Error::new(std::io::ErrorKind::Other, "Unexpected invalid pipe handle")
            }
            PipeError::FailedToCreatePipe(error) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create pipe: {}", error.0),
            ),
            PipeError::UnsupportedPlatform => {
                std::io::Error::new(std::io::ErrorKind::Other, "Unsupported platform for pipes")
            }
            PipeError::FailedToCloseHandle(_, error) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to close pipe handle: {}", error.0),
            ),
            PipeError::FailedToRead(_, error) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read from pipe: {}", error),
            ),
            PipeError::FailedToWrite(_, error) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to write to pipe: {}", error),
            ),
            PipeError::FailedSetNamedPipeHandleState(_, error) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to set named pipe handle state: {:?}", error),
            ),
        }
    }
}

impl From<PipeError> for ShellError {
    fn from(error: PipeError) -> Self {
        match error {
            PipeError::UnexpectedInvalidPipeHandle => {
                ShellError::IOError("Unexpected invalid pipe handle".to_string())
            }
            PipeError::FailedToCreatePipe(error) => {
                ShellError::IOError(format!("Failed to create pipe: {}", error.0))
            }
            PipeError::UnsupportedPlatform => {
                ShellError::IOError("Unsupported platform for pipes".to_string())
            }
            PipeError::FailedToCloseHandle(v, e) => {
                ShellError::IOError(format!("Failed to close pipe handle {:?}: {}", v, e.0))
            }
            PipeError::FailedToRead(v, e) => {
                ShellError::IOError(format!("Failed to read from pipe {:?}: {}", v, e))
            }
            PipeError::FailedToWrite(v, e) => {
                ShellError::IOError(format!("Failed to write to pipe {:?}: {}", v, e))
            }
            PipeError::FailedSetNamedPipeHandleState(v, e) => ShellError::IOError(format!(
                "Failed to set named pipe handle state {:?}: {:?}",
                v, e
            )),
        }
    }
}

impl std::fmt::Display for PipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipeError::UnexpectedInvalidPipeHandle => {
                write!(f, "Unexpected invalid pipe handle")
            }
            PipeError::FailedToCreatePipe(error) => {
                write!(f, "Failed to create pipe: {}", error.0)
            }
            PipeError::UnsupportedPlatform => write!(f, "Unsupported platform for pipes"),
            PipeError::FailedToCloseHandle(v, e) => {
                write!(f, "Failed to close pipe handle {:?}: {}", v, e.0)
            }
            PipeError::FailedToRead(v, e) => {
                write!(f, "Failed to read from pipe {:?}: {}", v, e)
            }
            PipeError::FailedToWrite(v, e) => {
                write!(f, "Failed to write to pipe {:?}: {}", v, e)
            }
            PipeError::FailedSetNamedPipeHandleState(v, e) => {
                write!(f, "Failed to set named pipe handle state {:?}: {:?}", v, e)
            }
        }
    }
}
