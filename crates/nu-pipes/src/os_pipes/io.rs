use std::io::{BufReader, BufWriter, Write};

use konst::{primitive::parse_i32, unwrap_ctx};
use log::trace;

use crate::{
    unidirectional::{Pipe, PipeRead, PipeWrite},
    utils::catch_result,
    Closeable, PipeEncoding,
};

const BUFFER_CAPACITY: usize = 128 * 1024;
const ZSTD_COMPRESSION_LEVEL: i32 = if let Some(n) = option_env!("ZSTD_COMPRESSION_LEVEL") {
    unwrap_ctx!(parse_i32(n))
} else {
    3
};
const ZSTD_ENABLE_MULTITHREAD: bool = false;

/// Represents an unbuffered handle writer. Prefer `BufferedHandleWriter` over this for better performance.
pub struct PipeWriter<'p> {
    pub(crate) pipe: Pipe<PipeWrite>,
    writer: Option<Box<dyn FinishableWrite<Inner = Pipe<PipeWrite>> + 'p>>,
}

impl<'p> PipeWriter<'p> {
    pub fn new(pipe: Pipe<PipeWrite>) -> Self {
        let encoding = pipe.encoding();
        let finishable_write: Box<dyn FinishableWrite<Inner = Pipe<PipeWrite>> + 'p> =
            match encoding {
                PipeEncoding::Zstd => {
                    let encoder = catch_result::<_, std::io::Error, _>(|| {
                        let mut enc =
                            zstd::stream::Encoder::new(pipe.clone(), ZSTD_COMPRESSION_LEVEL)?;
                        if ZSTD_ENABLE_MULTITHREAD {
                            enc.multithread(num_cpus::get() as u32 - 1)?;
                        }
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
                PipeEncoding::None => {
                    Box::new(BufWriter::with_capacity(BUFFER_CAPACITY, pipe.clone()))
                }
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

    pub fn close(&mut self) -> Result<(), std::io::Error> {
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

impl std::io::Write for PipeWriter<'_> {
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

impl<W: FinishableWrite> FinishableWrite for BufWriter<W> {
    fn finish(mut self: Box<Self>) -> Result<(), std::io::Error> {
        self.flush()?;
        Box::new(self.into_inner()?).finish()
    }

    fn set_pledged_src_size(&mut self, size: Option<u64>) -> Result<(), std::io::Error> {
        self.get_mut().set_pledged_src_size(size)
    }

    type Inner = W::Inner;
}

/// A struct representing a handle reader.
pub struct PipeReader {
    pub(crate) reader: Box<dyn std::io::Read + Send>,
    pub pipe: Pipe<PipeRead>,
}

impl Clone for PipeReader {
    fn clone(&self) -> Self {
        PipeReader::new(self.pipe.clone())
    }
}

impl std::fmt::Debug for PipeReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipeReader")
            .field("pipe", &self.pipe)
            .finish()
    }
}

impl PipeReader {
    pub fn new(pipe: Pipe<PipeRead>) -> Self {
        let encoding = pipe.encoding();

        let reader: Box<dyn std::io::Read + Send> = match encoding {
            PipeEncoding::Zstd => {
                let decoder = catch_result::<_, std::io::Error, _>(|| {
                    let dec = zstd::stream::Decoder::new(pipe.clone())?;
                    Ok(dec)
                });
                match decoder {
                    Ok(decoder) => Box::new(decoder),
                    Err(e) => {
                        trace!("failed to create zstd decoder, falling back to raw ({})", e);
                        Box::new(BufReader::with_capacity(BUFFER_CAPACITY, pipe.clone()))
                    }
                }
            }
            PipeEncoding::None => Box::new(BufReader::with_capacity(BUFFER_CAPACITY, pipe.clone())),
        };

        Self { reader, pipe }
    }

    pub fn close(&mut self) -> Result<(), std::io::Error> {
        self.pipe.close().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to close handle: {:?}", e),
            )
        })
    }

    pub fn into_pipe(self) -> Pipe<PipeRead> {
        self.pipe
    }
}

impl std::io::Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

impl Closeable for PipeReader {
    fn close(&mut self) -> Result<(), std::io::Error> {
        Self::close(self)
    }
}

impl Closeable for PipeWriter<'_> {
    fn close(&mut self) -> Result<(), std::io::Error> {
        Self::close(self)
    }
}

unsafe impl Sync for PipeReader {}
