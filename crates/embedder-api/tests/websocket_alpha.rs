use futures_util::{SinkExt, StreamExt};
use meow_embedder_api::{BrowserEngine, CancellationToken};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    time::{Duration, sleep},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::test]
async fn websocket_text_binary_close_and_events_reach_the_document() {
    let websocket = spawn_echo_websocket().await;
    let page = serve_page(format!(
        r#"<!doctype html>
<p id='state'>connecting</p><p id='text'>pending</p><p id='binary'>pending</p><p id='close'>pending</p>
<script>
const state = document.querySelector('#state');
const text = document.querySelector('#text');
const binary = document.querySelector('#binary');
const closed = document.querySelector('#close');
const socket = new WebSocket('{websocket}');
socket.onopen = () => {{
  state.textContent = 'open';
  socket.send('meow');
}};
socket.onmessage = event => {{
  if (typeof event.data === 'string') {{
    text.textContent = event.data;
    socket.send(new Uint8Array([1, 2, 3, 255]));
  }} else {{
    binary.textContent = Array.from(new Uint8Array(event.data)).join(',');
    socket.close(1000, 'done');
  }}
}};
socket.onerror = () => state.textContent = 'error';
socket.onclose = event => {{
  closed.textContent = event.code + ':' + event.reason + ':' + event.wasClean;
}};
</script>"#,
    ))
    .await;

    let mut engine = BrowserEngine::new();
    engine
        .navigate(&page, &CancellationToken::new())
        .await
        .unwrap();

    for _ in 0..100 {
        let report = engine.pump_web_tasks().await;
        assert!(
            report.errors.is_empty(),
            "web task errors: {:?}",
            report.errors
        );
        if element_text(&engine, "close") == "1000:done:true" {
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(element_text(&engine, "state"), "open");
    assert_eq!(element_text(&engine, "text"), "echo:meow");
    assert_eq!(element_text(&engine, "binary"), "255,3,2,1");
    assert_eq!(element_text(&engine, "close"), "1000:done:true");
}

async fn spawn_echo_websocket() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        while let Some(message) = socket.next().await {
            match message.unwrap() {
                Message::Text(text) => {
                    socket
                        .send(Message::Text(format!("echo:{text}").into()))
                        .await
                        .unwrap();
                }
                Message::Binary(bytes) => {
                    let mut bytes = bytes.to_vec();
                    bytes.reverse();
                    socket.send(Message::Binary(bytes.into())).await.unwrap();
                }
                Message::Close(_) => {
                    socket.flush().await.unwrap();
                    break;
                }
                Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await.unwrap(),
                Message::Pong(_) | Message::Frame(_) => {}
            }
        }
    });
    format!("ws://{address}/chat")
}

async fn serve_page(body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    format!("http://{address}/")
}

fn element_text(engine: &BrowserEngine, id: &str) -> String {
    let document = &engine.current_document().document;
    let element = document
        .elements_in_tree_order()
        .into_iter()
        .find(|element| document.element_attribute(element, "id").as_deref() == Some(id))
        .expect("element should exist");
    document.text_content(&element)
}
