//! 使用 macOS CoreGraphics 将 PDF 拆为单页 PDF。
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

    use suite_core::error::{CoreError, CoreResult};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    type CFIndex = isize;
    type CFURLRef = *const c_void;
    type CGPDFDocumentRef = *const c_void;
    type CGPDFPageRef = *const c_void;
    type CGDataConsumerRef = *const c_void;
    type CGContextRef = *mut c_void;

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

#[cfg(test)]
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

#[cfg(test)]
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
}
