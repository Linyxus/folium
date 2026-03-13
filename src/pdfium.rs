use std::sync::OnceLock;

use pdfium_render::prelude::*;

struct SendSyncPdfium(Pdfium);
unsafe impl Send for SendSyncPdfium {}
unsafe impl Sync for SendSyncPdfium {}

pub fn get_pdfium() -> &'static Pdfium {
    static INSTANCE: OnceLock<SendSyncPdfium> = OnceLock::new();
    &INSTANCE
        .get_or_init(|| {
            let dylib = std::env::current_exe()
                .expect("cannot resolve executable path")
                .parent()
                .expect("executable has no parent directory")
                .join("libpdfium.dylib");
            SendSyncPdfium(Pdfium::new(
                Pdfium::bind_to_library(&dylib).unwrap_or_else(|e| {
                    panic!("failed to load {}: {e}", dylib.display())
                }),
            ))
        })
        .0
}
