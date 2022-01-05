use std::{
    pin::Pin,
    task::{Context, Poll},
};

use async_native_tls::TlsStream;
use async_tungstenite::{
    tungstenite::{self, Message},
    WebSocketStream,
};
use futures_util::{Sink, Stream};
use smol::net::TcpStream;

/// A WebSocket or WebSocket+TLS connection.
///
/// Taken from:
/// https://github.com/smol-rs/smol/blob/1a542a8864a770c04ae35c4e4f79a650078f72e5/examples/websocket-server.rs#L79
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum WsStream {
    /// A plain WebSocket connection.
    Plain(WebSocketStream<TcpStream>),

    /// A WebSocket connection secured by TLS.
    Tls(WebSocketStream<TlsStream<TcpStream>>),
}

impl Sink<Message> for WsStream {
    type Error = tungstenite::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match &mut *self {
            WsStream::Plain(s) => Pin::new(s).poll_ready(cx),
            WsStream::Tls(s) => Pin::new(s).poll_ready(cx),
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        match &mut *self {
            WsStream::Plain(s) => Pin::new(s).start_send(item),
            WsStream::Tls(s) => Pin::new(s).start_send(item),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match &mut *self {
            WsStream::Plain(s) => Pin::new(s).poll_flush(cx),
            WsStream::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match &mut *self {
            WsStream::Plain(s) => Pin::new(s).poll_close(cx),
            WsStream::Tls(s) => Pin::new(s).poll_close(cx),
        }
    }
}

impl Stream for WsStream {
    type Item = tungstenite::Result<Message>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match &mut *self {
            WsStream::Plain(s) => Pin::new(s).poll_next(cx),
            WsStream::Tls(s) => Pin::new(s).poll_next(cx),
        }
    }
}
