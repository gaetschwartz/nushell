use nu_protocol::StreamDataType;
use serde::{Deserialize, Serialize};

use crate::{
    io::{PipeReader, PipeWriter},
    os_pipes::{pipe_impl, PipeImplBase},
    PipeError, PipeFd,
};

use super::IntoPipeFd;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PipeDirection {
    UsServerThemClient,
    UsClientThemServer,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct UnOpenedPipe<T: PipeFdType> {
    pub datatype: StreamDataType,

    pub(crate) fd: PipeFd<T>,
    pub(crate) other_fd: PipeFd<T::Other>,
    pub mode: PipeMode,
}

impl<T: PipeFdType> UnOpenedPipe<T> {
    pub fn close(self) -> Result<(), PipeError> {
        pipe_impl::PipeImpl::close_pipe(self.fd)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Pipe<T: PipeFdType> {
    pub datatype: StreamDataType,

    pub(crate) fd: PipeFd<T>,
    pub mode: PipeMode,
    marker: std::marker::PhantomData<T>,
}

impl<T: PipeFdType> Pipe<T> {
    pub fn invalid() -> Self {
        Self {
            datatype: StreamDataType::Binary,
            fd: pipe_impl::PipeImpl::INVALID_FD_VALUE.into_pipe_fd(),
            mode: PipeMode::CrossProcess,
            marker: std::marker::PhantomData,
        }
    }
}
impl Pipe<PipeRead> {
    pub fn reader(self) -> PipeReader {
        PipeReader::new(self)
    }
}
impl Pipe<PipeWrite> {
    pub fn writer(self) -> PipeWriter {
        PipeWriter::new(self)
    }
}

/// Creates a new pipe. Pipes are unidirectional streams of bytes composed of a read end and a write end. They can be used for interprocess communication.
/// Uses `pipe(2)` on unix and `CreatePipe` on windows.
pub fn pipe(
    arg: PipeOptions,
) -> Result<(UnOpenedPipe<PipeRead>, UnOpenedPipe<PipeWrite>), PipeError> {
    let pipe = pipe_impl::PipeImpl::create_pipe()?;

    let rp = UnOpenedPipe {
        datatype: StreamDataType::Binary,
        fd: pipe.read_fd,
        other_fd: pipe.write_fd,
        mode: arg.mode,
    };
    let wp = UnOpenedPipe {
        datatype: StreamDataType::Binary,
        fd: pipe.write_fd,
        other_fd: pipe.read_fd,
        mode: arg.mode,
    };
    Ok((rp, wp))
}

pub trait HandleIO<T: PipeFdType> {
    fn get_pipe(&self) -> &Pipe<T>;
}

trait OpenablePipe {
    type Inner: Sized;
    fn open(&self) -> Result<Self::Inner, PipeError>;
}

impl UnOpenedPipe<PipeRead> {
    pub fn open(&self) -> Result<Pipe<PipeRead>, PipeError> {
        if pipe_impl::PipeImpl::should_close_other_for_mode(self.mode) {
            // close both their ends of the pipe in our process
            pipe_impl::PipeImpl::close_pipe(self.other_fd)?;
        }

        let pipe = Pipe {
            datatype: self.datatype,
            fd: self.fd,
            mode: self.mode,
            marker: std::marker::PhantomData,
        };

        Ok(pipe)
    }
}
impl UnOpenedPipe<PipeWrite> {
    pub fn open(&self) -> Result<Pipe<PipeWrite>, PipeError> {
        if pipe_impl::PipeImpl::should_close_other_for_mode(self.mode) {
            // close both their ends of the pipe in our process
            pipe_impl::PipeImpl::close_pipe(self.other_fd)?;
        }

        let pipe = Pipe {
            datatype: self.datatype,
            fd: self.fd,
            mode: self.mode,
            marker: std::marker::PhantomData,
        };

        Ok(pipe)
    }
}

impl<T: PipeFdType> Pipe<T> {
    pub fn close(&self) -> Result<(), PipeError> {
        pipe_impl::PipeImpl::close_pipe(self.fd)
    }

    pub fn mode(&self) -> PipeMode {
        self.mode
    }
}

impl std::io::Read for Pipe<PipeRead> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::read(self.fd, buf)?)
    }
}

impl std::io::Write for Pipe<PipeWrite> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::write(self.fd, buf)?)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl std::io::Read for &Pipe<PipeRead> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::read(self.fd, buf)?)
    }
}
impl std::io::Write for &Pipe<PipeWrite> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::write(self.fd, buf)?)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub struct PipeRead(std::marker::PhantomData<()>);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub struct PipeWrite(std::marker::PhantomData<()>);

pub trait PipeFdType: Sized + Copy + 'static {
    const NAME: char;
    const TYPE: PipeFdTypeEnum;
    type Other: PipeFdType;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PipeFdTypeEnum {
    Read,
    Write,
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum PipeMode {
    #[serde(rename = "xpc")]
    CrossProcess,
    #[serde(rename = "ipc")]
    InProcess,
}

pub struct PipeOptions {
    pub mode: PipeMode,
}

impl PipeOptions {
    pub fn new(mode: PipeMode) -> Self {
        Self { mode }
    }

    pub const IN_PROCESS: Self = Self {
        mode: PipeMode::InProcess,
    };
}

impl Default for PipeOptions {
    fn default() -> Self {
        Self {
            mode: PipeMode::CrossProcess,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{os_pipes::IntoPipeFd, unidirectional::PipeWrite};

    use super::{PipeMode, PipeRead, UnOpenedPipe};

    #[test]
    fn assert_pipe_cant_be_transmuted() {
        let read = UnOpenedPipe::<PipeRead> {
            datatype: nu_protocol::StreamDataType::Binary,
            fd: 12i32.into_pipe_fd(),
            other_fd: 42i32.into_pipe_fd(),
            mode: PipeMode::InProcess,
        };

        let serialized = serde_json::to_string(&read).unwrap();
        println!("{}", serialized);
        // deserialize the pipe
        let deserialized = serde_json::from_str::<UnOpenedPipe<PipeWrite>>(&serialized);

        assert!(deserialized.is_err());
        println!("This is expected: {:?}", deserialized.unwrap_err());
    }
}
