use std::io::Read;
use std::net::TcpStream;

use bytes::{BufMut, BytesMut};
use prost::Message;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/event.proto.rs"));
}

fn main() {
    let mut stream =
        TcpStream::connect("127.0.0.1:5000").expect("failed to connect to TCP endpoint");

    let mut buf = BytesMut::new();
    let mut rbuf = [0u8; 1024];

    loop {
        match stream.read(&mut rbuf[..]) {
            Ok(0) => {
                println!("server disconnected, closing");
                break;
            }
            Ok(n) => buf.put_slice(&rbuf[..n]),
            Err(e) => eprintln!("read error: {:?}", e),
        };

        match proto::Event::decode_length_delimited(&mut buf) {
            Err(e) => eprintln!("decode error: {:?}", e),
            Ok(msg) => println!("event: {:?}", msg),
        }
    }
}
