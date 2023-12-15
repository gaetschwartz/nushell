use std::io::Read;

use nu_pipes::{unidirectional::PipeRead, PipeFd};

fn main() {
    let json_read = std::env::args().nth(1).unwrap();
    let read: PipeFd<PipeRead> = serde_json::from_str(&json_read).unwrap();
    let mut reader = read.into_reader();

    let mut buf = Vec::new();
    loop {
        let mut chunk = [0u8; 256];
        let read = reader.read(&mut chunk).unwrap();
        eprintln!("read {} bytes", read);
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
    }
    reader.close().unwrap();

    println!("{}", std::str::from_utf8(&buf).unwrap());
}
