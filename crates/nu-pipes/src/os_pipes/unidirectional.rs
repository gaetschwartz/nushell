//! Unidirectional pipes
use serde::{Deserialize, Serialize};

use crate::{
    os_pipes::{sys, PipeImplBase},
    AsPipeFd, PipeError, PipeFd,
};

/// Creates a new pipe. Pipes are unidirectional streams of bytes composed of a read end and a write end. They can be used for interprocess communication.
/// Uses `pipe(2)` on unix and `CreatePipe` on windows.
pub fn pipe() -> Result<(PipeFd<PipeRead>, PipeFd<PipeWrite>), PipeError> {
    let pipe = sys::PipeImpl::create_pipe()?;

    Ok((pipe.read_fd, pipe.write_fd))
}

pub(crate) struct RawPipeReader<T: AsPipeFd<PipeRead>>(pub(crate) T);
pub(crate) struct RawPipeWriter<T: AsPipeFd<PipeWrite>>(pub(crate) T);

impl<T: AsPipeFd<PipeRead>> std::io::Read for RawPipeReader<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(sys::PipeImpl::read(&self.0, buf)?)
    }
}
impl<T: AsPipeFd<PipeWrite>> std::io::Write for RawPipeWriter<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(sys::PipeImpl::write(&self.0, buf)?)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The "read" capability of a pipe.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub struct PipeRead(std::marker::PhantomData<()>);

/// The "write" capability of a pipe.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub struct PipeWrite(std::marker::PhantomData<()>);

/// A trait for `PipeFd` types. This is used to implement the `PipeFd` type for both `PipeRead` and `PipeWrite`.
pub trait PipeFdType: Sized + Copy + 'static {
    /// The name of the pipe type.
    const NAME: char;
    /// The type of the pipe.
    const TYPE: PipeFdTypeEnum;
    /// The other end of the pipe. If this is `PipeRead`, then `Other` is `PipeWrite` and vice versa.
    type Other: PipeFdType;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
/// Enum representing the type of pipe file descriptor.
pub enum PipeFdTypeEnum {
    /// Read end of the pipe.
    Read,
    /// Write end of the pipe.
    Write,
    /// Unknown type of pipe.
    Unknown,
}

impl PipeFdType for PipeRead {
    const NAME: char = 'r';
    const TYPE: PipeFdTypeEnum = PipeFdTypeEnum::Read;
    type Other = PipeWrite;
}
impl PipeFdType for PipeWrite {
    const NAME: char = 'w';
    const TYPE: PipeFdTypeEnum = PipeFdTypeEnum::Write;
    type Other = PipeRead;
}

#[cfg(test)]
mod tests {
    use crate::{unidirectional::PipeWrite, FromRawPipeFd, PipeFd};

    use super::PipeRead;

    #[test]
    fn assert_pipe_cant_be_transmuted() {
        let read: PipeFd<PipeRead> = unsafe { PipeFd::from_raw_pipe_fd(42) };

        let serialized = serde_json::to_string(&read).unwrap();
        println!("{}", serialized);
        // deserialize the pipe
        let deserialized = serde_json::from_str::<PipeFd<PipeWrite>>(&serialized);

        assert!(deserialized.is_err());
        println!("This is expected: {:?}", deserialized.unwrap_err());
    }
}
