//! Development-only runner. It is never included in the release package.
use paymerge_pdf::{
    extract::{extract_invoice, read_pages},
    merge::merge_invoice,
    pdf,
};
use pdfium_render::prelude::*;
use serde_json::json;
use std::{env, fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("extract-dir") if args.len() == 4 => {
            let mut paths: Vec<_> = fs::read_dir(&args[2])?.map(|p| p.map(|p|p.path())).collect::<std::io::Result<_>>()?;
            paths.retain(|p|p.extension().is_some_and(|s|s.eq_ignore_ascii_case("pdf")));
            paths.sort();
            let mut results = Vec::new();
            for (index,path) in paths.iter().enumerate() {
                results.push(json!({"id":index+1,"file":path.file_name().map(|p| p.to_string_lossy().into_owned()),"data":extract_invoice(path)?}));
            }
            fs::write(&args[3],serde_json::to_vec_pretty(&results)?)?;
            println!("{} invoices extracted",results.len());
        }
        Some("dump") if args.len() == 4 => {
            fs::write(&args[3],serde_json::to_vec_pretty(&read_pages(Path::new(&args[2]))?)?)?;
        }
        Some("merge") if args.len() == 6 || args.len() == 7 => {
            let pages=merge_invoice(Path::new(&args[2]),Path::new(&args[3]),Path::new(&args[4]),Path::new(&args[5]),args.get(6).is_some_and(|s|s=="overwrite"))?;
            println!("merged_pages={pages}");
        }
        Some("inspect") if args.len() >= 3 => {
            let doc=pdf::engine()?.load_pdf_from_file(&args[2],None)?;
            let mut dimensions=Vec::new();
            for (index,page) in doc.pages().iter().enumerate() {
                dimensions.push(json!([page.width().value,page.height().value]));
                if let Some(directory)=args.get(3) {
                    fs::create_dir_all(directory)?;
                    page.render_with_config(&PdfRenderConfig::new().set_target_width(1240))?.as_image()?.save(Path::new(directory).join(format!("page-{}.png",index+1)))?;
                }
            }
            println!("{}",json!({"pages":dimensions}));
        }
        _ => return Err("Usage: acceptance extract-dir DIR JSON | dump PDF JSON | merge PDF BUY PAY OUTPUT [overwrite] | inspect PDF [PNG_DIR]".into()),
    }
    Ok(())
}
