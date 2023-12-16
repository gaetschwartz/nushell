use std::{
    io::{Read, Write},
    process::Command,
};

use nu_pipes::{
    unidirectional::{pipe, PipeRead},
    utils, PipeFd,
};

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
fn pipes_readwrite() {
    let (read, write) = pipe().unwrap();
    let mut reader = read.into_reader();
    let mut writer = write.into_writer();
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
fn pipes_with_closed_read_end_cant_write() {
    let (read, write) = pipe().unwrap();
    let mut reader = read.into_reader();
    let mut writer = write.into_writer();
    // write hello world to the pipe
    let written = writer.write("hello world".as_bytes()).unwrap();
    writer.flush().unwrap();

    assert_eq!(written, 11);

    let mut buf = [0u8; 11];
    let read = reader.read(&mut buf).unwrap();
    assert_eq!(read, 11);
    assert_eq!(&buf[..], "hello world".as_bytes());

    reader.close().unwrap();

    let written = writer.write("woohoo ohoho".as_bytes());
    let flushed = writer.flush();

    assert!(
        written.is_err() || flushed.is_err(),
        "Expected error, but {}",
        written
            .map(|b| format!("wrote {} bytes", b))
            .or_else(|_| flushed.map(|_| "flushed".to_string()))
            .unwrap()
    );
}

#[test]
fn pipe_read_write_in_thread() {
    let (read, write) = pipe().unwrap();
    let mut writer = write.into_writer();
    // write hello world to the pipe
    let written = writer.write("hello world".as_bytes()).unwrap();

    assert_eq!(written, 11);
    writer.close().unwrap();

    // serialize the pipe
    let serialized = serde_json::to_string(&read).unwrap();
    // spawn a new process
    let (read, buf) = utils::named_thread("thread@pipe_in_another_thread", move || {
        // deserialize the pipe
        let deserialized: PipeFd<PipeRead> = serde_json::from_str(&serialized).unwrap();
        let mut reader = deserialized.into_reader();

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

trait ReadExact: Read {
    fn read_exactly_n<const N: usize>(&mut self) -> Result<[u8; N], std::io::Error> {
        let mut buf = [0u8; N];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }
}
impl<R: Read> ReadExact for R {}

#[test]
fn pipe_in_another_thread_cancelled() {
    let (read, write) = pipe().unwrap();

    let thread: std::thread::JoinHandle<Result<(), std::io::Error>> =
        utils::named_thread("thread@pipe_in_another_thread_cancelled", move || {
            let mut writer = write.into_writer();

            // serialize the pipe
            loop {
                eprintln!("Writing to pipe...");
                _ = writer.write("hello world".as_bytes())?;
                std::thread::sleep(std::time::Duration::from_millis(50));
                writer.flush()?;
            }
        })
        .unwrap();

    let mut reader = read.into_reader();
    eprintln!("Starting to read from pipe...");
    let s1 = reader.read_exactly_n::<11>().unwrap();
    eprintln!("Read from pipe... (1)");
    assert_eq!(&s1[..], b"hello world");
    eprintln!("Read from pipe... (2)");
    let s2 = reader.read_exactly_n::<11>().unwrap();
    assert_eq!(&s2[..], b"hello world");
    eprintln!("Closing pipe...");
    reader.close().unwrap();
    eprintln!("Joining thread...");
    let joined = thread.join().unwrap();
    println!("This error is expected: {:?}", joined);
    match joined {
        Ok(_) => panic!("Thread should have been cancelled"),
        Err(e) => match e.kind() {
            std::io::ErrorKind::BrokenPipe => {}
            _ => panic!("Unexpected error: {:?}", e),
        },
    }
}

#[test]
fn test_pipe_in_another_process() {
    println!("Compiling pipe_echoer...");
    const BINARY_NAME: &str = "pipe_echoer";

    Command::new("cargo")
        .arg("build")
        .arg("-q")
        .arg("--bin")
        .arg(BINARY_NAME)
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    let (read, write) = pipe().unwrap();
    println!("read: {:?}", read);
    println!("write: {:?}", write);
    let read_dup = read.try_clone().unwrap();

    // serialize the pipe
    let json = serde_json::to_string(&read_dup).unwrap();
    read.close().unwrap();

    println!("Running pipe_echoer...");

    // spawn a new process
    let mut res = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg(BINARY_NAME)
        .arg(json)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // write hello world to the pipe
    let mut writer = write.into_writer();
    let written = writer.write(b"hello world").unwrap();
    assert_eq!(written, 11);
    writer.flush().unwrap();
    writer.close().unwrap();

    println!("Waiting for pipe_echoer to finish...");

    let code = res.wait().unwrap();
    // read_dup.close().unwrap();

    if !code.success() {
        panic!("Process failed: {:?}", code);
    }

    assert_eq!(as_string(res.stdout.take()), "hello world\n");
}
