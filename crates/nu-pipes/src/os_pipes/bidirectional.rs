use std::{
    io::{Read, Seek, Write},
    sync::mpsc::Sender,
    thread::{JoinHandle, Scope, ScopedJoinHandle},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NXPCError {}
impl std::fmt::Display for NXPCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

impl std::error::Error for NXPCError {}

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

    fn deserialize_fixed_size(buf: &[u8]) -> Result<Box<Self>, std::io::Error>;
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

    fn deserialize_fixed_size(buf: &[u8]) -> Result<Box<Self>, std::io::Error> {
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

                        let mut cmd_buf = [0u8; 128];
                        let mut cmd_read = 0;
                        loop {
                            let r = reader.read(&mut cmd_buf[cmd_read..])?;
                            if r == 0 {
                                break;
                            }
                            cmd_read += r;
                            if cmd_read == 128 {
                                break;
                            }
                        }
                        let cmd = ServerCommand::deserialize_fixed_size(&cmd_buf[..cmd_read])
                            .unwrap_or_else(|_| {
                                panic!("Failed to deserialize command: {:?}", &cmd_buf[..cmd_read])
                            });
                        match *cmd {
                            // 0: Seek, 1: Read, 2: ReadAll, 3: Close
                            ServerCommand::Skip(amt) => {
                                served.skip(amt)?;
                            }
                            ServerCommand::Read(to_read) => {
                                let mut buf = [0u8; 4096];

                                let mut read = 0;
                                loop {
                                    let r = served.read(&mut buf[read..])?;
                                    if r == 0 {
                                        break;
                                    }
                                    read += r;
                                    if read == to_read as usize {
                                        break;
                                    }
                                    let written = writer.write(&buf[..read])?;
                                    if written != read {
                                        panic!("Failed to write all bytes");
                                    }
                                }
                            }
                            ServerCommand::ReadAll => {
                                let mut buf = [0u8; 4096];

                                loop {
                                    let r = served.read(&mut buf[..])?;
                                    if r == 0 {
                                        break;
                                    }
                                    let written = writer.write(&buf[..r])?;
                                    if written != r {
                                        panic!("Failed to write all bytes");
                                    }
                                }
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
