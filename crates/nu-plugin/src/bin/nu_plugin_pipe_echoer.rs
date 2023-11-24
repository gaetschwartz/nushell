use nu_plugin::OsPipe;
use std::io::Read;

fn main() {
    let serialized = std::env::args().nth(1).unwrap();
    let mut deserialized: OsPipe = serde_json::from_str(&serialized).unwrap();

    let mut buf = [0u8; 11];

    _ = deserialized.read(&mut buf).unwrap();

    println!("{}", std::str::from_utf8(&buf).unwrap());

    deserialized.close().unwrap();
}
