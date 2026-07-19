//! Bounded static image loading, decoding, and per-engine resource caching.

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fmt,
    sync::Arc,
};

use base64::Engine as _;
use image::{DynamicImage, ImageFormat};
use meow_html::{Document, NodeId};
use meow_net::{CancellationToken, Loader, Request};
use meow_url_policy::BrowserUrl;

pub const MAX_IMAGE_DIMENSION: u32 = 4096;
pub const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
pub const DEFAULT_IMAGE_CACHE_ENTRIES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
    Gif,
    Svg,
}

#[derive(Clone, Debug)]
pub struct ImageResource {
    pub source_url: String,
    pub width: u32,
    pub height: u32,
    pub kind: ImageKind,
    /// Premultiplied RGBA8 pixels.
    pub pixels: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageLoadError {
    pub node: NodeId,
    pub source: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageCacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub decodes: u64,
    pub evictions: u64,
    pub resident_entries: usize,
    pub resident_bytes: usize,
}

#[derive(Debug)]
pub struct ImageCache {
    max_entries: usize,
    entries: HashMap<String, Arc<ImageResource>>,
    order: VecDeque<String>,
    metrics: ImageCacheMetrics,
}

impl ImageCache {
    #[must_use]
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries: max_entries.max(1),
            entries: HashMap::new(),
            order: VecDeque::new(),
            metrics: ImageCacheMetrics::default(),
        }
    }

    #[must_use]
    pub fn metrics(&self) -> ImageCacheMetrics {
        ImageCacheMetrics {
            resident_entries: self.entries.len(),
            resident_bytes: self
                .entries
                .values()
                .map(|resource| resource.pixels.len())
                .sum(),
            ..self.metrics
        }
    }

    fn get(&mut self, key: &str) -> Option<Arc<ImageResource>> {
        let value = self.entries.get(key).cloned();
        if value.is_some() {
            self.metrics.hits = self.metrics.hits.saturating_add(1);
            self.touch(key);
        } else {
            self.metrics.misses = self.metrics.misses.saturating_add(1);
        }
        value
    }

    fn insert(&mut self, key: String, resource: Arc<ImageResource>) {
        self.entries.insert(key.clone(), resource);
        self.touch(&key);
        while self.entries.len() > self.max_entries {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if self.entries.remove(&oldest).is_some() {
                self.metrics.evictions = self.metrics.evictions.saturating_add(1);
            }
        }
    }

    fn touch(&mut self, key: &str) {
        self.order.retain(|candidate| candidate != key);
        self.order.push_back(key.to_owned());
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new(DEFAULT_IMAGE_CACHE_ENTRIES)
    }
}

pub(crate) async fn load_document_images(
    loader: &Loader,
    document: &Document,
    base_url: &BrowserUrl,
    cancellation: &CancellationToken,
    cache: &mut ImageCache,
) -> (BTreeMap<NodeId, Arc<ImageResource>>, Vec<ImageLoadError>) {
    let mut images = BTreeMap::new();
    let mut errors = Vec::new();
    for element in document.elements_in_tree_order() {
        if document.element_local_name(&element).as_deref() != Some("img") {
            continue;
        }
        let Some(source) = document.element_attribute(&element, "src") else {
            continue;
        };
        match load_image(loader, base_url, &source, cancellation, cache).await {
            Ok(image) => {
                images.insert(element.id(), image);
            }
            Err(message) => errors.push(ImageLoadError {
                node: element.id(),
                source,
                message,
            }),
        }
    }
    (images, errors)
}

async fn load_image(
    loader: &Loader,
    base_url: &BrowserUrl,
    source: &str,
    cancellation: &CancellationToken,
    cache: &mut ImageCache,
) -> Result<Arc<ImageResource>, String> {
    let (key, final_url, content_type, bytes) = if source.starts_with("data:") {
        let key = source.to_owned();
        if let Some(resource) = cache.get(&key) {
            return Ok(resource);
        }
        let (content_type, bytes) = decode_data_url(source)?;
        (key, source.to_owned(), Some(content_type), bytes)
    } else {
        let requested = base_url
            .resolve(source)
            .map_err(|error| error.to_string())?;
        let key = requested.to_string();
        if let Some(resource) = cache.get(&key) {
            return Ok(resource);
        }
        let response = loader
            .load(Request::image(requested), cancellation)
            .await
            .map_err(|error| error.to_string())?;
        if !response.status.is_success() {
            return Err(format!("image HTTP status {}", response.status));
        }
        (
            key,
            response.metadata.final_url.to_string(),
            response.metadata.content_type,
            response.body.to_vec(),
        )
    };
    let decode_span = tracing::info_span!(
        "image_decode",
        source = %final_url,
        bytes = bytes.len(),
    );
    let _decode_guard = decode_span.enter();
    let resource = Arc::new(decode_image(&final_url, content_type.as_deref(), &bytes)?);
    cache.metrics.decodes = cache.metrics.decodes.saturating_add(1);
    cache.insert(key, Arc::clone(&resource));
    Ok(resource)
}

fn decode_image(
    source_url: &str,
    content_type: Option<&str>,
    bytes: &[u8],
) -> Result<ImageResource, String> {
    let svg = content_type
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("image/svg+xml"))
        || bytes.starts_with(b"<svg")
        || bytes
            .windows(4)
            .position(|window| window.eq_ignore_ascii_case(b"<svg"))
            .is_some_and(|offset| offset < 256);
    if svg {
        return decode_svg(source_url, bytes);
    }
    let format = image::guess_format(bytes).map_err(|error| error.to_string())?;
    let kind = match format {
        ImageFormat::Png => ImageKind::Png,
        ImageFormat::Jpeg => ImageKind::Jpeg,
        ImageFormat::Gif => ImageKind::Gif,
        _ => return Err(format!("unsupported raster image format {format:?}")),
    };
    let image = image::load_from_memory_with_format(bytes, format)
        .map_err(|error| format!("image decode failed: {error}"))?;
    decoded_dynamic(source_url, kind, image)
}

fn decoded_dynamic(
    source_url: &str,
    kind: ImageKind,
    image: DynamicImage,
) -> Result<ImageResource, String> {
    let width = image.width();
    let height = image.height();
    validate_dimensions(width, height)?;
    let mut pixels = image.into_rgba8().into_raw();
    premultiply(&mut pixels);
    Ok(ImageResource {
        source_url: source_url.to_owned(),
        width,
        height,
        kind,
        pixels: pixels.into(),
    })
}

fn decode_svg(source_url: &str, bytes: &[u8]) -> Result<ImageResource, String> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(bytes, &options)
        .map_err(|error| format!("SVG parse failed: {error}"))?;
    let size = tree.size().to_int_size();
    let width = size.width();
    let height = size.height();
    validate_dimensions(width, height)?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| "SVG raster allocation failed".to_owned())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Ok(ImageResource {
        source_url: source_url.to_owned(),
        width,
        height,
        kind: ImageKind::Svg,
        pixels: pixmap.data().to_vec().into(),
    })
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), String> {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || pixels > MAX_IMAGE_PIXELS
    {
        return Err(format!(
            "image dimensions {width}x{height} exceed the decoder limit"
        ));
    }
    Ok(())
}

fn premultiply(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        pixel[0] = ((u16::from(pixel[0]) * alpha + 127) / 255) as u8;
        pixel[1] = ((u16::from(pixel[1]) * alpha + 127) / 255) as u8;
        pixel[2] = ((u16::from(pixel[2]) * alpha + 127) / 255) as u8;
    }
}

fn decode_data_url(source: &str) -> Result<(String, Vec<u8>), String> {
    let payload = source
        .strip_prefix("data:")
        .ok_or_else(|| "not a data URL".to_owned())?;
    let (metadata, data) = payload
        .split_once(',')
        .ok_or_else(|| "data URL omitted comma separator".to_owned())?;
    let mut pieces = metadata.split(';');
    let content_type = pieces
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("text/plain")
        .to_owned();
    let base64 = pieces.any(|value| value.eq_ignore_ascii_case("base64"));
    let bytes = if base64 {
        base64::engine::general_purpose::STANDARD
            .decode(data.trim())
            .map_err(|error| format!("invalid base64 data URL: {error}"))?
    } else {
        percent_decode(data)?
    };
    Ok((content_type, bytes))
}

fn percent_decode(value: &str) -> Result<Vec<u8>, String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = bytes
                .get(index + 1..index + 3)
                .ok_or_else(|| "truncated percent escape in data URL".to_owned())?;
            let pair = std::str::from_utf8(pair).map_err(|error| error.to_string())?;
            output.push(
                u8::from_str_radix(pair, 16)
                    .map_err(|_| "invalid percent escape in data URL".to_owned())?,
            );
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    Ok(output)
}

impl fmt::Display for ImageKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_svg_decodes_and_cache_metrics_are_stable() {
        let source = "data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='2'%20height='2'%3E%3Crect%20width='2'%20height='2'%20fill='red'/%3E%3C/svg%3E";
        let (content_type, bytes) = decode_data_url(source).unwrap();
        let image = decode_image(source, Some(&content_type), &bytes).unwrap();
        assert_eq!(
            (image.width, image.height, image.kind),
            (2, 2, ImageKind::Svg)
        );
        assert_eq!(image.pixels.len(), 16);
    }
}
