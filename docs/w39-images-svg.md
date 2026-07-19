# W39 images, data URLs, and SVG subset

Navigation discovers `<img src>` elements and resolves resources before document commit. External images use the existing bounded HTTP loader. Data URLs support base64 and percent-encoded payloads.

The static decoder accepts PNG, JPEG, and the first frame of GIF through the `image` crate. Basic SVG is parsed and rasterized through resvg/usvg. Decoded images are bounded to 4096×4096 and 16 million pixels, converted to premultiplied RGBA, cached by canonical source URL, and attached to DOM node IDs.

Replaced-image layout supports explicit CSS width/height, intrinsic dimensions, and auto-height aspect-ratio preservation. The display list stores image resources separately from `DrawImage` commands. The media integration test renders PNG, JPEG, GIF, and data-SVG into a flex strip and verifies output pixels plus cache hits after reload.
