use crate::{Result, message, pdf};
use image::{
    DynamicImage, Rgb, RgbImage,
    imageops::{self, FilterType},
};
use pdfium_render::prelude::*;
use std::path::Path;

const WIDTH: u32 = 2480;
const HEIGHT: u32 = 3508;
const MARGIN: u32 = 118;
const GAP: u32 = 35;
const CONTENT_WIDTH: u32 = WIDTH - 2 * MARGIN;
const HALF_HEIGHT: u32 = (HEIGHT - 2 * MARGIN) / 2;

fn white_rgb(image: DynamicImage) -> RgbImage {
    let rgba = image.into_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let a = u32::from(p[3]);
        Rgb(std::array::from_fn(|i| {
            ((u32::from(p[i]) * a + 255 * (255 - a) + 127) / 255) as u8
        }))
    })
}

fn fit(image: &RgbImage, width: u32, height: u32) -> RgbImage {
    let scale = (width as f64 / image.width() as f64).min(height as f64 / image.height() as f64);
    imageops::resize(
        image,
        ((image.width() as f64 * scale) as u32).max(1),
        ((image.height() as f64 * scale) as u32).max(1),
        FilterType::Lanczos3,
    )
}

fn centered(canvas: &mut RgbImage, image: &RgbImage, x: u32, y: u32, width: u32, height: u32) {
    let fitted = fit(image, width, height);
    imageops::replace(
        canvas,
        &fitted,
        i64::from(x + (width - fitted.width()) / 2),
        i64::from(y + (height - fitted.height()) / 2),
    );
}

fn orient(image: RgbImage) -> RgbImage {
    let w = image.width() as f64;
    let h = image.height() as f64;
    let target_w = CONTENT_WIDTH as f64;
    let target_h = (HALF_HEIGHT - GAP) as f64;
    let normal = (target_w / w).min(target_h / h);
    let rotated = (target_w / h).min(target_h / w);
    if rotated > normal {
        imageops::rotate270(&image)
    } else {
        image
    }
}

pub fn output_page_count(invoice_pages: usize) -> usize {
    invoice_pages / 2 + 1
}

/// The final invoice page shares its A4 sheet with the two screenshots, as in v6.1.
pub fn merge_invoice(
    invoice: &Path,
    purchase: &Path,
    payment: &Path,
    output: &Path,
    overwrite: bool,
) -> Result<usize> {
    if let Ok(destination) = output.canonicalize() {
        for input in [invoice, purchase, payment] {
            if input.canonicalize().ok().as_ref() == Some(&destination) {
                return Err(message("输出文件与输入文件相同，请选择其他输出目录"));
            }
        }
    }
    let parent = output.parent().ok_or_else(|| message("输出文件路径无效"))?;
    if !parent.is_dir() {
        return Err(message("输出目录不存在"));
    }
    let engine = pdf::engine()?;
    let source = engine.load_pdf_from_file(invoice, None)?;
    let count = source.pages().len() as usize;
    if count == 0 {
        return Err(message("PDF 没有页面"));
    }
    let buy = white_rgb(image::open(purchase)?);
    let pay = white_rgb(image::open(payment)?);
    let mut document = engine.create_new_pdf()?;
    let mut index = 0;
    let mut created = 0;
    while index < count {
        let final_sheet = index == count - 1;
        let mut canvas = RgbImage::from_pixel(WIDTH, HEIGHT, Rgb([255, 255, 255]));
        let render = |page_index: usize| -> Result<RgbImage> {
            let page = source.pages().get(page_index as PdfPageIndex)?;
            let width = (page.width().value * 300.0 / 72.0).round() as i32;
            let bitmap =
                page.render_with_config(&PdfRenderConfig::new().set_target_width(width))?;
            Ok(orient(white_rgb(bitmap.as_image()?)))
        };
        centered(
            &mut canvas,
            &render(index)?,
            MARGIN,
            MARGIN,
            CONTENT_WIDTH,
            HALF_HEIGHT - GAP,
        );
        index += 1;
        if final_sheet {
            let quarter = CONTENT_WIDTH / 2;
            centered(
                &mut canvas,
                &buy,
                MARGIN,
                MARGIN + HALF_HEIGHT,
                quarter - GAP / 2,
                HALF_HEIGHT - GAP,
            );
            centered(
                &mut canvas,
                &pay,
                MARGIN + quarter + GAP / 2,
                MARGIN + HALF_HEIGHT,
                quarter - GAP / 2,
                HALF_HEIGHT - GAP,
            );
        } else if index < count - 1 {
            centered(
                &mut canvas,
                &render(index)?,
                MARGIN,
                MARGIN + HALF_HEIGHT,
                CONTENT_WIDTH,
                HALF_HEIGHT - GAP,
            );
            index += 1;
        }
        let image = PdfPageImageObject::new_with_size(
            &document,
            &DynamicImage::ImageRgb8(canvas),
            PdfPoints::from_mm(210.0),
            PdfPoints::from_mm(297.0),
        )?;
        let mut page = document
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::a4())?;
        page.objects_mut().add_image_object(image)?;
        created += 1;
    }
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    document.save_to_file(temp.path())?;
    if overwrite {
        temp.persist(output).map_err(|e| e.error)?;
    } else {
        temp.persist_noclobber(output).map_err(|e| e.error)?;
    }
    Ok(created)
}
