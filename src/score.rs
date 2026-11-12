//! A PDF score shown as images, one per page, with PDFKit doing the rendering.
//!
//! The pages are part of the page they are shown in: they scroll with it, all of
//! them one under the other.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui;

/// Width of the rendered pages, in pixels.
const RENDER_W: f64 = 1600.0;

#[derive(Default)]
struct Job {
    pages: Vec<egui::ColorImage>,
    done: bool,
    error: Option<String>,
}

pub struct Score {
    pub path: PathBuf,
    job: Arc<Mutex<Job>>,
    textures: Vec<egui::TextureHandle>,
    done: bool,
    error: Option<String>,
}

impl Score {
    /// Starts rendering the pages in the background.
    pub fn open(path: &Path) -> Score {
        let job = Arc::new(Mutex::new(Job::default()));
        let out = job.clone();
        let p = path.to_path_buf();
        std::thread::spawn(move || {
            let r = render(&p, |img| out.lock().unwrap().pages.push(img));
            let mut j = out.lock().unwrap();
            j.done = true;
            j.error = r.err();
        });
        Score { path: path.to_path_buf(), job, textures: Vec::new(), done: false, error: None }
    }

    /// All the pages, as wide as `width`, one under the other.
    pub fn show(&mut self, ui: &mut egui::Ui, width: f32) {
        {
            let mut j = self.job.lock().unwrap();
            for img in j.pages.drain(..) {
                let name = format!("score-{}-{}", self.path.display(), self.textures.len());
                self.textures.push(ui.ctx().load_texture(name, img, egui::TextureOptions::LINEAR));
            }
            self.done = j.done;
            self.error = j.error.clone();
        }
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        for t in &self.textures {
            let size = t.size_vec2();
            let h = width * size.y / size.x;
            let (r, _) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
            ui.painter().image(t.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            ui.add_space(12.0);
        }
        if !self.done {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new("Préparation de la partition…").weak());
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

#[cfg(target_os = "macos")]
fn render(path: &Path, mut page_done: impl FnMut(egui::ColorImage)) -> Result<(), String> {
    use objc2::AllocAnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
    use objc2_foundation::{NSDictionary, NSSize, NSString, NSURL};
    use objc2_pdf_kit::{PDFDisplayBox, PDFDocument};

    let unreadable = || format!("Impossible de lire la partition {}", path.display());
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let doc = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }.ok_or_else(unreadable)?;
    for i in 0..unsafe { doc.pageCount() } {
        let img = objc2::rc::autoreleasepool(|_| -> Option<egui::ColorImage> {
            let page = unsafe { doc.pageAtIndex(i) }?;
            let b = unsafe { page.boundsForBox(PDFDisplayBox::CropBox) };
            // Pages turned a quarter are shown turned: their width is the box's height.
            let turned = unsafe { page.rotation() } % 180 != 0;
            let (w, h) = if turned { (b.size.height, b.size.width) } else { (b.size.width, b.size.height) };
            let size = NSSize::new(RENDER_W, RENDER_W * h / w.max(1.0));
            let image = unsafe { page.thumbnailOfSize_forBox(size, PDFDisplayBox::CropBox) };
            let tiff = image.TIFFRepresentation()?;
            let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
            let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
            let rgba = image::load_from_memory(&png.to_vec()).ok()?.to_rgba8();
            Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], &rgba))
        });
        page_done(img.ok_or_else(unreadable)?);
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn render(_path: &Path, _page_done: impl FnMut(egui::ColorImage)) -> Result<(), String> {
    Err("L'affichage des partitions PDF n'est disponible que sur macOS.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `TILL_TEST_PDF=/some/file.pdf cargo test score` renders every page of a real PDF.
    #[test]
    fn renders_pdf_pages() {
        let Ok(path) = std::env::var("TILL_TEST_PDF") else { return };
        let mut pages = Vec::new();
        render(Path::new(&path), |img| pages.push(img)).unwrap();
        assert!(!pages.is_empty());
        for p in &pages {
            assert_eq!(p.size[0], RENDER_W as usize);
            assert!(p.pixels.iter().any(|c| c.r() < 128), "page blanche");
        }
        eprintln!("{} pages, {:?}", pages.len(), pages[0].size);
    }
}
