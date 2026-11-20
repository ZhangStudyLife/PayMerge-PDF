use super::*;
use paymerge_pdf::update::{self, ReleaseInfo, StagedUpdate};
use std::time::Instant;

enum Event {
    Checked(paymerge_pdf::Result<Option<ReleaseInfo>>),
    Progress(u64, u64),
    Downloaded(paymerge_pdf::Result<StagedUpdate>),
}

pub(super) struct Updates {
    sender: mpsc::Sender<Event>,
    receiver: mpsc::Receiver<Event>,
    context: egui::Context,
    last_check: Option<Instant>,
    pub checking: bool,
    pub downloading: bool,
    pub available: Option<ReleaseInfo>,
    pub staged: Option<StagedUpdate>,
    pub notice: String,
    pub open: bool,
    progress: (u64, u64),
}

impl Updates {
    pub fn new(context: egui::Context) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            context,
            last_check: None,
            checking: false,
            downloading: false,
            available: None,
            staged: None,
            notice: "自动检查 GitHub Releases".into(),
            open: false,
            progress: (0, 0),
        }
    }

    pub fn due(&self) -> bool {
        !self.checking
            && !self.downloading
            && self.staged.is_none()
            && self
                .last_check
                .is_none_or(|time| time.elapsed() >= update::CHECK_INTERVAL)
    }

    pub fn check(&mut self) {
        if self.checking || self.downloading || self.staged.is_some() {
            return;
        }
        self.checking = true;
        self.last_check = Some(Instant::now());
        self.notice = "正在检查更新…".into();
        let sender = self.sender.clone();
        let context = self.context.clone();
        thread::spawn(move || {
            let _ = sender.send(Event::Checked(update::check_latest(
                update::CURRENT_VERSION,
            )));
            context.request_repaint();
        });
    }

    fn download(&mut self, app_dir: &Path) {
        let Some(release) = self.available.clone() else {
            return;
        };
        self.downloading = true;
        self.notice = "正在下载更新…".into();
        self.progress = (0, release.size);
        let app_dir = app_dir.to_owned();
        let sender = self.sender.clone();
        let context = self.context.clone();
        thread::spawn(move || {
            let result = update::download_update(&release, &app_dir, |done, total| {
                let _ = sender.send(Event::Progress(done, total));
                context.request_repaint();
            });
            let _ = sender.send(Event::Downloaded(result));
            context.request_repaint();
        });
    }

    pub fn poll(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Checked(result) => {
                    self.checking = false;
                    match result {
                        Ok(Some(release)) => {
                            self.notice = format!("发现新版 v{}", release.version);
                            self.available = Some(release);
                        }
                        Ok(None) => {
                            self.notice = "已是最新公开版本".into();
                            self.available = None;
                        }
                        Err(error) => self.notice = error.to_string(),
                    }
                }
                Event::Progress(done, total) => self.progress = (done, total),
                Event::Downloaded(result) => {
                    self.downloading = false;
                    match result {
                        Ok(staged) => {
                            self.notice = "更新已就绪，重启后完成升级".into();
                            self.staged = Some(staged);
                        }
                        Err(error) => self.notice = error.to_string(),
                    }
                }
            }
        }
    }

    pub fn badge(&self) -> &str {
        if self.checking {
            "检查中"
        } else if self.downloading {
            "下载中"
        } else if self.staged.is_some() {
            "更新就绪"
        } else if self.available.is_some() {
            "有新版本"
        } else {
            "检查更新"
        }
    }

    pub fn dialog(&mut self, app_dir: &Path, has_pending_files: bool) -> bool {
        if !self.open {
            return false;
        }
        let mut open = true;
        let mut check = false;
        let mut download = false;
        let mut install = false;
        egui::Window::new("版本更新")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(500.0)
            .show(&self.context, |ui| {
                ui.label(
                    RichText::new(format!("PayMerge · v{}", update::CURRENT_VERSION))
                        .size(22.0)
                        .strong(),
                );
                ui.label(RichText::new(&self.notice).color(MUTED));
                ui.add_space(10.0);
                if let Some(release) = &self.available {
                    ui.label(
                        RichText::new(format!("新版 v{}", release.version))
                            .color(ACCENT)
                            .strong(),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(230.0)
                        .show(ui, |ui| {
                            ui.label(&release.notes);
                        });
                }
                if self.downloading {
                    let (done, total) = self.progress;
                    let fraction = if total == 0 {
                        0.0
                    } else {
                        done as f32 / total as f32
                    };
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .show_percentage()
                            .text(format!(
                                "{:.1} / {:.1} MB",
                                done as f64 / 1_000_000.0,
                                total as f64 / 1_000_000.0
                            )),
                    );
                }
                ui.add_space(8.0);
                ui.label("更新仅替换程序文件，保留姓名、组别、输出目录及已生成的 PDF。");
                if self.staged.is_some() && has_pending_files {
                    ui.colored_label(
                        Color32::from_rgb(155, 99, 24),
                        "还有待处理的材料，请先输出或清除，再重启升级。",
                    );
                }
                ui.horizontal(|ui| {
                    check = ui
                        .add_enabled(
                            !self.checking && !self.downloading && self.staged.is_none(),
                            egui::Button::new("检查更新"),
                        )
                        .clicked();
                    if self.staged.is_some() {
                        install = ui
                            .add_enabled(
                                !has_pending_files,
                                egui::Button::new(
                                    RichText::new("重启并升级").color(Color32::WHITE),
                                )
                                .fill(ACCENT),
                            )
                            .clicked();
                    } else if self.available.is_some() {
                        download = ui
                            .add_enabled(
                                !self.downloading,
                                egui::Button::new(RichText::new("下载更新").color(Color32::WHITE))
                                    .fill(ACCENT),
                            )
                            .clicked();
                    }
                    ui.hyperlink_to(
                        "GitHub Releases",
                        format!("{}/releases", update::REPOSITORY_URL),
                    );
                });
                ui.label(
                    RichText::new("启动时检查，运行期间每 6 小时检查一次。")
                        .size(12.0)
                        .color(MUTED),
                );
            });
        self.open = open;
        if check {
            self.check();
        }
        if download {
            self.download(app_dir);
        }
        if install && let Some(staged) = self.staged.take() {
            match update::launch_installer(staged, std::process::id(), true) {
                Ok(_) => return true,
                Err(error) => self.notice = format!("启动升级程序失败：{error}"),
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn update_failure_does_not_loop_or_block_invoice_work() {
        let mut updates = Updates::new(egui::Context::default());
        updates.last_check = Some(Instant::now());
        updates.checking = true;
        updates
            .sender
            .send(Event::Checked(Err(paymerge_pdf::message(
                "network unavailable",
            ))))
            .unwrap();
        updates.poll();
        assert!(!updates.checking && !updates.due() && updates.available.is_none());
        assert!(updates.notice.contains("network unavailable"));
    }
}
