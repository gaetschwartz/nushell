use nu_protocol::StreamDataType;
use serde::{Deserialize, Serialize};

use crate::{pipe_impl, Handle, HandleTypeEnum, PipeError, StreamEncoding};

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
        let pipe = pipe_impl::create_pipe()?;
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
        if pipe_impl::should_close_other_for_mode(self.mode) {
            // close both their ends of the pipe in our process
            pipe_impl::close_handle(self.other_handle)?;
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
        pipe_impl::close_handle(self.handle)
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
        pipe_impl::read_handle(&self.handle, buf)
    }
}
impl std::io::Write for Pipe<PipeWrite> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        pipe_impl::write_handle(&self.handle, buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl std::io::Read for &Pipe<PipeRead> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        pipe_impl::read_handle(&self.handle, buf)
    }
}
impl std::io::Write for &Pipe<PipeWrite> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        pipe_impl::write_handle(&self.handle, buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PipeRead(std::marker::PhantomData<()>);
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PipeWrite(std::marker::PhantomData<()>);

pub trait HandleType {}

impl HandleType for PipeRead {}
impl HandleType for PipeWrite {}

impl Serialize for PipeRead {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str("PipeRead")
    }
}
impl Serialize for PipeWrite {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str("PipeWrite")
    }
}

impl<'de> Deserialize<'de> for PipeRead {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct PipeReadVisitor;
        impl serde::de::Visitor<'_> for PipeReadVisitor {
            type Value = PipeRead;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("PipeRead")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<PipeRead, E> {
                if value == "PipeRead" {
                    Ok(PipeRead(std::marker::PhantomData))
                } else {
                    Err(E::custom(format!("expected PipeRead, got {}", value)))
                }
            }
        }
        d.deserialize_str(PipeReadVisitor)
    }
}

impl<'de> Deserialize<'de> for PipeWrite {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct PipeWriteVisitor;
        impl serde::de::Visitor<'_> for PipeWriteVisitor {
            type Value = PipeWrite;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("PipeWrite")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<PipeWrite, E> {
                if value == "PipeWrite" {
                    Ok(PipeWrite(std::marker::PhantomData))
                } else {
                    Err(E::custom(format!("expected PipeWrite, got {}", value)))
                }
            }
        }
        d.deserialize_str(PipeWriteVisitor)
    }
}

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
