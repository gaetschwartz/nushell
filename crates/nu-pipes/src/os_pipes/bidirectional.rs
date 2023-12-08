use std::{
    io::{Read, Seek, Write},
    sync::mpsc::Sender,
    thread::{Scope, ScopedJoinHandle},
};

use serde::{Deserialize, Serialize};

use crate::{
    errors::PipeError,
    io::PipeWriter,
    unidirectional::{
        HandleType, Pipe, PipeMode, PipeRead, PipeWrite, UnOpenedPipe, UnidirectionalPipe,
        UnidirectionalPipeOptions,
    },
    PipeEncoding, PipeReader,
};

/// NXPC (Nu Cross Process Communication)

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
enum PipeState<T: HandleType> {
    UnOpened(UnOpenedPipe<T>),
    Opened(Pipe<T>),
}

struct NXPCServerRole;
struct NXPCClientRole;
pub trait NXPCRole {}
impl NXPCRole for NXPCServerRole {}
impl NXPCRole for NXPCClientRole {}

enum PipeThreadMessage<'a> {
    Serve(Box<dyn MaybeSeekable + Send + 'a>),
    Close,
}

#[derive(Debug)]
struct ThreadData<'scope, 'a: 'scope, T> {
    handle: ScopedJoinHandle<'scope, T>,
    tx: Sender<PipeThreadMessage<'a>>,
}

trait FixedSizedSerialization<const N: usize> {
    fn serialize_fixed_size(&self) -> Result<[u8; N], std::io::Error>;

    fn deserialize_fixed_size(buf: &[u8; N]) -> Result<Box<Self>, std::io::Error>;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
/// Fixed size of 128 bytes
enum ServerCommand {
    Skip(u64), // 0
    Read(u64), // 1
    ReadAll,   // 2
    Close,     // 3
}

impl FixedSizedSerialization<128> for ServerCommand {
    fn serialize_fixed_size(&self) -> Result<[u8; 128], std::io::Error> {
        let mut buf = [0u8; 128];
        match self {
            ServerCommand::Skip(pos) => {
                buf[0] = 0;
                buf[1..9].copy_from_slice(&pos.to_be_bytes());
            }
            ServerCommand::Read(pos) => {
                buf[0] = 1;
                buf[1..9].copy_from_slice(&pos.to_be_bytes());
            }
            ServerCommand::ReadAll => {
                buf[0] = 2;
            }
            ServerCommand::Close => {
                buf[0] = 3;
            }
        }
        Ok(buf)
    }

    fn deserialize_fixed_size(buf: &[u8; 128]) -> Result<Box<Self>, std::io::Error> {
        match buf[0] {
            0 => {
                let mut pos = [0u8; 8];
                pos.copy_from_slice(&buf[1..9]);
                Ok(Box::new(ServerCommand::Skip(u64::from_be_bytes(pos))))
            }
            1 => {
                let mut pos = [0u8; 8];
                pos.copy_from_slice(&buf[1..9]);
                Ok(Box::new(ServerCommand::Read(u64::from_be_bytes(pos))))
            }
            2 => Ok(Box::new(ServerCommand::ReadAll)),
            3 => Ok(Box::new(ServerCommand::Close)),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid command",
            )),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NXPCEndpoint<'scope, 'a: 'scope, T: NXPCRole> {
    read: PipeState<PipeRead>,
    write: PipeState<PipeWrite>,
    marker: std::marker::PhantomData<&'a T>,
    #[serde(skip, default)]
    thread: Option<ThreadData<'scope, 'a, Result<(), std::io::Error>>>,
}

pub struct BidirectionalPipe<'scope, 'a> {
    server: NXPCEndpoint<'scope, 'a, NXPCServerRole>,
    client: NXPCEndpoint<'scope, 'a, NXPCClientRole>,
}

impl<'scope, 'a> BidirectionalPipe<'scope, 'a> {
    pub fn create() -> Result<Self, PipeError> {
        let opt = UnidirectionalPipeOptions {
            encoding: PipeEncoding::None,
            mode: PipeMode::CrossProcess,
        };
        let (r1, w1) = UnidirectionalPipe::create_from_options(opt)?.tuple();
        let (r2, w2) = UnidirectionalPipe::create_from_options(opt)?.tuple();

        Ok(BidirectionalPipe {
            server: NXPCEndpoint {
                write: PipeState::UnOpened(w1),
                read: PipeState::UnOpened(r2),
                marker: std::marker::PhantomData,
                thread: None,
            },
            client: NXPCEndpoint {
                write: PipeState::UnOpened(w2),
                read: PipeState::UnOpened(r1),
                marker: std::marker::PhantomData,
                thread: None,
            },
        })
    }
}

trait MaybeSeekable: Read {
    fn seekable(&self) -> bool {
        false
    }
    fn skip(&mut self, pos: u64) -> Result<u64, std::io::Error> {
        std::io::copy(&mut self.take(pos), &mut std::io::sink())
    }
}

impl<T: Read + Seek> MaybeSeekable for T {
    fn seekable(&self) -> bool {
        true
    }

    fn skip(&mut self, pos: u64) -> Result<u64, std::io::Error> {
        self.seek(std::io::SeekFrom::Current(pos as i64))
    }
}

impl<'scope, T: NXPCRole> NXPCEndpoint<'scope, '_, T> {
    pub fn open(&mut self) -> Result<(), PipeError> {
        match &self.read {
            PipeState::UnOpened(p) => {
                let r = p.open_raw()?;
                self.read = PipeState::Opened(r);
            }
            PipeState::Opened(_) => {}
        }
        match &self.write {
            PipeState::UnOpened(p) => {
                let w = p.open_raw()?;
                self.write = PipeState::Opened(w);
            }
            PipeState::Opened(_) => {}
        }
        Ok(())
    }
}

impl<'scope, 'a> NXPCEndpoint<'scope, 'a, NXPCServerRole> {
    fn start<'env>(&mut self, scope: &'scope Scope<'scope, 'env>) -> Result<(), PipeError>
    where
        'a: 'scope,
    {
        self.open()?;
        let (tx, rx) = std::sync::mpsc::channel::<PipeThreadMessage>();
        let PipeState::Opened(w) = &self.write else {
            unreachable!()
        };
        let PipeState::Opened(r) = &self.read else {
            unreachable!()
        };
        let (w, r) = (w.clone(), r.clone());
        let handle: ScopedJoinHandle<'scope, Result<(), std::io::Error>> = scope.spawn(move || {
            let mut writer = PipeWriter::new(w);
            let mut reader = PipeReader::new(r);
            let rx = rx;
            loop {
                let msg = match rx.recv() {
                    Ok(msg) => msg,
                    Err(_) => break,
                };
                match msg {
                    PipeThreadMessage::Serve(mut served) => {
                        // wait for the client to send a command
                        let cmd_buf: [u8; 128] = reader.read_exact_buf()?;
                        let cmd = ServerCommand::deserialize_fixed_size(&cmd_buf)?;
                        match *cmd {
                            // 0: Seek, 1: Read, 2: ReadAll, 3: Close
                            ServerCommand::Skip(amt) => {
                                served.skip(amt)?;
                            }
                            ServerCommand::Read(to_read) => {
                                let mut bufreader = std::io::BufReader::new(served.take(to_read));
                                std::io::copy(&mut bufreader, &mut writer)?;
                            }
                            ServerCommand::ReadAll => {
                                let mut bufreader = std::io::BufReader::new(served);
                                std::io::copy(&mut bufreader, &mut writer)?;
                            }
                            ServerCommand::Close => break,
                        }
                    }

                    PipeThreadMessage::Close => break,
                };
            }

            Ok(())
        });
        self.thread = Some(ThreadData { handle, tx });
        Ok(())
    }
}

impl<'sscope, 'sa> NXPCEndpoint<'sscope, 'sa, NXPCServerRole> {
    pub fn serve_seekable<'r, R: MaybeSeekable + Send + 'r>(
        &mut self,
        reader: R,
    ) -> Result<(), std::io::Error>
    where
        'r: 'sa,
    {
        self.open()?;
        if let Some(thread) = &self.thread {
            thread
                .tx
                .send(PipeThreadMessage::Serve(Box::new(reader)))
                .map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "Failed to send serve message",
                    )
                })?;
        } else {
            unreachable!()
        }
        Ok(())
    }
}

trait ReadExactBuf<const N: usize>: Read {
    fn read_exact_buf(&mut self) -> Result<[u8; N], std::io::Error>;
}

impl<const N: usize, R: Read> ReadExactBuf<N> for R {
    fn read_exact_buf(&mut self) -> Result<[u8; N], std::io::Error> {
        let mut buf = [0u8; N];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }
}
