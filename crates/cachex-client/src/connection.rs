

use cachex_core::protocol::{encode_command, parse_command, parse_response, Command, Response};
use std::io;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(250);

pub struct Connection {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl Connection {
    pub async fn connect(address: &str) -> io::Result<Self> {
        let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "connect timed out"))??;
        let (reader, writer) = stream.into_split();
        Ok(Connection {
            reader: BufReader::new(reader),
            writer,
        })
    }

    pub async fn command(&mut self, command: &Command) -> io::Result<Response> {
        let mut line = encode_command(command);
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;

        let mut buf = Vec::with_capacity(256);
        let read = self.reader.read_until(b'\n', &mut buf).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed by server",
            ));
        }
        let text = String::from_utf8_lossy(&buf);
        parse_response(&text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
    }
}

pub async fn one_shot(address: &str, command: &Command) -> io::Result<Response> {
    let mut connection = Connection::connect(address).await?;
    connection.command(command).await
}

pub fn parse_command_line(line: &str) -> Result<Command, cachex_core::protocol::ProtocolError> {
    parse_command(line)
}