use std::io::Read;

use nu_pipes::unidirectional::{PipeRead, UnOpenedPipe};

fn main() {
    let serialized = std::env::args().nth(1).unwrap();
    let deserialized: UnOpenedPipe<PipeRead> = serde_json::from_str(&serialized).unwrap();
    let mut reader = deserialized.open().unwrap();

    let mut buf = vec![0u8; 256];
    loop {
        let mut chunk = [0u8; 256];
        let read = reader.read(&mut chunk).unwrap();
        eprintln!("read {} bytes", read);
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
    }

    println!("{}", std::str::from_utf8(&buf).unwrap());
}
