use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use crate::errors::{PipeError, PipeResult};

use self::{
    pipe_impl::NativeFd,
    unidirectional::{PipeFdType, PipeFdTypeEnum, PipeMode, PipeRead, PipeWrite},
};
pub use pipe_impl::OSError;

// pub mod bidirectional;
pub mod io;
pub mod unidirectional;

#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(unix, path = "unix.rs")]
mod pipe_impl;

pub(crate) trait PipeImplBase {
    fn create_pipe() -> Result<OsPipe, PipeError>;

    fn read(fd: impl AsPipeFd<PipeRead>, buf: &mut [u8]) -> PipeResult<usize>;

    fn write(fd: impl AsPipeFd<PipeWrite>, buf: &[u8]) -> PipeResult<usize>;

    fn close_pipe<T: PipeFdType>(fd: impl AsPipeFd<T>) -> PipeResult<()>;

    fn should_close_other_for_mode(mode: PipeMode) -> bool;

    const INVALID_FD_VALUE: NativeFd;
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub(crate) struct OsPipe {
    read_fd: PipeFd<PipeRead>,
    write_fd: PipeFd<PipeWrite>,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(transparent)]
pub struct PipeFd<T: PipeFdType + ?Sized>(
    #[cfg_attr(windows, serde(with = "pipe_impl::handle_serialization"))] pub(crate) NativeFd,
    PhantomData<T>,
);

impl<T: PipeFdType> std::fmt::Debug for PipeFd<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(windows)]
        let fd = self.0 .0;
        #[cfg(unix)]
        let fd = self.0;
        match T::TYPE {
            PipeFdTypeEnum::Read => write!(f, "PipeFd::Read({})", fd),
            PipeFdTypeEnum::Write => write!(f, "PipeFd::Write({})", fd),
            PipeFdTypeEnum::Unknown => write!(f, "PipeFd::Unknown({})", fd),
        }
    }
}

impl<T: PipeFdType> PipeFd<T> {
    pub fn close(&self) -> Result<(), PipeError> {
        pipe_impl::PipeImpl::close_pipe(self)
    }
}

impl<T: PipeFdType> From<PipeFd<T>> for NativeFd {
    fn from(val: PipeFd<T>) -> Self {
        val.0
    }
}

trait IntoPipeFd<T: PipeFdType>: AsNativeFd {
    fn into_pipe_fd(self) -> PipeFd<T>;
}

impl<T: PipeFdType, F: AsNativeFd> IntoPipeFd<T> for F {
    fn into_pipe_fd(self) -> PipeFd<T> {
        PipeFd(self.as_native_fd(), PhantomData)
    }
}

pub trait AsNativeFd {
    /// Returns the native handle of the object.
    fn as_native_fd(&self) -> NativeFd;
}

impl<T: PipeFdType> AsNativeFd for PipeFd<T> {
    fn as_native_fd(&self) -> NativeFd {
        self.0
    }
}
impl AsNativeFd for NativeFd {
    fn as_native_fd(&self) -> NativeFd {
        *self
    }
}
impl<T: AsNativeFd> AsNativeFd for &T {
    fn as_native_fd(&self) -> NativeFd {
        (*self).as_native_fd()
    }
}

pub trait AsPipeFd<T: PipeFdType> {
    fn as_pipe_fd(&self) -> &PipeFd<T>;
}
impl<T: PipeFdType> AsPipeFd<T> for PipeFd<T> {
    fn as_pipe_fd(&self) -> &PipeFd<T> {
        self
    }
}
impl AsPipeFd<PipeRead> for OsPipe {
    fn as_pipe_fd(&self) -> &PipeFd<PipeRead> {
        &self.read_fd
    }
}
impl AsPipeFd<PipeWrite> for OsPipe {
    fn as_pipe_fd(&self) -> &PipeFd<PipeWrite> {
        &self.write_fd
    }
}

impl<T: PipeFdType, F: AsPipeFd<T>> AsPipeFd<T> for &F {
    fn as_pipe_fd(&self) -> &PipeFd<T> {
        (*self).as_pipe_fd()
    }
}
impl<T: PipeFdType> AsNativeFd for dyn AsPipeFd<T> {
    fn as_native_fd(&self) -> NativeFd {
        self.as_pipe_fd().as_native_fd()
    }
}

pub trait PipeFdHasType {
    fn get_type(&self) -> PipeFdTypeEnum;
}

impl<T: PipeFdType> PipeFdHasType for PipeFd<T> {
    fn get_type(&self) -> PipeFdTypeEnum {
        T::TYPE
    }
}

impl<T: PipeFdType> std::fmt::Display for PipeFd<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(windows)]
        let fd = self.0 .0;
        #[cfg(unix)]
        let fd = self.0;
        write!(f, "{:?} ({})", T::NAME, fd)
    }
}
