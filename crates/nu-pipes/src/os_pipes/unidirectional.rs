use nu_protocol::StreamDataType;
use serde::{Deserialize, Serialize};

use crate::{
    io::{PipeReader, PipeWriter},
    os_pipes::{pipe_impl, PipeImplBase},
    PipeError, PipeFd, PipeFdType,
};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PipeDirection {
    UsServerThemClient,
    UsClientThemServer,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct UnOpenedPipe<T: HandleType> {
    pub datatype: StreamDataType,

    pub(crate) fd: PipeFd,
    pub(crate) other_fd: PipeFd,
    pub mode: PipeMode,
    pub(crate) ty: T,
}

impl<T: HandleType> UnOpenedPipe<T> {
    pub fn close(&self) -> Result<(), PipeError> {
        pipe_impl::PipeImpl::close_pipe(&self.fd)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Pipe<T: HandleType> {
    pub(crate) datatype: StreamDataType,

    pub(crate) fd: PipeFd,
    pub(crate) mode: PipeMode,
    marker: std::marker::PhantomData<T>,
}

impl<T: HandleType> Pipe<T> {
    pub fn invalid() -> Self {
        Self {
            datatype: StreamDataType::Binary,
            #[cfg(windows)]
            fd: PipeFd(
                windows::Win32::Foundation::INVALID_HANDLE_VALUE,
                PipeFdType::Read,
            ),
            #[cfg(unix)]
            fd: PipeFd(-1, PipeFdType::Read),
            mode: PipeMode::CrossProcess,
            marker: std::marker::PhantomData,
        }
    }
}

/// Creates a new pipe. Pipes are unidirectional streams of bytes composed of a read end and a write end. They can be used for interprocess communication.
/// Uses `pipe(2)` on unix and `CreatePipe` on windows.
pub fn pipe(
    arg: PipeOptions,
) -> Result<(UnOpenedPipe<PipeRead>, UnOpenedPipe<PipeWrite>), PipeError> {
    let pipe = pipe_impl::PipeImpl::create_pipe()?;
    assert!(pipe.write_fd.1 == PipeFdType::Write);
    assert!(pipe.read_fd.1 == PipeFdType::Read);

    let rp = UnOpenedPipe {
        datatype: StreamDataType::Binary,
        fd: pipe.read_fd,
        other_fd: pipe.write_fd,
        mode: arg.mode,
        ty: PipeRead(std::marker::PhantomData),
    };
    let wp = UnOpenedPipe {
        datatype: StreamDataType::Binary,
        fd: pipe.write_fd,
        other_fd: pipe.read_fd,
        mode: arg.mode,
        ty: PipeWrite(std::marker::PhantomData),
    };
    Ok((rp, wp))
}

pub trait HandleIO<T: HandleType> {
    fn get_pipe(&self) -> &Pipe<T>;
}

trait OpenablePipe {
    type Inner: Sized;
    fn open(&self) -> Result<Self::Inner, PipeError>;
}

impl UnOpenedPipe<PipeRead> {
    pub fn open(&self) -> Result<PipeReader, PipeError> {
        if pipe_impl::PipeImpl::should_close_other_for_mode(self.mode) {
            // close both their ends of the pipe in our process
            pipe_impl::PipeImpl::close_pipe(&self.other_fd)?;
        }
        let pipe = Pipe {
            datatype: self.datatype,
            fd: self.fd,
            mode: self.mode,
            marker: std::marker::PhantomData,
        };

        Ok(PipeReader::new(pipe))
    }
}
impl UnOpenedPipe<PipeWrite> {
    pub fn open(&self) -> Result<PipeWriter, PipeError> {
        if pipe_impl::PipeImpl::should_close_other_for_mode(self.mode) {
            // close both their ends of the pipe in our process
            pipe_impl::PipeImpl::close_pipe(&self.other_fd)?;
        }

        let pipe = Pipe {
            datatype: self.datatype,
            fd: self.fd,
            mode: self.mode,
            marker: std::marker::PhantomData,
        };

        Ok(PipeWriter::new(pipe))
    }
}

impl<T: HandleType> Pipe<T> {
    pub fn close(&self) -> Result<(), PipeError> {
        pipe_impl::PipeImpl::close_pipe(&self.fd)
    }

    pub fn mode(&self) -> PipeMode {
        self.mode
    }
}

impl std::io::Read for Pipe<PipeRead> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::read(&self.fd, buf)?)
    }
}

impl std::io::Write for Pipe<PipeWrite> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::write(&self.fd, buf)?)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl std::io::Read for &Pipe<PipeRead> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::read(&self.fd, buf)?)
    }
}
impl std::io::Write for &Pipe<PipeWrite> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::write(&self.fd, buf)?)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PipeRead(std::marker::PhantomData<()>);
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PipeWrite(std::marker::PhantomData<()>);

pub trait HandleType {
    const NAME: &'static str;
}

impl HandleType for PipeRead {
    const NAME: &'static str = "read";
}
impl HandleType for PipeWrite {
    const NAME: &'static str = "write";
}

macro_rules! impl_serialize_deserialize {
    ($type:ty) => {
        impl $type {
            fn new() -> Self {
                Self(std::marker::PhantomData)
            }
        }

        impl Serialize for $type {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(<$type>::NAME)
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl serde::de::Visitor<'_> for Visitor {
                    type Value = $type;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str(<$type>::NAME)
                    }

                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<$type, E> {
                        if value == <$type>::NAME {
                            Ok(<$type>::new())
                        } else {
                            Err(E::custom(format!(
                                "expected {}, got {}",
                                <$type>::NAME,
                                value
                            )))
                        }
                    }
                }
                d.deserialize_str(Visitor)
            }
        }
    };
}

impl_serialize_deserialize!(PipeRead);
impl_serialize_deserialize!(PipeWrite);

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum PipeMode {
    CrossProcess,
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
