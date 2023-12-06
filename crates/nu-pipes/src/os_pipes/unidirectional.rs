use nu_protocol::StreamDataType;
use serde::{Deserialize, Serialize};

use crate::{
    os_pipes::{pipe_impl, PipeImplBase},
    Handle, HandleTypeEnum, PipeError, StreamEncoding,
};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PipeDirection {
    UsServerThemClient,
    UsClientThemServer,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct UnOpenedPipe<T: HandleType> {
    pub datatype: StreamDataType,
    pub encoding: StreamEncoding,

    handle: Handle,
    other_handle: Handle,
    pub mode: PipeMode,
    ty: T,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Pipe<T: HandleType> {
    pub(crate) datatype: StreamDataType,
    pub(crate) encoding: StreamEncoding,

    pub(crate) handle: Handle,
    pub(crate) mode: PipeMode,
    marker: std::marker::PhantomData<T>,
}

pub struct UnidirectionalPipe {
    pub read: UnOpenedPipe<PipeRead>,
    pub write: UnOpenedPipe<PipeWrite>,
}

impl UnidirectionalPipe {
    /// Creates a new pipe. Pipes are unidirectional streams of bytes composed of a read end and a write end. They can be used for interprocess communication.
    /// Uses `pipe(2)` on unix and `CreatePipe` on windows.
    pub fn create_default() -> Result<Self, PipeError> {
        Self::create_from_options(UniDirectionalPipeOptions::default())
    }

    pub fn create_from_options(
        arg: UniDirectionalPipeOptions,
    ) -> Result<UnidirectionalPipe, PipeError> {
        let pipe = pipe_impl::PipeImpl::create_pipe()?;
        assert!(pipe.write_handle.1 == HandleTypeEnum::Write);
        assert!(pipe.read_handle.1 == HandleTypeEnum::Read);

        let rp = UnOpenedPipe {
            datatype: StreamDataType::Binary,
            encoding: arg.encoding,
            handle: pipe.read_handle,
            other_handle: pipe.write_handle,
            mode: arg.mode,
            ty: PipeRead(std::marker::PhantomData),
        };
        let wp = UnOpenedPipe {
            datatype: StreamDataType::Binary,
            encoding: arg.encoding,
            handle: pipe.write_handle,
            other_handle: pipe.read_handle,
            mode: arg.mode,
            ty: PipeWrite(std::marker::PhantomData),
        };
        Ok(UnidirectionalPipe {
            read: rp,
            write: wp,
        })
    }
}

impl<T: HandleType> UnOpenedPipe<T> {
    pub fn open(&self) -> Result<Pipe<T>, PipeError> {
        if pipe_impl::PipeImpl::should_close_other_for_mode(self.mode) {
            // close both their ends of the pipe in our process
            pipe_impl::PipeImpl::close_handle(&self.other_handle)?;
        }
        Ok(Pipe {
            datatype: self.datatype,
            encoding: self.encoding,
            handle: self.handle,
            mode: self.mode,
            marker: std::marker::PhantomData,
        })
    }
}

impl<T: HandleType> Pipe<T> {
    pub fn close(&self) -> Result<(), PipeError> {
        pipe_impl::PipeImpl::close_handle(&self.handle)
    }

    pub fn encoding(&self) -> StreamEncoding {
        self.encoding
    }

    pub fn mode(&self) -> PipeMode {
        self.mode
    }
}

impl std::io::Read for Pipe<PipeRead> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::read_handle(&self.handle, buf)?)
    }
}
impl std::io::Write for Pipe<PipeWrite> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::write_handle(&self.handle, buf)?)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl std::io::Read for &Pipe<PipeRead> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::read_handle(&self.handle, buf)?)
    }
}
impl std::io::Write for &Pipe<PipeWrite> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(pipe_impl::PipeImpl::write_handle(&self.handle, buf)?)
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

pub struct UniDirectionalPipeOptions {
    pub encoding: StreamEncoding,
    pub mode: PipeMode,
}

impl Default for UniDirectionalPipeOptions {
    fn default() -> Self {
        Self {
            encoding: StreamEncoding::None,
            mode: PipeMode::CrossProcess,
        }
    }
}
