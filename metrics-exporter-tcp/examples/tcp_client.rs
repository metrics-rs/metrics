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

        loop {
            let needed = match prost::decode_length_delimiter(&buf[..]) {
                Err(e) => {
                    // According to decode_length_delimiter doc:
                    // If the supplied buffer contains fewer than 10 bytes, then an error indicates that more input is required to decode the full delimiter.
                    // If the supplied buffer contains 10 bytes or more, then the buffer contains an invalid delimiter, and typically the buffer should be considered corrupt.
                    if buf.len() >= 10 {
                        eprintln!("decode error in size: {:?}", e);
                    }
                    break;
                }
                Ok(usize) => prost::length_delimiter_len(usize) + usize,
            };
            if buf.len() < needed {
                break;
            }
            let packet = buf.split_to(needed);
            match proto::Event::decode_length_delimited(&packet[..]) {
                Err(e) => eprintln!("decode error: {:?}", e),
                Ok(msg) => println!("event: {:?}", msg),
            }
        }
    }
}
