use std::sync::{Mutex, OnceLock};

use pdfium_render::prelude::*;

// Pdfium contains raw pointers and is not Send, but the Mutex ensures only
// one thread ever accesses it at a time.
struct PdfiumHolder(Pdfium);
unsafe impl Send for PdfiumHolder {}

static INSTANCE: OnceLock<Mutex<PdfiumHolder>> = OnceLock::new();

pub fn init_pdfium() {
    INSTANCE.get_or_init(|| {
        let dylib = std::env::current_exe()
            .expect("cannot resolve executable path")
            .parent()
            .expect("executable has no parent directory")
            .join("libpdfium.dylib");
        Mutex::new(PdfiumHolder(Pdfium::new(
            Pdfium::bind_to_library(&dylib).unwrap_or_else(|e| {
                panic!("failed to load {}: {e}", dylib.display())
            }),
        )))
    });
}

/// RAII guard that holds the pdfium lock.  Derefs to `&Pdfium`.
pub struct PdfiumGuard(std::sync::MutexGuard<'static, PdfiumHolder>);

impl std::ops::Deref for PdfiumGuard {
    type Target = Pdfium;
    // *self.0  -> PdfiumHolder (via MutexGuard::Deref)
    // **self.0 -> Pdfium       (via PdfiumHolder::Deref)
    fn deref(&self) -> &Pdfium { &self.0.0 }
}

impl std::ops::DerefMut for PdfiumGuard {
    fn deref_mut(&mut self) -> &mut Pdfium { &mut self.0.0 }
}

/// Acquire exclusive access to pdfium. Blocks until no other thread renders.
pub fn lock_pdfium() -> PdfiumGuard {
    PdfiumGuard(
        INSTANCE
            .get()
            .expect("pdfium not initialised — call init_pdfium() at startup")
            .lock()
            .unwrap(),
    )
}
