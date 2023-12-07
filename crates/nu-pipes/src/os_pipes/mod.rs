use serde::{Deserialize, Serialize};

use crate::errors::{PipeError, PipeResult};

use self::{
    io::{PipeReader, PipeWriter},
    pipe_impl::InnerHandleType,
    unidirectional::PipeMode,
};
pub use pipe_impl::OSError;

// pub mod bidirectional;
pub mod io;
pub mod unidirectional;

#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(unix, path = "unix.rs")]
mod pipe_impl;

trait PipeImplBase {
    fn create_pipe() -> Result<OsPipe, PipeError>;

    fn close_handle(handle: &Handle) -> PipeResult<()>;

    fn read_handle(handle: &Handle, buf: &mut [u8]) -> PipeResult<usize>;

    fn write_handle(handle: &Handle, buf: &[u8]) -> PipeResult<usize>;

    fn should_close_other_for_mode(mode: PipeMode) -> bool;
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub(crate) struct OsPipe {
    read_handle: Handle,
    write_handle: Handle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HandleTypeEnum {
    #[serde(rename = "read")]
    Read,
    #[serde(rename = "write")]
    Write,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Handle(
    #[cfg_attr(windows, serde(with = "pipe_impl::handle_serialization"))] pub(crate) InnerHandleType,
    pub(crate) HandleTypeEnum,
);

impl Handle {
    pub fn close(&self) -> Result<(), PipeError> {
        pipe_impl::PipeImpl::close_handle(self)
    }

    #[allow(non_snake_case)]
    #[inline(always)]
    fn Read(handle: InnerHandleType) -> Handle {
        Handle(handle, HandleTypeEnum::Read)
    }

    #[allow(non_snake_case)]
    #[inline(always)]
    fn Write(handle: InnerHandleType) -> Handle {
        Handle(handle, HandleTypeEnum::Write)
    }

    #[inline(always)]
    fn native(&self) -> InnerHandleType {
        self.0
    }
}

impl From<Handle> for InnerHandleType {
    fn from(val: Handle) -> Self {
        val.0
    }
}

pub trait HandleIO {
    /// Returns the handle of the object.
    fn handle(&self) -> Handle;
    fn encoding(&self) -> PipeEncoding;
}

pub trait AsNativeHandle {
    /// Returns the native handle of the object.
    fn as_native_handle(&self) -> InnerHandleType;
}

impl AsNativeHandle for Handle {
    fn as_native_handle(&self) -> InnerHandleType {
        self.native()
    }
}

impl<T: HandleIO> AsNativeHandle for T {
    fn as_native_handle(&self) -> InnerHandleType {
        self.handle().native()
    }
}

impl HandleIO for PipeWriter<'_> {
    fn handle(&self) -> Handle {
        self.pipe.handle
    }

    fn encoding(&self) -> PipeEncoding {
        self.pipe.encoding
    }
}

impl HandleIO for PipeReader {
    fn handle(&self) -> Handle {
        self.pipe.handle
    }

    fn encoding(&self) -> PipeEncoding {
        self.pipe.encoding
    }
}

pub trait Closeable: HandleIO {
    /// Closes the object.
    fn close(&mut self) -> Result<(), std::io::Error>;
}

impl std::fmt::Display for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(windows)]
        let handle = self.0 .0;
        #[cfg(unix)]
        let handle = self.0;
        write!(f, "{:?} ({})", self.1, handle)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PipeEncoding {
    Zstd,
    None,
}

impl PipeEncoding {
    #[inline(always)]
    pub fn recommended_input_size(&self) -> usize {
        match self {
            PipeEncoding::Zstd => zstd_safe::DCtx::in_size(),
            PipeEncoding::None => 16 * 1024,
        }
    }
}
