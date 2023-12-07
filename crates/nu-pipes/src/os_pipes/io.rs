use std::borrow::Borrow;

use log::trace;

use crate::{
    unidirectional::{Pipe, PipeRead, PipeWrite},
    utils::catch_result,
    Closeable, StreamEncoding,
};

const BUFFER_CAPACITY: usize = 16 * 1024 * 1024;
const ZSTD_COMPRESSION_LEVEL: i32 = 0;

/// Represents an unbuffered handle writer. Prefer `BufferedHandleWriter` over this for better performance.
pub struct HandleWriter<'p> {
    pub(crate) pipe: Pipe<PipeWrite>,
    writer: Option<Box<dyn FinishableWrite<Inner = Pipe<PipeWrite>> + 'p>>,
}

impl<'p> HandleWriter<'p> {
    pub fn new(pipe: Pipe<PipeWrite>) -> Self {
        let encoding = pipe.encoding();
        let finishable_write: Box<dyn FinishableWrite<Inner = Pipe<PipeWrite>> + 'p> =
            match encoding {
                StreamEncoding::Zstd => {
                    let encoder: Result<zstd::Encoder<'_, Pipe<PipeWrite>>, std::io::Error> =
                        catch_result(|| {
                            let mut enc =
                                zstd::stream::Encoder::new(pipe.clone(), ZSTD_COMPRESSION_LEVEL)?;
                            enc.multithread(1)?;
                            Ok(enc)
                        });
                    match encoder {
                        Ok(encoder) => Box::new(encoder),
                        Err(e) => {
                            trace!("failed to create zstd encoder, falling back to raw ({})", e);
                            Box::new(pipe.clone())
                        }
                    }
                }
                StreamEncoding::None => Box::new(pipe.clone()),
            };
        Self {
            pipe,
            writer: Some(finishable_write),
        }
    }

    pub fn set_pledged_src_size(&mut self, size: Option<u64>) -> Result<(), std::io::Error> {
        self.writer.as_mut().map_or(
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "writer is already closed",
            )),
            |w| w.set_pledged_src_size(size),
        )
    }
}

impl std::io::Write for HandleWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writer.as_mut().map_or(
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "writer is already closed",
            )),
            |w| w.write(buf),
        )
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.as_mut().map_or(
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "writer is already closed",
            )),
            |w| w.flush(),
        )
    }
}

trait FinishableWrite: std::io::Write {
    type Inner: Sized;

    fn finish(self: Box<Self>) -> Result<(), std::io::Error>;
    fn set_pledged_src_size(&mut self, _size: Option<u64>) -> Result<(), std::io::Error> {
        Ok(())
    }
}

impl FinishableWrite for zstd::stream::Encoder<'_, Pipe<PipeWrite>> {
    fn finish(self: Box<Self>) -> Result<(), std::io::Error> {
        zstd::stream::Encoder::finish(*self)?;
        Ok(())
    }

    fn set_pledged_src_size(&mut self, size: Option<u64>) -> Result<(), std::io::Error> {
        zstd::stream::Encoder::set_pledged_src_size(self, size)
    }

    type Inner = Pipe<PipeWrite>;
}

impl FinishableWrite for Pipe<PipeWrite> {
    type Inner = Pipe<PipeWrite>;

    #[inline(always)]
    fn finish(self: Box<Self>) -> Result<(), std::io::Error> {
        Ok(())
    }
}

/// A struct representing a handle reader.
pub struct HandleReader<'p> {
    pub(crate) reader: Box<dyn std::io::Read + 'p>,
    pub(crate) pipe: Pipe<PipeRead>,
}

impl<'p> HandleReader<'p> {
    pub fn new(pipe: Pipe<PipeRead>) -> Self {
        let encoding = pipe.encoding();

        let reader: Box<dyn std::io::Read> = match encoding {
            StreamEncoding::Zstd => {
                let decoder = zstd::stream::Decoder::new(pipe.clone());
                match decoder {
                    Ok(decoder) => Box::new(decoder),
                    Err(e) => {
                        trace!("failed to create zstd decoder, falling back to raw ({})", e);
                        Box::new(std::io::BufReader::with_capacity(
                            BUFFER_CAPACITY,
                            pipe.clone(),
                        ))
                    }
                }
            }
            StreamEncoding::None => Box::new(std::io::BufReader::with_capacity(
                BUFFER_CAPACITY,
                pipe.clone(),
            )),
        };

        Self { reader, pipe }
    }

    fn pipe(&self) -> &Pipe<PipeRead> {
        self.pipe.borrow()
    }
}

impl std::io::Read for HandleReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

impl Closeable for HandleWriter<'_> {
    fn close(&mut self) -> Result<(), std::io::Error> {
        let writer = self.writer.take();
        match writer {
            Some(writer) => {
                writer.finish()?;
            }
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "failed to close handle: writer is already closed",
                ))
            }
        }

        self.pipe.close().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to close handle: {:?}", e),
            )
        })
    }
}

impl Closeable for HandleReader<'_> {
    fn close(&mut self) -> Result<(), std::io::Error> {
        self.pipe().close().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to close handle: {:?}", e),
            )
        })
    }
}
