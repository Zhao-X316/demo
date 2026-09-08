//! 使用 macOS CoreGraphics 将 PDF 拆为单页 PDF，并按页渲染为视觉模型可读的 JPEG。
//!
//! 当前桌面发布目标是 macOS；这里直接复用系统框架，避免为了一个入站步骤引入新的
//! PDF 解析供应链。非 macOS 构建保留明确错误，不会把整份 PDF 伪装成单页 artifact。

use std::path::Path;

#[cfg(not(target_os = "macos"))]
use suite_core::error::CoreError;
use suite_core::error::CoreResult;

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use image::codecs::jpeg::JpegEncoder;
    use image::GrayImage;
    use suite_core::error::{CoreError, CoreResult};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    type CFIndex = isize;
    type CFURLRef = *const c_void;
    type CGPDFDocumentRef = *const c_void;
    type CGPDFPageRef = *const c_void;
    type CGDataConsumerRef = *const c_void;
    type CGContextRef = *mut c_void;
    type CGColorSpaceRef = *const c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGAffineTransform {
        a: f64,
        b: f64,
        c: f64,
        d: f64,
        tx: f64,
        ty: f64,
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: *const c_void,
            buffer: *const u8,
            buffer_length: CFIndex,
            is_directory: bool,
        ) -> CFURLRef;
        fn CFRelease(value: *const c_void);
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPDFDocumentCreateWithURL(url: CFURLRef) -> CGPDFDocumentRef;
        fn CGPDFDocumentGetNumberOfPages(document: CGPDFDocumentRef) -> usize;
        fn CGPDFDocumentGetPage(document: CGPDFDocumentRef, page_number: usize) -> CGPDFPageRef;
        fn CGPDFDocumentRelease(document: CGPDFDocumentRef);
        fn CGPDFPageGetBoxRect(page: CGPDFPageRef, box_kind: i32) -> CGRect;
        fn CGPDFPageGetRotationAngle(page: CGPDFPageRef) -> i32;
        fn CGPDFPageGetDrawingTransform(
            page: CGPDFPageRef,
            box_kind: i32,
            rect: CGRect,
            rotate: i32,
            preserve_aspect_ratio: bool,
        ) -> CGAffineTransform;
        fn CGDataConsumerCreateWithURL(url: CFURLRef) -> CGDataConsumerRef;
        fn CGDataConsumerRelease(consumer: CGDataConsumerRef);
        fn CGPDFContextCreate(
            consumer: CGDataConsumerRef,
            media_box: *const CGRect,
            auxiliary_info: *const c_void,
        ) -> CGContextRef;
        fn CGPDFContextBeginPage(context: CGContextRef, page_info: *const c_void);
        fn CGContextDrawPDFPage(context: CGContextRef, page: CGPDFPageRef);
        fn CGPDFContextEndPage(context: CGContextRef);
        fn CGPDFContextClose(context: CGContextRef);
        fn CGContextRelease(context: CGContextRef);
        fn CGColorSpaceCreateDeviceGray() -> CGColorSpaceRef;
        fn CGColorSpaceRelease(color_space: CGColorSpaceRef);
        fn CGBitmapContextCreate(
            data: *mut c_void,
            width: usize,
            height: usize,
            bits_per_component: usize,
            bytes_per_row: usize,
            color_space: CGColorSpaceRef,
            bitmap_info: u32,
        ) -> CGContextRef;
        fn CGContextConcatCTM(context: CGContextRef, transform: CGAffineTransform);
    }

    struct SourceDocument {
        url: CFURLRef,
        document: CGPDFDocumentRef,
    }

    impl Drop for SourceDocument {
        fn drop(&mut self) {
            // SAFETY: both references are created by the matching Create functions and owned here.
            unsafe {
                CGPDFDocumentRelease(self.document);
                CFRelease(self.url);
            }
        }
    }

    fn file_url(path: &Path) -> CoreResult<CFURLRef> {
        let bytes = path.as_os_str().as_bytes();
        // SAFETY: bytes points to a live path buffer for the duration of the call; CoreFoundation
        // creates and owns an independent URL object on success.
        let url = unsafe {
            CFURLCreateFromFileSystemRepresentation(
                std::ptr::null(),
                bytes.as_ptr(),
                bytes.len() as CFIndex,
                false,
            )
        };
        if url.is_null() {
            Err(CoreError::Parse("PDF 文件路径无法打开".into()))
        } else {
            Ok(url)
        }
    }

    fn open_document(path: &Path) -> CoreResult<SourceDocument> {
        let url = file_url(path)?;
        // SAFETY: url is a valid CFURL created above and remains alive in SourceDocument.
        let document = unsafe { CGPDFDocumentCreateWithURL(url) };
        if document.is_null() {
            // SAFETY: url was created by CFURLCreate... and has not been released.
            unsafe { CFRelease(url) };
            return Err(CoreError::Parse("PDF 无法读取或已损坏".into()));
        }
        Ok(SourceDocument { url, document })
    }

    fn temp_root() -> CoreResult<PathBuf> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "jiaofu-pdf-pages-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root)
            .map_err(|error| CoreError::Io(format!("创建 PDF 拆页临时目录失败：{error}")))?;
        Ok(root)
    }

    fn render_page(
        document: CGPDFDocumentRef,
        page_number: usize,
        output: &Path,
    ) -> CoreResult<()> {
        // SAFETY: document is alive for this call and page_number was bounded by page count.
        let page = unsafe { CGPDFDocumentGetPage(document, page_number) };
        if page.is_null() {
            return Err(CoreError::Parse(format!("PDF 第 {page_number} 页无法读取")));
        }
        let output_url = file_url(output)?;
        // SAFETY: output_url is a valid CFURL retained until the end of this function.
        let consumer = unsafe { CGDataConsumerCreateWithURL(output_url) };
        if consumer.is_null() {
            // SAFETY: output_url was created by file_url and has not been released.
            unsafe { CFRelease(output_url) };
            return Err(CoreError::Io(format!(
                "无法创建 PDF 第 {page_number} 页输出"
            )));
        }
        // kCGPDFMediaBox = 0. The page and returned box remain valid while document is alive.
        let media_box = unsafe { CGPDFPageGetBoxRect(page, 0) };
        // SAFETY: consumer and media_box remain live for the context lifetime.
        let context = unsafe { CGPDFContextCreate(consumer, &media_box, std::ptr::null()) };
        if context.is_null() {
            // SAFETY: both values were created above and are owned by this function.
            unsafe {
                CGDataConsumerRelease(consumer);
                CFRelease(output_url);
            }
            return Err(CoreError::Io(format!(
                "无法创建 PDF 第 {page_number} 页上下文"
            )));
        }
        // SAFETY: all CoreGraphics references are valid and owned for the duration of the calls.
        unsafe {
            CGPDFContextBeginPage(context, std::ptr::null());
            CGContextDrawPDFPage(context, page);
            CGPDFContextEndPage(context);
            CGPDFContextClose(context);
            CGContextRelease(context);
            CGDataConsumerRelease(consumer);
            CFRelease(output_url);
        }
        Ok(())
    }

    pub fn split(path: &Path) -> CoreResult<Vec<Vec<u8>>> {
        let source = open_document(path)?;
        // SAFETY: source.document is a live CGPDFDocumentRef.
        let page_count = unsafe { CGPDFDocumentGetNumberOfPages(source.document) };
        if page_count == 0 {
            return Err(CoreError::Invalid("PDF 不包含可导入页面".into()));
        }
        let root = temp_root()?;
        let result = (|| -> CoreResult<Vec<Vec<u8>>> {
            let mut pages = Vec::with_capacity(page_count);
            for page_number in 1..=page_count {
                let output = root.join(format!("page-{page_number}.pdf"));
                render_page(source.document, page_number, &output)?;
                pages.push(std::fs::read(&output).map_err(|error| {
                    CoreError::Io(format!("读取 PDF 第 {page_number} 页失败：{error}"))
                })?);
            }
            Ok(pages)
        })();
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    fn render_visual_page(document: CGPDFDocumentRef, page_number: usize) -> CoreResult<Vec<u8>> {
        const LONG_EDGE: f64 = 1800.0;
        // SAFETY: document remains alive and page_number was bounded by the page count.
        let page = unsafe { CGPDFDocumentGetPage(document, page_number) };
        if page.is_null() {
            return Err(CoreError::Parse(format!("PDF 第 {page_number} 页无法读取")));
        }
        // kCGPDFMediaBox = 0. CoreGraphics owns the returned page box value.
        let media_box = unsafe { CGPDFPageGetBoxRect(page, 0) };
        if !media_box.size.width.is_finite()
            || !media_box.size.height.is_finite()
            || media_box.size.width <= 0.0
            || media_box.size.height <= 0.0
        {
            return Err(CoreError::Parse(format!("PDF 第 {page_number} 页尺寸非法")));
        }
        // Rotation affects the output aspect ratio; the drawing transform below applies it.
        let rotation = unsafe { CGPDFPageGetRotationAngle(page) }.rem_euclid(360);
        let (logical_width, logical_height) = if matches!(rotation, 90 | 270) {
            (media_box.size.height, media_box.size.width)
        } else {
            (media_box.size.width, media_box.size.height)
        };
        let scale = LONG_EDGE / logical_width.max(logical_height);
        let width = (logical_width * scale).round().max(1.0) as usize;
        let height = (logical_height * scale).round().max(1.0) as usize;
        let byte_len = width
            .checked_mul(height)
            .ok_or_else(|| CoreError::Invalid("PDF 视觉页尺寸过大".into()))?;
        let mut pixels = vec![255_u8; byte_len];
        // SAFETY: the created color space is owned until released below.
        let color_space = unsafe { CGColorSpaceCreateDeviceGray() };
        if color_space.is_null() {
            return Err(CoreError::Io("无法创建 PDF 灰度渲染色彩空间".into()));
        }
        // kCGImageAlphaNone = 0. `pixels` stays allocated and unmoved until the context is released.
        let context = unsafe {
            CGBitmapContextCreate(
                pixels.as_mut_ptr().cast(),
                width,
                height,
                8,
                width,
                color_space,
                0,
            )
        };
        if context.is_null() {
            // SAFETY: color_space was created above and is still owned here.
            unsafe { CGColorSpaceRelease(color_space) };
            return Err(CoreError::Io(format!(
                "无法创建 PDF 第 {page_number} 页位图上下文"
            )));
        }
        let target = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: width as f64,
                height: height as f64,
            },
        };
        // SAFETY: page/context are live; the system transform handles crop origin and page rotation.
        unsafe {
            let transform = CGPDFPageGetDrawingTransform(page, 0, target, 0, true);
            CGContextConcatCTM(context, transform);
            CGContextDrawPDFPage(context, page);
            CGContextRelease(context);
            CGColorSpaceRelease(color_space);
        }
        // CGBitmapContext's first row represents the lower edge; image expects the upper edge first.
        for top in 0..(height / 2) {
            let bottom = height - 1 - top;
            let top_start = top * width;
            let bottom_start = bottom * width;
            for offset in 0..width {
                pixels.swap(top_start + offset, bottom_start + offset);
            }
        }
        let image = GrayImage::from_raw(width as u32, height as u32, pixels)
            .ok_or_else(|| CoreError::Parse("PDF 视觉页像素无法编码".into()))?;
        let mut encoded = Vec::new();
        JpegEncoder::new_with_quality(&mut encoded, 88)
            .encode_image(&image)
            .map_err(|error| {
                CoreError::Io(format!("编码 PDF 第 {page_number} 页 JPEG 失败：{error}"))
            })?;
        Ok(encoded)
    }

    pub fn render_jpegs(path: &Path) -> CoreResult<Vec<Vec<u8>>> {
        const MAX_PAGES: usize = 40;
        let source = open_document(path)?;
        // SAFETY: source.document is a live CGPDFDocumentRef.
        let page_count = unsafe { CGPDFDocumentGetNumberOfPages(source.document) };
        if page_count == 0 {
            return Err(CoreError::Invalid("PDF 不包含可识别页面".into()));
        }
        if page_count > MAX_PAGES {
            return Err(CoreError::Invalid(format!(
                "答案 PDF 共 {page_count} 页，超过单次最多 {MAX_PAGES} 页"
            )));
        }
        (1..=page_count)
            .map(|page_number| render_visual_page(source.document, page_number))
            .collect()
    }
}

#[cfg(target_os = "macos")]
pub fn split_to_single_page_pdfs(path: &Path) -> CoreResult<Vec<Vec<u8>>> {
    macos::split(path)
}

#[cfg(not(target_os = "macos"))]
pub fn split_to_single_page_pdfs(_path: &Path) -> CoreResult<Vec<Vec<u8>>> {
    Err(CoreError::Invalid(
        "当前版本只在 macOS 支持 PDF 拆页，请改用 JPG/JPEG 上传".into(),
    ))
}

#[cfg(target_os = "macos")]
pub fn render_to_jpegs(path: &Path) -> CoreResult<Vec<Vec<u8>>> {
    macos::render_jpegs(path)
}

#[cfg(not(target_os = "macos"))]
pub fn render_to_jpegs(_path: &Path) -> CoreResult<Vec<Vec<u8>>> {
    Err(CoreError::Invalid(
        "当前版本只在 macOS 支持答案 PDF 视觉解析，请改用 JPG/JPEG 上传".into(),
    ))
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn two_page_pdf_fixture() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >>".to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 6 0 R >>".to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
    ];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0_usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    bytes.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.into_iter().skip(1) {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    bytes
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn splits_pdf_into_real_single_page_documents() {
        let root = std::env::temp_dir().join(format!("jiaofu-pdf-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("two-pages.pdf");
        std::fs::write(&source, two_page_pdf_fixture()).unwrap();
        let pages = split_to_single_page_pdfs(&source).unwrap();
        assert_eq!(pages.len(), 2);
        for (index, page) in pages.into_iter().enumerate() {
            let path = root.join(format!("single-{index}.pdf"));
            std::fs::write(&path, page).unwrap();
            let split_again = split_to_single_page_pdfs(&path).unwrap();
            assert_eq!(split_again.len(), 1);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn renders_pdf_pages_as_real_jpegs() {
        let root =
            std::env::temp_dir().join(format!("jiaofu-pdf-render-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("two-pages.pdf");
        std::fs::write(&source, two_page_pdf_fixture()).unwrap();
        let pages = render_to_jpegs(&source).unwrap();
        assert_eq!(pages.len(), 2);
        for page in pages {
            assert_eq!(&page[..2], &[0xff, 0xd8]);
            let image = image::load_from_memory(&page).unwrap();
            assert_eq!(image.width(), 1800);
            assert_eq!(image.height(), 1800);
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
