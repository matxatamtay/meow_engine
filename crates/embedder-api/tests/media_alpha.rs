use std::{io::Cursor, sync::Arc};

use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use meow_embedder_api::{BrowserEngine, CancellationToken, DisplayCommand, ImageKind};
use meow_renderer::{ReferenceRenderer, Renderer};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::test]
async fn png_jpeg_gif_data_svg_flex_and_cache_render_a_landing_strip() {
    let png = encode(ImageFormat::Png, [255, 0, 0, 255]);
    let jpeg = encode(ImageFormat::Jpeg, [0, 255, 0, 255]);
    let gif = encode(ImageFormat::Gif, [0, 0, 255, 255]);
    let server = MediaServer::spawn(png, jpeg, gif).await;
    let mut engine = BrowserEngine::new();
    engine
        .navigate(&server.url(), &CancellationToken::new())
        .await
        .unwrap();

    assert!(engine.current_document().image_errors.is_empty());
    let kinds = engine
        .current_document()
        .images
        .values()
        .map(|image| image.kind)
        .collect::<Vec<_>>();
    assert!(kinds.contains(&ImageKind::Png));
    assert!(kinds.contains(&ImageKind::Jpeg));
    assert!(kinds.contains(&ImageKind::Gif));
    assert!(kinds.contains(&ImageKind::Svg));

    let first_metrics = engine.current_document().image_cache_metrics;
    let frame = engine.render_document_frame(160, 40).unwrap();
    assert_eq!(frame.display_list().images().len(), 4);
    assert_eq!(
        frame
            .display_list()
            .commands()
            .iter()
            .filter(|command| matches!(command, DisplayCommand::DrawImage { .. }))
            .count(),
        4
    );
    let framebuffer = ReferenceRenderer::new()
        .render(frame.viewport(), frame.display_list())
        .unwrap();
    let red = pixel(&framebuffer, 20, 20);
    let green = pixel(&framebuffer, 60, 20);
    let blue = pixel(&framebuffer, 100, 20);
    let yellow = pixel(&framebuffer, 140, 20);
    assert!(red[0] > 220 && red[1] < 40 && red[2] < 40, "red={red:?}");
    assert!(
        green[1] > 180 && green[0] < 80 && green[2] < 80,
        "green={green:?}"
    );
    assert!(
        blue[2] > 220 && blue[0] < 40 && blue[1] < 40,
        "blue={blue:?}"
    );
    assert!(
        yellow[0] > 220 && yellow[1] > 180 && yellow[2] < 60,
        "yellow={yellow:?}"
    );

    engine.reload(&CancellationToken::new()).await.unwrap();
    let second_metrics = engine.current_document().image_cache_metrics;
    assert_eq!(second_metrics.decodes, first_metrics.decodes);
    assert!(second_metrics.hits >= first_metrics.hits + 4);
}

fn pixel(framebuffer: &meow_renderer::Framebuffer, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * framebuffer.width() + x) * 4) as usize;
    framebuffer.premultiplied_rgba()[offset..offset + 4]
        .try_into()
        .unwrap()
}

fn encode(format: ImageFormat, color: [u8; 4]) -> Vec<u8> {
    let image = ImageBuffer::from_pixel(2, 2, Rgba(color));
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut output, format)
        .unwrap();
    output.into_inner()
}

struct MediaServer {
    address: std::net::SocketAddr,
}

impl MediaServer {
    async fn spawn(png: Vec<u8>, jpeg: Vec<u8>, gif: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let assets = Arc::new((png, jpeg, gif));
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let assets = Arc::clone(&assets);
                tokio::spawn(async move {
                    let mut request = [0_u8; 4096];
                    let count = stream.read(&mut request).await.unwrap();
                    let request = String::from_utf8_lossy(&request[..count]);
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");
                    let svg = "data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='2'%20height='2'%3E%3Crect%20width='2'%20height='2'%20fill='%23ffd000'/%3E%3C/svg%3E";
                    let html = format!(
                        "<!doctype html><style>html,body{{display:block;margin:0}}main{{display:flex;width:160px;height:40px}}img{{width:40px;height:40px}}</style><main><img src='/red.png'><img src='/green.jpg'><img src='/blue.gif'><img src=\"{svg}\"></main>"
                    );
                    let (status, content_type, body): (&str, &str, Vec<u8>) = match path {
                        "/" => ("200 OK", "text/html; charset=utf-8", html.into_bytes()),
                        "/red.png" => ("200 OK", "image/png", assets.0.clone()),
                        "/green.jpg" => ("200 OK", "image/jpeg", assets.1.clone()),
                        "/blue.gif" => ("200 OK", "image/gif", assets.2.clone()),
                        _ => ("404 Not Found", "text/plain", b"missing".to_vec()),
                    };
                    let header = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    stream.write_all(header.as_bytes()).await.unwrap();
                    stream.write_all(&body).await.unwrap();
                });
            }
        });
        Self { address }
    }

    fn url(&self) -> String {
        format!("http://{}/", self.address)
    }
}
