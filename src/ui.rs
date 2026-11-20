use eframe::egui::{self, Color32, RichText};
#[cfg(test)]
use paymerge_pdf::model::Recognition;
use paymerge_pdf::{
    config::{Config, numbered_path, validate_personal},
    extract::extract_invoice,
    merge::merge_invoice,
    model::{InvoiceData, Review, filename},
};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

mod update_view;
mod view;
use update_view::Updates;

const ACCENT: Color32 = Color32::from_rgb(45, 44, 40);
const MUTED: Color32 = Color32::from_rgb(116, 112, 104);

enum Work {
    Extract(PathBuf, u64),
    Merge {
        invoice: PathBuf,
        buy: PathBuf,
        pay: PathBuf,
        output: PathBuf,
        overwrite: bool,
    },
}

enum Completed {
    Extract(u64, paymerge_pdf::Result<InvoiceData>),
    Merge(PathBuf, paymerge_pdf::Result<usize>),
}

pub fn run(startup_status: Option<String>) -> eframe::Result {
    let app_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let data_dir = paymerge_pdf::bundle::user_data_dir()
        .map_err(|error| eframe::Error::AppCreation(Box::new(error)))?;
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([860.0, 650.0])
        .with_min_inner_size([760.0, 590.0]);
    if let Ok(icon) = image::load_from_memory_with_format(
        include_bytes!("../source/icon.ico"),
        image::ImageFormat::Ico,
    ) {
        let rgba = icon.into_rgba8();
        viewport = viewport.with_icon(egui::IconData {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        });
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "合票 · PayMerge",
        options,
        Box::new(move |cc| {
            let mut app = App::new(cc, app_dir, data_dir);
            if let Some(status) = startup_status {
                app.set_status(status.clone(), false);
                app.updates.notice = status;
            }
            Ok(Box::new(app))
        }),
    )
}

struct App {
    app_dir: PathBuf,
    data_dir: PathBuf,
    show_notices: bool,
    config: Config,
    draft: Config,
    draft_output: String,
    show_config: bool,
    pdf: Option<PathBuf>,
    images: Vec<PathBuf>,
    data: Option<InvoiceData>,
    review: Review,
    generation: u64,
    extracting: bool,
    merging: bool,
    status: String,
    error: bool,
    sender: mpsc::Sender<Work>,
    receiver: mpsc::Receiver<Completed>,
    updates: Updates,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, app_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self::create(&cc.egui_ctx, app_dir, data_dir)
    }

    fn create(egui_context: &egui::Context, app_dir: PathBuf, data_dir: PathBuf) -> Self {
        view::configure_context(egui_context);
        let (sender, worker) = mpsc::channel::<Work>();
        let (done, receiver) = mpsc::channel();
        let context = egui_context.clone();
        thread::spawn(move || {
            while let Ok(work) = worker.recv() {
                let completed = match work {
                    Work::Extract(path, generation) => {
                        Completed::Extract(generation, extract_invoice(&path))
                    }
                    Work::Merge {
                        invoice,
                        buy,
                        pay,
                        output,
                        overwrite,
                    } => {
                        let result = merge_invoice(&invoice, &buy, &pay, &output, overwrite);
                        Completed::Merge(output, result)
                    }
                };
                if done.send(completed).is_err() {
                    break;
                }
                context.request_repaint();
            }
        });
        let (config, status, error) = match Config::load_migrated(&app_dir, &data_dir) {
            Ok(config) => (config, String::new(), false),
            Err(error) => (Config::default(), error.to_string(), true),
        };
        Self {
            draft: config.clone(),
            draft_output: config.output_path_absolute.clone(),
            show_config: !config.configured(),
            app_dir,
            data_dir,
            show_notices: false,
            config,
            pdf: None,
            images: Vec::new(),
            data: None,
            review: Review::default(),
            generation: 0,
            extracting: false,
            merging: false,
            status,
            error,
            sender,
            receiver,
            updates: Updates::new(egui_context.clone()),
        }
    }

    fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.status = text.into();
        self.error = error;
    }

    fn clear(&mut self) {
        self.pdf = None;
        self.images.clear();
        self.data = None;
        self.review = Review::default();
        self.generation += 1;
        self.extracting = false;
    }

    fn add_files(&mut self, files: impl IntoIterator<Item = PathBuf>) {
        let mut queue: VecDeque<_> = files.into_iter().collect();
        while let Some(path) = queue.pop_front() {
            if !path.is_file() {
                self.set_status("所选文件不存在或无法读取", true);
                continue;
            }
            let extension = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if extension == "pdf" {
                if self.pdf.is_some() && !ask("替换 PDF", "已有发票 PDF，是否替换？") {
                    continue;
                }
                self.set_pdf(path);
            } else if ["jpg", "jpeg", "png"].contains(&extension.as_str()) {
                if self.images.len() == 2 {
                    if !ask(
                        "替换图片",
                        "已有两张图片，是否替换最早添加的一张？\n替换后请核对购买、支付记录的顺序。",
                    ) {
                        continue;
                    }
                    self.images.remove(0);
                }
                self.images.push(path);
            } else {
                self.set_status("支持 PDF、JPG、JPEG 和 PNG 文件", true);
            }
        }
    }

    fn set_pdf(&mut self, path: PathBuf) {
        self.pdf = Some(path.clone());
        self.data = None;
        self.review = Review::default();
        self.generation += 1;
        self.extracting = true;
        let _ = self.sender.send(Work::Extract(path, self.generation));
        self.set_status("识别中…", false);
    }

    fn completed(&mut self) {
        while let Ok(result) = self.receiver.try_recv() {
            match result {
                Completed::Extract(generation, result) if generation == self.generation => {
                    self.extracting = false;
                    match result {
                        Ok(data) => {
                            let uncertain =
                                data.number.value().is_none() || data.amount.value().is_none();
                            self.review = Review::from_invoice(&data);
                            self.set_status(
                                if uncertain {
                                    "请补填金额与票号"
                                } else {
                                    ""
                                },
                                false,
                            );
                            self.data = Some(data);
                        }
                        Err(error) => self.set_status(error.to_string(), true),
                    }
                }
                Completed::Extract(..) => {}
                Completed::Merge(path, result) => {
                    self.merging = false;
                    match result {
                        Ok(pages) => {
                            self.clear();
                            self.set_status(
                                format!("已生成 {pages} 页 PDF：{}", path.display()),
                                false,
                            );
                        }
                        Err(error) => self.set_status(error.to_string(), true),
                    }
                }
            }
        }
    }

    fn output(&mut self) {
        let result = (|| -> paymerge_pdf::Result<(PathBuf, bool)> {
            let data = self
                .data
                .as_ref()
                .ok_or_else(|| paymerge_pdf::message("请先选择有效的 PDF"))?;
            let (number, amount) = self.review.validated(data)?;
            let output = self.config.output_directory(&self.app_dir)?.join(filename(
                &self.config.name,
                &self.config.group,
                amount,
                &number,
            )?);
            if output.exists() {
                let response = rfd::MessageDialog::new()
                    .set_title("文件已存在")
                    .set_description(format!(
                        "已存在同名文件：\n{}\n\n“是”覆盖；“否”添加序号；“取消”停止输出。",
                        output.file_name().unwrap_or_default().to_string_lossy()
                    ))
                    .set_buttons(rfd::MessageButtons::YesNoCancel)
                    .show();
                match response {
                    rfd::MessageDialogResult::Yes => Ok((output, true)),
                    rfd::MessageDialogResult::No => Ok((numbered_path(&output), false)),
                    _ => Err(paymerge_pdf::message("已取消输出")),
                }
            } else {
                Ok((output, false))
            }
        })();
        match result {
            Ok((output, overwrite)) => {
                let Some(invoice) = self.pdf.clone() else {
                    return;
                };
                if self.images.len() != 2 {
                    return;
                }
                self.merging = true;
                self.set_status("正在合并 PDF…", false);
                let _ = self.sender.send(Work::Merge {
                    invoice,
                    buy: self.images[0].clone(),
                    pay: self.images[1].clone(),
                    output,
                    overwrite,
                });
            }
            Err(error) => self.set_status(error.to_string(), true),
        }
    }

    fn configuration(&mut self, context: &egui::Context) {
        if !self.show_config {
            return;
        }
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new("设置")
            .open(&mut open)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .frame(egui::Frame::window(&context.global_style()).inner_margin(24))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(440.0)
            .show(context, |ui| {
                ui.heading("设置");
                ui.add_space(14.0);
                ui.columns(2, |columns| {
                    let name = columns[0].label(RichText::new("姓名").size(12.0).color(MUTED));
                    columns[0]
                        .add(
                            egui::TextEdit::singleline(&mut self.draft.name)
                                .desired_width(f32::INFINITY)
                                .margin(egui::vec2(9.0, 8.0)),
                        )
                        .labelled_by(name.id);
                    let group = columns[1].label(RichText::new("组别").size(12.0).color(MUTED));
                    columns[1]
                        .add(
                            egui::TextEdit::singleline(&mut self.draft.group)
                                .desired_width(f32::INFINITY)
                                .margin(egui::vec2(9.0, 8.0)),
                        )
                        .labelled_by(group.id);
                });
                ui.add_space(8.0);
                let output = ui.label(RichText::new("保存位置").size(12.0).color(MUTED));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.draft_output)
                            .desired_width(330.0)
                            .margin(egui::vec2(9.0, 8.0))
                            .hint_text("当前目录"),
                    )
                    .labelled_by(output.id);
                    if ui.button("浏览").clicked()
                        && let Some(folder) =
                            rfd::FileDialog::new().set_title("保存位置").pick_folder()
                    {
                        self.draft_output = folder.to_string_lossy().into_owned();
                    }
                });
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    save = ui
                        .add(
                            egui::Button::new(
                                RichText::new("保存").color(Color32::from_rgb(248, 246, 240)),
                            )
                            .fill(ACCENT)
                            .min_size(egui::vec2(86.0, 35.0)),
                        )
                        .clicked();
                    cancel = ui.add(egui::Button::new("取消").frame(false)).clicked();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(RichText::new("许可").size(11.0).color(MUTED))
                                    .frame(false),
                            )
                            .clicked()
                        {
                            self.show_notices = true;
                        }
                    });
                });
            });
        if !open || cancel {
            self.show_config = false;
        }
        if save {
            let result = (|| -> paymerge_pdf::Result<()> {
                self.draft.name = self.draft.name.trim().to_owned();
                self.draft.group = self.draft.group.trim().to_owned();
                validate_personal(&self.draft.name, "姓名")?;
                validate_personal(&self.draft.group, "组别")?;
                self.draft
                    .set_output(self.draft_output.trim(), &self.app_dir);
                if !self.draft.output_path_absolute.is_empty()
                    && !Path::new(&self.draft.output_path_absolute).is_dir()
                {
                    if !ask("创建输出目录", "输出目录不存在，是否创建？") {
                        return Err(paymerge_pdf::message("已取消创建目录"));
                    }
                    std::fs::create_dir_all(&self.draft.output_path_absolute)?;
                }
                std::fs::create_dir_all(&self.data_dir)?;
                self.draft.save(&self.data_dir)?;
                Ok(())
            })();
            match result {
                Ok(()) => {
                    self.config = self.draft.clone();
                    self.show_config = false;
                    self.set_status("配置已保存", false);
                }
                Err(error) => self.set_status(error.to_string(), true),
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.completed();
        self.updates.poll();
        if !cfg!(test) && self.updates.due() {
            self.updates.check();
        }
        let context = ui.ctx().clone();
        if self.merging && context.input(|input| input.viewport().close_requested()) {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if !self.merging && !self.show_config {
            let files = context.input(|input| {
                input
                    .raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .collect::<Vec<_>>()
            });
            if !files.is_empty() {
                self.add_files(files);
            }
        }
        self.panels(ui);
        self.configuration(&context);
        view::notices_window(&context, &mut self.show_notices);
        let pending =
            self.pdf.is_some() || !self.images.is_empty() || self.extracting || self.merging;
        if self.updates.dialog(pending) {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.extracting || self.merging || self.updates.checking || self.updates.downloading {
            context.request_repaint_after(Duration::from_millis(100));
        } else {
            context.request_repaint_after(Duration::from_secs(60));
        }
    }
}

fn ask(title: &str, message: &str) -> bool {
    rfd::MessageDialog::new()
        .set_title(title)
        .set_description(message)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
}

#[cfg(test)]
fn field_status<T>(field: &Recognition<T>) -> &'static str {
    match field {
        Recognition::Recognized(_) => "已识别",
        Recognition::Missing => "未识别",
        Recognition::Conflict(_) => "存在冲突，请核对",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paymerge_pdf::model::Money;

    fn invoice() -> InvoiceData {
        InvoiceData {
            page_count: 1,
            has_text: true,
            number: Recognition::Recognized("00000000000000001234".into()),
            amount: Recognition::Recognized(Money(1230)),
            date: Recognition::Recognized("2026-11-20".into()),
            seller: Recognition::Recognized("测试电子有限公司".into()),
        }
    }

    fn app(directory: &Path) -> (App, mpsc::Receiver<Work>, mpsc::Sender<Completed>) {
        let mut app = App::create(
            &egui::Context::default(),
            directory.to_owned(),
            directory.join("settings"),
        );
        let (sender, work) = mpsc::channel();
        let (done, receiver) = mpsc::channel();
        app.sender = sender;
        app.receiver = receiver;
        (app, work, done)
    }

    #[test]
    fn first_configuration_and_reload_use_existing_json_keys() {
        let directory = tempfile::tempdir().unwrap();
        let (first, _, _) = app(directory.path());
        assert!(first.show_config && !first.config.configured());
        let config = Config {
            name: "测试者".into(),
            group: "测试组".into(),
            ..Default::default()
        };
        config.save(directory.path()).unwrap();
        let (second, _, _) = app(directory.path());
        assert!(!second.show_config);
        assert_eq!(second.config.name, "测试者");
    }

    #[test]
    fn selected_files_keep_order_and_pdf_replacement_resets_manual_values() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, work, _) = app(directory.path());
        let paths =
            ["发票 文件.PDF", "购买记录.png", "支付记录.jpg"].map(|s| directory.path().join(s));
        for path in &paths {
            std::fs::write(path, b"fixture").unwrap();
        }
        app.add_files(paths.clone());
        assert_eq!(app.images, paths[1..]);
        assert!(matches!(work.recv().unwrap(), Work::Extract(_, 1)));
        app.review.number = "12345678".into();
        app.review.amount = "999".into();
        app.review.confirmed = true;
        app.data = Some(invoice());
        app.set_pdf(directory.path().join("另一张.pdf"));
        assert!(
            app.data.is_none()
                && app.review.number.is_empty()
                && app.review.amount.is_empty()
                && !app.review.confirmed
        );
        assert!(matches!(work.recv().unwrap(), Work::Extract(_, 2)));
        app.clear();
        assert!(app.pdf.is_none() && app.images.is_empty() && !app.extracting);
    }

    #[test]
    fn late_extraction_does_not_restore_a_cleared_invoice() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _, done) = app(directory.path());
        app.generation = 5;
        app.extracting = true;
        app.clear();
        done.send(Completed::Extract(5, Ok(invoice()))).unwrap();
        app.completed();
        assert!(app.data.is_none() && app.review.number.is_empty() && !app.extracting);
        app.pdf = Some(directory.path().join("current.pdf"));
        app.extracting = true;
        done.send(Completed::Extract(
            6,
            Err(paymerge_pdf::message("损坏的 PDF")),
        ))
        .unwrap();
        app.completed();
        assert!(app.pdf.is_some() && !app.extracting && app.error);
    }

    #[test]
    fn output_uses_reviewed_fields_and_failure_keeps_inputs() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, work, done) = app(directory.path());
        app.config = Config {
            name: "测试者".into(),
            group: "测试组".into(),
            ..Default::default()
        };
        app.pdf = Some(directory.path().join("invoice.pdf"));
        app.images = vec![
            directory.path().join("buy.png"),
            directory.path().join("pay.png"),
        ];
        app.data = Some(invoice());
        app.review = Review::from_invoice(app.data.as_ref().unwrap());
        app.review.amount = "23.45".into();
        app.output();
        match work.recv().unwrap() {
            Work::Merge {
                buy,
                pay,
                output,
                overwrite,
                ..
            } => {
                assert!(buy.ends_with("buy.png") && pay.ends_with("pay.png") && !overwrite);
                assert_eq!(output.file_name().unwrap(), "测试者_测试组_23.45_1234.pdf");
                done.send(Completed::Merge(
                    output,
                    Err(paymerge_pdf::message("文件被占用")),
                ))
                .unwrap();
            }
            _ => panic!("expected merge request"),
        }
        app.completed();
        assert!(app.pdf.is_some() && app.images.len() == 2 && app.error && !app.merging);
        assert_eq!(app.review.amount, "23.45");
    }

    #[test]
    fn headless_layout_handles_recognized_missing_and_conflicting_fields() {
        let directory = tempfile::tempdir().unwrap();
        let context = egui::Context::default();
        let mut app = App::create(
            &context,
            directory.path().to_owned(),
            directory.path().join("settings"),
        );
        app.show_config = false;
        for state in 0..3 {
            let mut data = invoice();
            if state == 1 {
                data.has_text = false;
                data.amount = Recognition::Missing;
                data.number = Recognition::Missing;
            }
            if state == 2 {
                data.amount = Recognition::Conflict(vec![Money(1230), Money(4560)]);
            }
            app.review = Review::from_invoice(&data);
            app.data = Some(data);
            let mut result = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(920.0, 820.0),
                    )),
                    ..Default::default()
                },
                |ui| app.content(ui),
            );
            let has_shapes = !result.shapes.is_empty();
            // No GPU renderer exists in this test, so consume its font texture delta explicitly.
            result.textures_delta.clear();
            assert!(has_shapes);
            assert_eq!(
                field_status(&app.data.as_ref().unwrap().amount),
                ["已识别", "未识别", "存在冲突，请核对"][state]
            );
        }
        #[cfg(windows)]
        if let Some(directory) = std::env::var_os("PAYMERGE_PREVIEW_DIR") {
            export_previews(Path::new(&directory));
        }
    }

    #[cfg(windows)]
    fn export_previews(output: &Path) {
        use egui_kittest::Harness;
        std::fs::create_dir_all(output).unwrap();
        for (name, ready, settings) in [
            ("workspace-empty", false, false),
            ("workspace-ready", true, false),
            ("settings", false, true),
        ] {
            let directory = tempfile::tempdir().unwrap();
            std::fs::create_dir(directory.path().join("报销材料")).unwrap();
            let mut app = App::create(
                &egui::Context::default(),
                directory.path().to_owned(),
                directory.path().join("settings"),
            );
            app.config = Config {
                name: "示例使用者".into(),
                group: "实验组A".into(),
                ..Default::default()
            };
            app.config.set_output(
                &directory.path().join("报销材料").display().to_string(),
                directory.path(),
            );
            app.draft = app.config.clone();
            app.draft_output = "报销材料".into();
            app.show_config = settings;
            app.updates.notice = "已是最新公开版本".into();
            if ready {
                app.pdf = Some(PathBuf::from("电子发票_示例.pdf"));
                app.images = vec![
                    PathBuf::from("购买记录_示例.png"),
                    PathBuf::from("支付记录_示例.png"),
                ];
                app.data = Some(invoice());
                app.review = Review::from_invoice(app.data.as_ref().unwrap());
                app.status = "".into();
            }
            let mut harness = Harness::builder()
                .with_size(egui::vec2(860.0, 650.0))
                .with_context_setup(view::configure_context)
                .build_ui_state(
                    |ui, app: &mut App| {
                        app.panels(ui);
                        let context = ui.ctx().clone();
                        app.configuration(&context);
                    },
                    app,
                );
            harness.run();
            harness
                .render()
                .unwrap()
                .save(output.join(format!("{name}.png")))
                .unwrap();
        }
    }
}
