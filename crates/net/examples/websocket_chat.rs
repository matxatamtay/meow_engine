use std::{env, error::Error};

use futures_util::{SinkExt, StreamExt};
use tokio::{net::TcpListener, sync::broadcast};
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let address = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9001".to_owned());
    let listener = TcpListener::bind(&address).await?;
    let (messages, _) = broadcast::channel::<String>(128);
    println!("MeowEngine chat WebSocket listening on ws://{address}/");

    loop {
        let (stream, peer) = listener.accept().await?;
        let messages = messages.clone();
        let mut receiver = messages.subscribe();
        tokio::spawn(async move {
            let socket = match accept_async(stream).await {
                Ok(socket) => socket,
                Err(error) => {
                    eprintln!("WebSocket handshake from {peer} failed: {error}");
                    return;
                }
            };
            let (mut writer, mut reader) = socket.split();
            let joined = format!("system: {peer} joined");
            let _ = messages.send(joined);

            loop {
                tokio::select! {
                    incoming = reader.next() => {
                        match incoming {
                            Some(Ok(Message::Text(text))) => {
                                let _ = messages.send(text.to_string());
                            }
                            Some(Ok(Message::Close(_))) | None => break,
                            Some(Ok(Message::Binary(_)
                                | Message::Ping(_)
                                | Message::Pong(_)
                                | Message::Frame(_))) => {}
                            Some(Err(error)) => {
                                eprintln!("WebSocket read from {peer} failed: {error}");
                                break;
                            }
                        }
                    }
                    outgoing = receiver.recv() => {
                        match outgoing {
                            Ok(text) => {
                                if writer.send(Message::Text(text.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
            let _ = messages.send(format!("system: {peer} left"));
        });
    }
}
