use std::{
    io::{Cursor, Read, Seek, Write},
    sync::{
        mpsc::{Receiver, Sender},
        Mutex,
    },
    thread::{Scope, ScopedJoinHandle},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    errors::PipeError,
    io::PipeWriter,
    unidirectional::{
        HandleType, Pipe, PipeMode, PipeRead, PipeWrite, UnOpenedPipe, UnidirectionalPipe,
        UnidirectionalPipeOptions,
    },
    PipeEncoding, PipeReader,
};

mod misc;
use misc::*;

/// NXPC (Nu Cross Process Communication)

struct NXPCServerRole;
struct NXPCClientRole;
pub trait NXPCRole {}
impl NXPCRole for NXPCServerRole {}
impl NXPCRole for NXPCClientRole {}

#[derive(Debug)]
struct ThreadData<'scope, M: Send> {
    handle: ScopedJoinHandle<'scope, Result<(), NXPCError>>,
    marker: std::marker::PhantomData<M>,
}

pub trait NxpcMessage: Serialize + DeserializeOwned + Send {}
impl<T: Serialize + DeserializeOwned + Send> NxpcMessage for T {}

#[derive(Serialize, Deserialize, Debug)]
struct ChunkHeader {
    magic: u32,
    len: u64,
}

pub struct NXPCEndpoint<'scope, 'a: 'scope, R: NXPCRole, M: NxpcMessage, P: NxpcMessage> {
    reader: Option<Box<dyn Read + Send + 'a>>,
    writer: Option<Box<dyn Write + Send + 'a>>,
    marker: std::marker::PhantomData<&'a (R, P)>,
    thread: Option<ThreadData<'scope, M>>,
}

pub struct BidirectionalPipe {
    server: UnopenedBiPipe<NXPCServerRole>,
    client: UnopenedBiPipe<NXPCClientRole>,
}

impl BidirectionalPipe {
    pub fn create() -> Result<Self, PipeError> {
        let opt = UnidirectionalPipeOptions {
            encoding: PipeEncoding::None,
            mode: PipeMode::CrossProcess,
        };
        let (r1, w1) = UnidirectionalPipe::create_from_options(opt)?.tuple();
        let (r2, w2) = UnidirectionalPipe::create_from_options(opt)?.tuple();

        Ok(BidirectionalPipe {
            server: UnopenedBiPipe {
                read: r1,
                write: w2,
                marker: std::marker::PhantomData,
            },
            client: UnopenedBiPipe {
                read: r2,
                write: w1,
                marker: std::marker::PhantomData,
            },
        })
    }
}

pub struct UnopenedBiPipe<R: NXPCRole> {
    read: UnOpenedPipe<PipeRead>,
    write: UnOpenedPipe<PipeWrite>,
    marker: std::marker::PhantomData<R>,
}

impl<'scope, R: NXPCRole> UnopenedBiPipe<R> {
    pub fn open<M: NxpcMessage, T: NxpcMessage>(&self) -> Result<NXPCEndpoint<R, M, T>, NXPCError> {
        let (r, w) = (self.read.open()?, self.write.open()?);
        Ok(NXPCEndpoint {
            reader: Some(Box::new(r)),
            writer: Some(Box::new(w)),
            marker: std::marker::PhantomData,
            thread: None,
        })
    }
}

impl<'scope, 'a, M: NxpcMessage + 'scope, T: NxpcMessage>
    NXPCEndpoint<'scope, 'a, NXPCServerRole, M, T>
{
    fn start<'env, E>(
        &mut self,
        scope: &'scope Scope<'scope, 'env>,
        on_data: impl Fn(T) -> Result<ThreadResult<M>, E> + Send + 'scope,
    ) -> Result<(), NXPCError>
    where
        'a: 'scope,
        E: Into<NXPCRuntimeError> + std::error::Error + Send,
    {
        let reader = self.reader.take().unwrap();
        let writer = self.writer.take().unwrap();

        let handle = scope.spawn(move || {
            let mut writer = writer;
            let mut reader = reader;

            loop {
                let msg: T = reader.read_msg()?;
                let result: Result<M, NXPCRuntimeError> = match on_data(msg) {
                    Ok(ThreadResult::Ok(res)) => Ok(res),
                    Err(e) => Err(Into::<NXPCRuntimeError>::into(e)),
                    Ok(ThreadResult::Close) => break,
                };
                rmp_serde::encode::write(&mut writer, &result)?;
                writer.flush()?;
            }

            Ok::<(), NXPCError>(())
        });
        self.thread = Some(ThreadData {
            handle,
            marker: std::marker::PhantomData,
        });
        Ok(())
    }

    fn close(&mut self) -> Result<(), NXPCError> {
        let thread = self.thread.take().unwrap();
        thread.handle.join().unwrap()?;
        Ok(())
    }
}

impl<'scope, 'a, M: NxpcMessage + 'scope, T: NxpcMessage>
    NXPCEndpoint<'scope, 'a, NXPCClientRole, M, T>
{
    fn request(&mut self, message: &M) -> Result<T, NXPCError> {
        let (Some(reader), Some(writer)) = (self.reader.as_mut(), self.writer.as_mut()) else {
            return Err(NXPCError::AlreadyStarted);
        };

        rmp_serde::encode::write(writer, message)?;
        writer.flush()?;

        let res: Result<T, NXPCRuntimeError> = rmp_serde::decode::from_read(reader)?;
        res.map_err(|e| e.into())
    }
    fn send(&mut self, message: &M) -> Result<(), NXPCError> {
        let Some(writer) = self.writer.as_mut() else {
            return Err(NXPCError::AlreadyStarted);
        };

        rmp_serde::encode::write(writer, message)?;
        writer.flush()?;

        Ok(())
    }

    fn recv(&mut self) -> Result<T, NXPCError> {
        let Some(reader) = self.reader.as_mut() else {
            return Err(NXPCError::AlreadyStarted);
        };

        let res: Result<T, NXPCRuntimeError> = rmp_serde::decode::from_read(reader)?;
        res.map_err(|e| e.into())
    }
}

impl<'scope, 'a, R: NXPCRole, M: NxpcMessage, T: NxpcMessage> NXPCEndpoint<'scope, 'a, R, M, T> {
    fn new(reader: Box<dyn Read + Send + 'a>, writer: Box<dyn Write + Send + 'a>) -> Self {
        NXPCEndpoint {
            reader: Some(reader),
            writer: Some(writer),
            marker: std::marker::PhantomData,
            thread: None,
        }
    }
}

pub enum ThreadResult<T> {
    Close,
    Ok(T),
}

trait ReadRmp: Read {
    fn read_msg<T: DeserializeOwned>(&mut self) -> Result<T, rmp_serde::decode::Error> {
        rmp_serde::from_read(self)
    }
}

impl<R: Read> ReadRmp for R {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufReader, Read},
        sync::atomic::AtomicUsize,
    };

    #[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
    enum ClientMsg {
        Greet(String),
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
    enum ServerMsg {
        Msg(String),
    }

    #[test]
    fn test_sent() {
        let mut cli_msg: Cursor<Vec<u8>> = Cursor::new(Vec::new());
        let srv_msg: Cursor<Vec<u8>> = Cursor::new(Vec::new());

        let res1 = {
            let mut client = NXPCEndpoint::<NXPCClientRole, ClientMsg, ServerMsg>::new(
                Box::new(srv_msg),
                Box::new(&mut cli_msg),
            );
            client.send(&ClientMsg::Greet("uwu ?".to_string()))
        };
        assert!(res1.is_ok());
        cli_msg.seek(std::io::SeekFrom::Start(0)).unwrap();

        let sent: ClientMsg = rmp_serde::decode::from_read(&mut cli_msg).unwrap();
        assert_eq!(sent, ClientMsg::Greet("uwu ?".to_string()));
    }

    #[test]
    fn test_recv() {
        let mut cli_msg: Cursor<Vec<u8>> = Cursor::new(Vec::new());
        let mut srv_msg: Cursor<Vec<u8>> = Cursor::new(Vec::new());

        rmp_serde::encode::write(
            &mut srv_msg,
            &Ok::<ServerMsg, NXPCRuntimeError>(ServerMsg::Msg("owo !".to_string())),
        )
        .unwrap();
        rmp_serde::encode::write(
            &mut srv_msg,
            &Err::<ServerMsg, NXPCRuntimeError>(NXPCRuntimeError {
                message: "This is bad".to_string(),
            }),
        )
        .unwrap();
        srv_msg.seek(std::io::SeekFrom::Start(0)).unwrap();

        let mut client = NXPCEndpoint::<NXPCClientRole, ClientMsg, ServerMsg>::new(
            Box::new(srv_msg),
            Box::new(&mut cli_msg),
        );
        let recv = client.recv();
        assert_eq!(
            recv.as_ref().unwrap(),
            &ServerMsg::Msg("owo !".to_string()),
            "Unexpected result: {:?}",
            recv
        );

        let recv = client.recv();
        match recv {
            Ok(_) => panic!("Should have failed"),
            Err(NXPCError::Runtime(NXPCRuntimeError { message })) => {
                assert_eq!(message, "This is bad".to_string())
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_start() {
        let cli_msg = FakePipe::new("\x1b[31mclient\x1b[0m");
        let srv_msg = FakePipe::new("\x1b[32mserver\x1b[0m");
        std::thread::scope(|scope| {
            let mut client = NXPCEndpoint::<NXPCClientRole, ClientMsg, ServerMsg>::new(
                Box::new(BufReader::new(&srv_msg)),
                Box::new(&cli_msg),
            );
            let mut server = NXPCEndpoint::<NXPCServerRole, ServerMsg, ClientMsg>::new(
                Box::new(BufReader::new(&cli_msg)),
                Box::new(&srv_msg),
            );
            server
                .start::<NXPCRuntimeError>(scope, |msg| {
                    eprintln!("Server received: {:?}", msg);
                    let resp = match msg {
                        ClientMsg::Greet(greet) => ServerMsg::Msg(greet),
                    };
                    eprintln!("Server will respond: {:?}", resp);
                    Ok(ThreadResult::Ok(resp))
                })
                .unwrap();

            let res = client.request(&ClientMsg::Greet("uwu ?".to_string()));
            assert_eq!(res.unwrap(), ServerMsg::Msg("uwu ?".to_string()));

            server.close().unwrap();
        });
    }

    struct FakePipe {
        buffer: Mutex<Vec<u8>>,
        condvar: std::sync::Condvar,
        name: &'static str,
    }

    impl FakePipe {
        fn new(name: &'static str) -> Self {
            FakePipe {
                buffer: Mutex::new(Vec::new()),
                condvar: std::sync::Condvar::new(),
                name,
            }
        }
    }

    impl Read for &FakePipe {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            eprintln!("{}: read", self.name);

            let mut lock = self.buffer.lock().unwrap();
            loop {
                let unread = lock.len();
                eprintln!("{}: read: unread data is {}", self.name, unread);
                if unread > 0 {
                    let min = unread.min(buf.len());
                    let read = lock.drain(..min).collect::<Vec<u8>>();
                    buf[..min].copy_from_slice(&read);
                    eprintln!("{}: read: read {} bytes", self.name, min);
                    return Ok(min);
                } else {
                    eprintln!("{}: read: waiting", self.name);
                    lock = self.condvar.wait(lock).unwrap();
                }
            }
        }
    }

    impl Write for &FakePipe {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            eprintln!("{}: write", self.name);
            let mut cursor = self.buffer.lock().unwrap();
            cursor.extend_from_slice(buf);
            self.condvar.notify_one();
            eprintln!("{}: write: wrote {} bytes", self.name, buf.len());
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
