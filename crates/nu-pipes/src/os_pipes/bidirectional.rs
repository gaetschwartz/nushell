use std::{
    io::{Cursor, Read, Seek, Write},
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
#[derive(Debug)]
pub enum NXPCEndpointError {
    NotStarted,
    NotOpened,
    AlreadyStarted,
    PipeError(PipeError),
    SendError,
}

impl std::fmt::Display for NXPCEndpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NXPCEndpointError::NotStarted => write!(f, "Endpoint not started"),
            NXPCEndpointError::NotOpened => write!(f, "Endpoint not opened"),
            NXPCEndpointError::AlreadyStarted => write!(f, "Endpoint already started"),
            NXPCEndpointError::PipeError(e) => write!(f, "Pipe error: {}", e),
            NXPCEndpointError::SendError => write!(f, "Send error"),
        }
    }
}

impl From<PipeError> for NXPCEndpointError {
    fn from(e: PipeError) -> Self {
        NXPCEndpointError::PipeError(e)
    }
}

impl<T> From<std::sync::mpsc::SendError<T>> for NXPCEndpointError {
    fn from(_: std::sync::mpsc::SendError<T>) -> Self {
        NXPCEndpointError::SendError
    }
}

impl std::error::Error for NXPCEndpointError {}

struct NXPCServerRole;
struct NXPCClientRole;
pub trait NXPCRole {}
impl NXPCRole for NXPCServerRole {}
impl NXPCRole for NXPCClientRole {}

enum PipeThreadMessage<'a> {
    Serve(Box<dyn MaybeSkipable + Send + 'a>),
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
    Skip(u32, u64), // 0
    Read(u32, u64), // 1
    ReadAll(u32),   // 2
    Close(u32),     // 3
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ServerResponse {
    Ok(u32),  // 0
    Err(u32), // 1
}

impl FixedSizedSerialization<16> for ServerCommand {
    fn serialize_fixed_size(&self) -> Result<[u8; 16], std::io::Error> {
        let cmd_ty: u8;
        let cmd_id: u32;
        let mut vec = Vec::from([0u8; 5]);
        match self {
            ServerCommand::Skip(id, pos) => {
                cmd_ty = 0;
                cmd_id = *id;
                vec.extend_from_slice(&pos.to_be_bytes());
            }
            ServerCommand::Read(id, pos) => {
                cmd_ty = 1;
                cmd_id = *id;
                vec.extend_from_slice(&pos.to_be_bytes());
            }
            ServerCommand::ReadAll(id) => {
                cmd_ty = 2;
                cmd_id = *id;
            }
            ServerCommand::Close(id) => {
                cmd_ty = 3;
                cmd_id = *id;
            }
        }
        vec[0] = cmd_ty;
        vec[1..5].copy_from_slice(&cmd_id.to_be_bytes());
        // pad the rest of the vec with 0s
        vec.resize(16, 0);
        Ok(TryFrom::try_from(vec).unwrap())
    }

    fn deserialize_fixed_size(buf: &[u8; 16]) -> Result<Box<Self>, std::io::Error> {
        let mut cursor = Cursor::new(buf);
        let cmd_ty = cursor.read_u8()?;
        let cmd_id = cursor.read_u32()?;
        match cmd_ty {
            0 => Ok(Box::new(ServerCommand::Skip(cmd_id, cursor.read_u64()?))),
            1 => Ok(Box::new(ServerCommand::Read(cmd_id, cursor.read_u64()?))),
            2 => Ok(Box::new(ServerCommand::ReadAll(cmd_id))),
            3 => Ok(Box::new(ServerCommand::Close(cmd_id))),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid command type",
            )),
        }
    }
}

impl FixedSizedSerialization<5> for ServerResponse {
    fn serialize_fixed_size(&self) -> Result<[u8; 5], std::io::Error> {
        let mut buf = [0u8; 5];
        let resp_ty: u8;
        let resp_id: u32;
        match self {
            ServerResponse::Ok(id) => {
                resp_ty = 0;
                resp_id = *id;
            }
            ServerResponse::Err(id) => {
                resp_ty = 1;
                resp_id = *id;
            }
        }
        buf[0] = resp_ty;
        buf[1..5].copy_from_slice(&resp_id.to_be_bytes());
        Ok(buf)
    }

    fn deserialize_fixed_size(buf: &[u8; 5]) -> Result<Box<Self>, std::io::Error> {
        let mut cursor = Cursor::new(buf);
        let resp_ty = cursor.read_u8()?;
        let resp_id = cursor.read_u32()?;
        match resp_ty {
            0 => Ok(Box::new(ServerResponse::Ok(resp_id))),
            1 => Ok(Box::new(ServerResponse::Err(resp_id))),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid response type",
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

trait MaybeSkipable: Read {
    fn skipable(&self) -> bool {
        false
    }
    fn skip(&mut self, pos: u64) -> Result<u64, std::io::Error> {
        std::io::copy(&mut self.take(pos), &mut std::io::sink())
    }
}

impl<T: Read + Seek> MaybeSkipable for T {
    fn skipable(&self) -> bool {
        true
    }

    fn skip(&mut self, pos: u64) -> Result<u64, std::io::Error> {
        self.seek(std::io::SeekFrom::Current(pos as i64))
    }
}

impl<'scope, T: NXPCRole> NXPCEndpoint<'scope, '_, T> {
    pub fn open(&mut self) -> Result<(), NXPCEndpointError> {
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

impl<'scope, 'a, T: NXPCRole> NXPCEndpoint<'scope, 'a, T> {
    fn assert_opened(&self) -> Result<(&Pipe<PipeRead>, &Pipe<PipeWrite>), NXPCEndpointError> {
        match (&self.read, &self.write) {
            (PipeState::Opened(r), PipeState::Opened(w)) => Ok((r, w)),
            _ => Err(NXPCEndpointError::NotOpened),
        }
    }

    fn assert_started(
        &self,
    ) -> Result<&ThreadData<'scope, 'a, Result<(), std::io::Error>>, NXPCEndpointError> {
        match &self.thread {
            Some(thread) => Ok(thread),
            None => Err(NXPCEndpointError::NotStarted),
        }
    }
}
impl<'scope, 'a> NXPCEndpoint<'scope, 'a, NXPCServerRole> {
    fn start<'env>(&mut self, scope: &'scope Scope<'scope, 'env>) -> Result<(), NXPCEndpointError>
    where
        'a: 'scope,
    {
        self.open()?;
        let (tx, rx) = std::sync::mpsc::channel::<PipeThreadMessage>();
        let (r, w) = self.assert_opened()?;
        let (r, w) = (r.clone(), w.clone());
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
                        let cmd_buf = reader.read_exact_buf()?;
                        let cmd = ServerCommand::deserialize_fixed_size(&cmd_buf)?;
                        match *cmd {
                            // 0: Seek, 1: Read, 2: ReadAll, 3: Close
                            ServerCommand::Skip(id, amt) => {
                                served.skip(amt)?;
                                writer.respond_ok(id)?;
                            }
                            ServerCommand::Read(id, to_read) => {
                                let mut bufreader = std::io::BufReader::new(served.take(to_read));
                                std::io::copy(&mut bufreader, &mut writer)?;
                                writer.respond_ok(id)?;
                            }
                            ServerCommand::ReadAll(id) => {
                                let mut bufreader = std::io::BufReader::new(served);
                                std::io::copy(&mut bufreader, &mut writer)?;
                                writer.respond_ok(id)?;
                            }
                            ServerCommand::Close(id) => {
                                writer.respond_ok(id)?;
                                break;
                            }
                        }
                    }

                    PipeThreadMessage::Close => break,
                };
            }

            loop {
                let msg = match rx.recv() {
                    Ok(msg) => msg,
                    Err(_) => break,
                };
                match msg {
                    PipeThreadMessage::Serve(mut served) => {
                        // wait for the client to send a command
                        let cmd_buf = reader.read_exact_buf()?;
                        let cmd = ServerCommand::deserialize_fixed_size(&cmd_buf)?;
                        match *cmd {
                            // 0: Seek, 1: Read, 2: ReadAll, 3: Close
                            ServerCommand::Skip(_id, amt) => {
                                served.skip(amt)?;
                            }
                            ServerCommand::Read(_id, to_read) => {
                                let mut bufreader = std::io::BufReader::new(served.take(to_read));
                                std::io::copy(&mut bufreader, &mut writer)?;
                            }
                            ServerCommand::ReadAll(_id) => {
                                let mut bufreader = std::io::BufReader::new(served);
                                std::io::copy(&mut bufreader, &mut writer)?;
                            }
                            ServerCommand::Close(_id) => break,
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

    pub fn serve_seekable<'r, R: MaybeSkipable + Send + 'r>(
        &mut self,
        reader: R,
    ) -> Result<(), NXPCEndpointError>
    where
        'r: 'a,
    {
        self.assert_opened()?;
        let thread = self.assert_started()?;

        thread.tx.send(PipeThreadMessage::Serve(Box::new(reader)))?;

        Ok(())
    }
}

impl<'scope, 'a, T: NXPCRole> Seek for NXPCEndpoint<'scope, 'a, T> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let (r, w) = self.assert_opened().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                "Pipe not opened for reading and writing",
            )
        })?;
        // send the command to the server
        let mut writer = PipeWriter::new(w.clone());
        let std::io::SeekFrom::Current(pos) = pos else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid seek position",
            ));
        };
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u32;

        let cmd = ServerCommand::Skip(id, pos as u64);
        let cmd_buf = cmd.serialize_fixed_size()?;
        writer.write_all(&cmd_buf)?;
        writer.flush()?;
        // wait for the server to respond
        let mut reader = PipeReader::new(r.clone());
        let resp_buf = reader.read_exact_buf()?;
        let resp = ServerResponse::deserialize_fixed_size(&resp_buf)?;
        match *resp {
            ServerResponse::Ok(id) => Ok(id as u64),
            ServerResponse::Err(id) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Server error: {}", id),
            )),
        }
    }
}

trait ReadExactBuf<const N: usize>: Read {
    fn read_exact_buf(&mut self) -> Result<[u8; N], std::io::Error>;
}

impl<const N: usize, R: Read> ReadExactBuf<N> for R {
    fn read_exact_buf(&mut self) -> Result<[u8; N], std::io::Error> {
        let mut buf: [u8; N] = [0u8; N];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }
}

trait IntRead: Read {
    fn read_u8(&mut self) -> Result<u8, std::io::Error>;
    fn read_u32(&mut self) -> Result<u32, std::io::Error>;
    fn read_u64(&mut self) -> Result<u64, std::io::Error>;
}

impl<R: Read> IntRead for R {
    fn read_u8(&mut self) -> Result<u8, std::io::Error> {
        let buf: [u8; 1] = self.read_exact_buf()?;
        Ok(buf[0])
    }

    fn read_u32(&mut self) -> Result<u32, std::io::Error> {
        Ok(u32::from_be_bytes(self.read_exact_buf()?))
    }

    fn read_u64(&mut self) -> Result<u64, std::io::Error> {
        Ok(u64::from_be_bytes(self.read_exact_buf()?))
    }
}

trait ServerRespondable: Write {
    fn respond_ok(&mut self, id: u32) -> Result<(), std::io::Error>;
    fn respond_err(&mut self, id: u32) -> Result<(), std::io::Error>;
}

impl<W: Write> ServerRespondable for W {
    fn respond_ok(&mut self, id: u32) -> Result<(), std::io::Error> {
        let resp = ServerResponse::Ok(id);
        let resp_buf = resp.serialize_fixed_size()?;
        self.write_all(&resp_buf)?;
        self.flush()?;
        Ok(())
    }

    fn respond_err(&mut self, id: u32) -> Result<(), std::io::Error> {
        let resp = ServerResponse::Err(id);
        let resp_buf = resp.serialize_fixed_size()?;
        self.write_all(&resp_buf)?;
        self.flush()?;
        Ok(())
    }
}
