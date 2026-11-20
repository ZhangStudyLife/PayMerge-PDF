#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 || args[1] != "--apply" {
        std::process::exit(2);
    }
    match paymerge_pdf::update::apply_plan(std::path::Path::new(&args[2])) {
        Ok(receipt) if receipt.success => {}
        Ok(receipt) => {
            eprintln!("{}", receipt.message);
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
