use nu_protocol::{PipelineData, RawStream, ShellError};

use crate::{Handle, OSError};

#[cfg(unix)]
impl From<std::io::Error> for OSError {
    fn from(error: std::io::Error) -> Self {
        OSError(error)
    }
}

#[derive(Debug, Clone)]
pub enum PipeError {
    UnexpectedInvalidPipeHandle,
    FailedToCreatePipe(OSError),
    UnsupportedPlatform,
    FailedToCloseHandle(Handle, OSError),
    FailedToRead(Handle, OSError),
    FailedToWrite(Handle, OSError),
    FailedSetNamedPipeHandleState(Handle, OSError),
}

#[allow(dead_code)]
pub type PipeResult<T> = Result<T, PipeError>;

impl From<PipeError> for std::io::Error {
    fn from(error: PipeError) -> Self {
        let shellerror = ShellError::from(error);
        std::io::Error::new(std::io::ErrorKind::Other, shellerror)
    }
}

impl From<PipeError> for ShellError {
    fn from(error: PipeError) -> Self {
        match error {
            PipeError::UnexpectedInvalidPipeHandle => {
                ShellError::IOError("Unexpected invalid pipe handle".to_string())
            }
            PipeError::FailedToCreatePipe(error) => {
                ShellError::IOError(format!("Failed to create pipe: {}", error))
            }
            PipeError::UnsupportedPlatform => {
                ShellError::IOError("Unsupported platform for pipes".to_string())
            }
            PipeError::FailedToCloseHandle(v, e) => {
                ShellError::IOError(format!("Failed to close pipe handle {:?}: {}", v, e))
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
        ShellError::from(self.clone()).fmt(f)
    }
}

pub trait MaybeRawStream {
    fn take_stream(&mut self) -> Option<RawStream>;
}

impl MaybeRawStream for PipelineData {
    fn take_stream(&mut self) -> Option<RawStream> {
        match self {
            PipelineData::Value { .. } => None,
            PipelineData::ListStream { .. } => None,
            PipelineData::ExternalStream { stdout, .. } => stdout.take(),
            PipelineData::Empty => None,
        }
    }
}

impl MaybeRawStream for Option<PipelineData> {
    fn take_stream(&mut self) -> Option<RawStream> {
        match self {
            Some(PipelineData::ExternalStream { stdout, .. }) => stdout.take(),
            _ => None,
        }
    }
}
