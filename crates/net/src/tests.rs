use std::{net::SocketAddr, time::Duration};

use bytes::Bytes;
use http::StatusCode;
use meow_url_policy::BrowserUrl;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

use super::*;

#[tokio::test]
async fn loads_response_and_retains_metadata() {
    let server = TestServer::spawn().await;
    let response = Loader::default()
        .load(Request::get(server.url("/ok")), &CancellationToken::new())
        .await
        .expect("response should load");

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body, Bytes::from_static(b"hello meow"));
    assert_eq!(response.metadata.received_bytes, 10);
    assert_eq!(
        response.metadata.content_type.as_deref(),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(response.metadata.declared_content_length, Some(10));
    assert_eq!(response.metadata.http_version, HttpVersion::Http11);
    assert!(response.metadata.redirects.is_empty());
}

#[tokio::test]
async fn follows_relative_redirects() {
    let server = TestServer::spawn().await;
    let response = Loader::default()
        .load(
            Request::get(server.url("/redirect")),
            &CancellationToken::new(),
        )
        .await
        .expect("redirect should load");

    assert_eq!(response.body, Bytes::from_static(b"hello meow"));
    assert_eq!(response.metadata.redirects.len(), 1);
    assert_eq!(response.metadata.redirects[0].status, StatusCode::FOUND);
    assert_eq!(response.metadata.final_url, server.url("/ok"));
}

#[tokio::test]
async fn enforces_response_byte_limit() {
    let server = TestServer::spawn().await;
    let config = LoadConfig {
        max_response_bytes: 8,
        ..LoadConfig::default()
    };
    let error = Loader::new(config)
        .load(
            Request::get(server.url("/large")),
            &CancellationToken::new(),
        )
        .await
        .expect_err("large body should fail");

    assert!(matches!(error, NetError::ResponseTooLarge { limit: 8 }));
}

#[tokio::test]
async fn enforces_timeout_and_cancellation() {
    let server = TestServer::spawn().await;
    let config = LoadConfig {
        request_timeout: Duration::from_millis(30),
        ..LoadConfig::default()
    };
    let timeout_error = Loader::new(config)
        .load(Request::get(server.url("/slow")), &CancellationToken::new())
        .await
        .expect_err("slow response should time out");
    assert!(matches!(timeout_error, NetError::Timeout));

    let token = CancellationToken::new();
    let cancellation = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancellation.cancel();
    });
    let cancelled_error = Loader::default()
        .load(Request::get(server.url("/slow")), &token)
        .await
        .expect_err("cancelled response should fail");
    assert!(matches!(cancelled_error, NetError::Cancelled));
}

struct TestServer {
    address: SocketAddr,
    task: JoinHandle<()>,
}

impl TestServer {
    async fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(handle_connection(stream));
            }
        });
        Self { address, task }
    }

    fn url(&self, path: &str) -> BrowserUrl {
        BrowserUrl::parse(&format!("http://{}{path}", self.address)).unwrap()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn handle_connection(mut stream: TcpStream) {
    let mut request = vec![0_u8; 4096];
    let Ok(read) = stream.read(&mut request).await else {
        return;
    };
    let request = String::from_utf8_lossy(&request[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let response = match path {
        "/ok" => "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 10\r\nConnection: close\r\n\r\nhello meow".to_owned(),
        "/redirect" => "HTTP/1.1 302 Found\r\nLocation: /ok\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        "/large" => format!(
            "HTTP/1.1 200 OK\r\nContent-Length: 64\r\nConnection: close\r\n\r\n{}",
            "x".repeat(64)
        ),
        "/slow" => {
            tokio::time::sleep(Duration::from_millis(200)).await;
            "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nslow".to_owned()
        }
        _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
    };
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}
