use std::sync::Arc;

#[derive(Debug)]
pub struct PdfStateData {
    pub bytes: Arc<Vec<u8>>,
    pub page_count: usize,
    pub current_page: usize,
    pub scale: f32,
}

/// Plain Rust data returned from the background render thread.
pub struct RenderResult {
    pub rgba: Vec<u8>,
    pub px_w: usize,
    pub px_h: usize,
    pub pt_w: f64,
    pub pt_h: f64,
    pub scale: f32,
    pub page: usize,
    pub page_count: usize,
}
