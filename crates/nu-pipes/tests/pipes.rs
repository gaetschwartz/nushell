use std::{
    io::{Read, Write},
    process::Command,
};

use nu_pipes::{
    unidirectional::{
        PipeMode, PipeRead, UnOpenedPipe, UniDirectionalPipeOptions, UnidirectionalPipe,
    },
    utils, PipeEncoding,
};

trait TestPipeExt {
    fn in_process() -> UnidirectionalPipe;
}

impl TestPipeExt for UnidirectionalPipe {
    fn in_process() -> Self {
        Self::create_from_options(UniDirectionalPipeOptions {
            encoding: PipeEncoding::None,
            mode: PipeMode::InProcess,
        })
        .unwrap()
    }
}

trait ReadAsString {
    fn read_as_string(&mut self) -> Result<String, std::io::Error>;
}

impl<R: Read> ReadAsString for R {
    fn read_as_string(&mut self) -> Result<String, std::io::Error> {
        let mut buf = String::new();
        self.read_to_string(&mut buf).map(|_| buf)
    }
}

fn as_string(r: Option<impl Read>) -> String {
    r.map(|mut s| s.read_as_string().unwrap())
        .unwrap_or("None".to_string())
}
#[test]
fn test_pipe() {
    let UnidirectionalPipe { read, write } = UnidirectionalPipe::in_process();
    let mut reader = read.open().unwrap();
    let mut writer = write.open().unwrap();
    // write hello world to the pipe
    let written = writer.write("hello world".as_bytes()).unwrap();
    writer.close().unwrap();

    assert_eq!(written, 11);

    let mut buf = [0u8; 256];

    let read = reader.read(&mut buf).unwrap();
    reader.close().unwrap();

    assert_eq!(read, 11);
    assert_eq!(&buf[..read], "hello world".as_bytes());
}

#[test]
fn test_serialized_pipe() {
    let UnidirectionalPipe { read, write } = UnidirectionalPipe::in_process();
    let mut writer = write.open().unwrap();
    // write hello world to the pipe
    let written = writer.write("hello world".as_bytes()).unwrap();

    assert_eq!(written, 11);

    writer.close().unwrap();

    // serialize the pipe
    let serialized = serde_json::to_string(&read).unwrap();
    println!("{}", serialized);
    // deserialize the pipe
    let deserialized: UnOpenedPipe<PipeRead> = serde_json::from_str(&serialized).unwrap();
    let mut reader = deserialized.open().unwrap();

    let mut buf = [0u8; 11];

    let read = reader.read(&mut buf).unwrap();

    assert_eq!(read, 11);
    assert_eq!(buf, "hello world".as_bytes());
    reader.close().unwrap();
}

#[test]
fn pipe_in_another_thread() {
    let UnidirectionalPipe { read, write } = UnidirectionalPipe::in_process();
    let mut writer = write.open().unwrap();
    // write hello world to the pipe
    let written = writer.write("hello world".as_bytes()).unwrap();

    assert_eq!(written, 11);
    writer.close().unwrap();

    // serialize the pipe
    let serialized = serde_json::to_string(&read).unwrap();
    // spawn a new process
    let (read, buf) = utils::named_thread("thread@pipe_in_another_thread", move || {
        // deserialize the pipe
        let deserialized: UnOpenedPipe<PipeRead> = serde_json::from_str(&serialized).unwrap();
        let mut reader = deserialized.open().unwrap();

        let mut buf = [0u8; 32];

        let read = reader.read(&mut buf).unwrap();

        reader.close().unwrap();

        (read, buf)
    })
    .unwrap()
    .join()
    .unwrap();

    assert_eq!(read, 11);
    assert_eq!(&buf[..read], "hello world".as_bytes());
}

#[test]
fn test_pipe_in_another_process() {
    println!("Compiling pipe_echoer...");
    const BINARY_NAME: &str = "pipe_echoer";

    Command::new("cargo")
        .arg("build")
        .arg("--bin")
        .arg(BINARY_NAME)
        .stderr(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    let UnidirectionalPipe { read, write } =
        UnidirectionalPipe::create_from_options(UniDirectionalPipeOptions {
            encoding: PipeEncoding::None,
            mode: PipeMode::CrossProcess,
        })
        .unwrap();

    println!("read: {:?}", read);
    println!("write: {:?}", write);

    // serialize the pipe
    let serialized = serde_json::to_string(&read).unwrap();
    println!("{}", serialized);

    // spawn a new process
    let mut res = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg(BINARY_NAME)
        .arg(serialized)
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    println!("Running pipe_echoer...");
    let mut writer = write.open().unwrap();
    // write hello world to the pipe
    let written = writer.write("hello world".as_bytes()).unwrap();
    assert_eq!(written, 11);
    writer.close().unwrap();

    let code = res.wait().unwrap();

    if !code.success() {
        panic!("stderr: {}", as_string(res.stderr.take()));
    }

    assert_eq!(as_string(res.stdout.take()), "hello world\n");
}
