use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use crate::{
    errors::{PipeError, PipeResult},
    PipeReader,
};

use self::{
    io::{CloseOwningError, OwningPipeReader, OwningPipeWriter, PipeWriter},
    sys::NativeFd,
    unidirectional::{PipeFdType, PipeFdTypeEnum, PipeRead, PipeWrite},
};
pub use sys::OSError;

// pub mod bidirectional;
pub mod io;
pub mod unidirectional;

#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(unix, path = "unix.rs")]
mod sys;

pub const PIPE_BUFFER_CAPACITY: usize = 1024 * 8;

pub(crate) trait PipeImplBase {
    fn create_pipe() -> Result<OsPipe, PipeError>;

    fn read(fd: impl AsPipeFd<PipeRead>, buf: &mut [u8]) -> PipeResult<usize>;

    fn write(fd: impl AsPipeFd<PipeWrite>, buf: &[u8]) -> PipeResult<usize>;

    fn close_pipe<T: PipeFdType>(fd: impl AsPipeFd<T>) -> PipeResult<()>;

    fn dup<T: PipeFdType>(fd: impl AsPipeFd<T>) -> PipeResult<PipeFd<T>>;

    const INVALID_FD_VALUE: NativeFd;
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub(crate) struct OsPipe {
    read_fd: PipeFd<PipeRead>,
    write_fd: PipeFd<PipeWrite>,
}

#[derive(Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct PipeFd<T: PipeFdType>(pub(crate) NativeFd, pub(crate) PhantomData<T>);

impl PipeFd<PipeRead> {
    pub fn into_reader(self) -> OwningPipeReader {
        OwningPipeReader::new(self)
    }
}

impl PipeFd<PipeWrite> {
    pub fn into_writer(self) -> OwningPipeWriter {
        OwningPipeWriter::new(self)
    }
}

impl<T: PipeFdType> PipeFd<T> {
    pub fn try_clone(&self) -> Result<PipeFd<T>, PipeError> {
        sys::PipeImpl::dup(self)
    }
}

impl<T: PipeFdType> From<i32> for PipeFd<T> {
    fn from(val: i32) -> Self {
        PipeFd(val.as_native_fd(), PhantomData)
    }
}

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
    pub fn close(self) -> Result<(), CloseOwningError<PipeFd<T>, PipeError>> {
        match sys::PipeImpl::close_pipe(&self) {
            Ok(()) => Ok(()),
            Err(e) => Err(CloseOwningError::new(e, self)),
        }
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
    #[inline]
    fn as_native_fd(&self) -> NativeFd {
        self.0
    }
}
impl AsNativeFd for NativeFd {
    #[inline]
    fn as_native_fd(&self) -> NativeFd {
        *self
    }
}
impl<T: AsNativeFd> AsNativeFd for &T {
    #[inline]
    fn as_native_fd(&self) -> NativeFd {
        (*self).as_native_fd()
    }
}

pub trait AsPipeFd<T: PipeFdType> {
    fn as_pipe_fd(&self) -> &PipeFd<T>;
}
impl<T: PipeFdType> AsPipeFd<T> for PipeFd<T> {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<T> {
        self
    }
}
impl AsPipeFd<PipeRead> for PipeReader<'_> {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<PipeRead> {
        self.fd
    }
}
impl AsPipeFd<PipeWrite> for PipeWriter<'_> {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<PipeWrite> {
        self.fd
    }
}
impl AsPipeFd<PipeRead> for OsPipe {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<PipeRead> {
        &self.read_fd
    }
}
impl AsPipeFd<PipeWrite> for OsPipe {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<PipeWrite> {
        &self.write_fd
    }
}

impl<T: PipeFdType, F: AsPipeFd<T>> AsPipeFd<T> for &F {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<T> {
        (*self).as_pipe_fd()
    }
}
impl<T: PipeFdType> AsNativeFd for dyn AsPipeFd<T> {
    #[inline]
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

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
struct PipeFdSer(#[cfg_attr(windows, serde(with = "sys::FdSerializable"))] pub(crate) NativeFd);

impl<T: PipeFdType> Serialize for PipeFd<T> {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> Result<<S as serde::Serializer>::Ok, <S as serde::Serializer>::Error>
    where
        S: serde::Serializer,
    {
        (PipeFdSer(self.0), T::NAME).serialize(serializer)
    }
}

impl<'de, T: PipeFdType> Deserialize<'de> for PipeFd<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (fd, name) = <(PipeFdSer, char)>::deserialize(deserializer)?;
        if name != T::NAME {
            return Err(serde::de::Error::custom(format!(
                "expected pipe type {}, got {}",
                T::NAME,
                name
            )));
        }
        Ok(PipeFd(fd.0, PhantomData))
    }
}
