use serde::{Deserialize, Serialize};

use crate::errors::{PipeError, PipeResult};

use self::{pipe_impl::NativeFd, unidirectional::PipeMode};
pub use pipe_impl::OSError;

// pub mod bidirectional;
pub mod io;
pub mod unidirectional;

#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(unix, path = "unix.rs")]
mod pipe_impl;

pub(crate) trait PipeImplBase {
    fn create_pipe() -> Result<OsPipe, PipeError>;

    fn close_pipe(fd: &PipeFd) -> PipeResult<()>;

    fn read(fd: &PipeFd, buf: &mut [u8]) -> PipeResult<usize>;

    fn write(fd: &PipeFd, buf: &[u8]) -> PipeResult<usize>;

    fn should_close_other_for_mode(mode: PipeMode) -> bool;
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub(crate) struct OsPipe {
    read_fd: PipeFd,
    write_fd: PipeFd,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PipeFdType {
    #[serde(rename = "read")]
    Read,
    #[serde(rename = "write")]
    Write,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipeFd(
    #[cfg_attr(windows, serde(with = "pipe_impl::handle_serialization"))] pub(crate) NativeFd,
    pub(crate) PipeFdType,
);

impl PipeFd {
    pub fn close(&self) -> Result<(), PipeError> {
        pipe_impl::PipeImpl::close_pipe(self)
    }

    #[allow(non_snake_case)]
    #[inline(always)]
    fn Read(handle: NativeFd) -> PipeFd {
        PipeFd(handle, PipeFdType::Read)
    }

    #[allow(non_snake_case)]
    #[inline(always)]
    fn Write(handle: NativeFd) -> PipeFd {
        PipeFd(handle, PipeFdType::Write)
    }

    #[inline(always)]
    fn native(&self) -> NativeFd {
        self.0
    }
}

impl From<PipeFd> for NativeFd {
    fn from(val: PipeFd) -> Self {
        val.0
    }
}

pub trait AsNativeFd {
    /// Returns the native handle of the object.
    fn as_native_fd(&self) -> NativeFd;
}

impl AsNativeFd for PipeFd {
    fn as_native_fd(&self) -> NativeFd {
        self.native()
    }
}

impl std::fmt::Display for PipeFd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(windows)]
        let handle = self.0 .0;
        #[cfg(unix)]
        let handle = self.0;
        write!(f, "{:?} ({})", self.1, handle)
    }
}
