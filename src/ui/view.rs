use super::*;

const PAPER: Color32 = Color32::from_rgb(248, 246, 240);
const LIGHT: Color32 = Color32::from_rgb(241, 239, 232);
const LINE: Color32 = Color32::from_rgb(216, 211, 201);
const SEAL: Color32 = Color32::from_rgb(153, 61, 47);

fn brush_rule(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 5.0), egui::Sense::hover());
    let points = (0..15)
        .map(|i| {
            egui::pos2(
                rect.left() + rect.width() * i as f32 / 14.0,
                rect.center().y + (i as f32 * 1.7).sin() * 0.35,
            )
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(points, egui::Stroke::new(0.8, LINE)));
}

impl App {
    fn can_output(&self) -> bool {
        !self.merging
            && !self.extracting
            && self.images.len() == 2
            && self.config.configured()
            && self
                .data
                .as_ref()
                .is_some_and(|data| self.review.validated(data).is_ok())
    }
    fn open_configuration(&mut self) {
        self.draft = self.config.clone();
        self.draft_output = self.config.output_path_absolute.clone();
        self.show_config = true;
    }
    pub(super) fn panels(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(PAPER).inner_margin(30))
            .show(ui, |ui| {
                egui::Panel::bottom("output")
                    .exact_size(82.0)
                    .resizable(false)
                    .show_separator_line(false)
                    .frame(egui::Frame::new().fill(PAPER))
                    .show(ui, |ui| self.output_bar(ui));
                self.content(ui);
            });
    }
    pub(super) fn content(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("合票")
                    .font(egui::FontId::new(
                        40.0,
                        egui::FontFamily::Name("ink".into()),
                    ))
                    .color(ACCENT),
            );
            let (seal, _) = ui.allocate_exact_size(egui::vec2(19.0, 24.0), egui::Sense::hover());
            ui.painter().rect_stroke(
                seal.shrink(1.0),
                1.0,
                egui::Stroke::new(1.1, SEAL),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                seal.center(),
                egui::Align2::CENTER_CENTER,
                "合",
                egui::FontId::proportional(12.0),
                SEAL,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new(self.updates.badge()).size(13.0))
                            .frame(false),
                    )
                    .clicked()
                {
                    self.updates.open = true;
                    self.updates.check();
                }
                if ui
                    .add_enabled(
                        !self.merging,
                        egui::Button::new(RichText::new("设置").size(13.0)).frame(false),
                    )
                    .clicked()
                {
                    self.open_configuration();
                }
            });
        });
        ui.add_space(7.0);
        brush_rule(ui);
        ui.add_space(19.0);
        egui::ScrollArea::vertical()
            .id_salt("materials")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.merging,
                            egui::Button::new("添加").min_size(egui::vec2(68.0, 29.0)),
                        )
                        .clicked()
                        && let Some(paths) = rfd::FileDialog::new()
                            .set_title("添加材料")
                            .add_filter("材料", &["pdf", "png", "jpg", "jpeg"])
                            .pick_files()
                    {
                        self.add_files(paths);
                    }
                    if ui
                        .add_enabled(
                            !self.merging && (self.pdf.is_some() || !self.images.is_empty()),
                            egui::Button::new("清空").frame(false),
                        )
                        .clicked()
                    {
                        self.clear();
                        self.set_status("", false);
                    }
                    if self.extracting {
                        ui.spinner();
                    }
                });
                ui.add_space(12.0);
                ui.columns(3, |columns| {
                    let files = [
                        self.pdf.as_deref(),
                        self.images.first().map(PathBuf::as_path),
                        self.images.get(1).map(PathBuf::as_path),
                    ];
                    for (index, label) in ["发票", "购买记录", "支付记录"].into_iter().enumerate()
                    {
                        let ui = &mut columns[index];
                        egui::Frame::new()
                            .fill(LIGHT)
                            .inner_margin(16)
                            .corner_radius(2)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.set_min_height(98.0);
                                ui.label(RichText::new(label).color(MUTED).size(12.0));
                                ui.add_space(17.0);
                                if let Some(path) = files[index] {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(
                                                path.file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy(),
                                            )
                                            .size(14.0),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(path.display().to_string());
                                } else {
                                    ui.label(
                                        RichText::new("—")
                                            .size(24.0)
                                            .color(Color32::from_rgb(186, 181, 170)),
                                    );
                                }
                            });
                    }
                });
                ui.add_space(26.0);
                let enabled = self.data.is_some() && !self.extracting && !self.merging;
                ui.add_enabled_ui(enabled, |ui| {
                    ui.columns(2, |columns| {
                        let number =
                            columns[0].label(RichText::new("票号").size(12.0).color(MUTED));
                        columns[0]
                            .add(
                                egui::TextEdit::singleline(&mut self.review.number)
                                    .desired_width(f32::INFINITY)
                                    .margin(egui::vec2(10.0, 9.0))
                                    .hint_text("—"),
                            )
                            .labelled_by(number.id);
                        let amount =
                            columns[1].label(RichText::new("金额 / 元").size(12.0).color(MUTED));
                        columns[1]
                            .add(
                                egui::TextEdit::singleline(&mut self.review.amount)
                                    .desired_width(f32::INFINITY)
                                    .font(egui::FontId::proportional(19.0))
                                    .margin(egui::vec2(10.0, 7.0))
                                    .hint_text("—"),
                            )
                            .labelled_by(amount.id);
                    });
                });
                if let Some(data) = &self.data {
                    let uncertain = data.number.value().is_none() || data.amount.value().is_none();
                    if uncertain {
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new(if data.has_text {
                                "请核对金额与票号"
                            } else {
                                "图片发票，请补填金额与票号"
                            })
                            .size(12.0)
                            .color(SEAL),
                        );
                        ui.checkbox(&mut self.review.confirmed, "已核对原件");
                    }
                    ui.add_space(9.0);
                    ui.horizontal(|ui| {
                        if let Some(date) = data.date.value() {
                            ui.label(RichText::new(date).size(12.0).color(MUTED));
                        }
                        if let Some(seller) = data.seller.value() {
                            ui.add(
                                egui::Label::new(RichText::new(seller).size(12.0).color(MUTED))
                                    .truncate(),
                            )
                            .on_hover_text(seller);
                        }
                    });
                }
            });
    }
    fn output_bar(&mut self, ui: &mut egui::Ui) {
        brush_rule(ui);
        ui.add_space(11.0);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 164.0).max(200.0);
            ui.allocate_ui_with_layout(
                egui::vec2(width, 45.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    let preview = self
                        .data
                        .as_ref()
                        .and_then(|data| self.review.validated(data).ok())
                        .and_then(|(number, amount)| {
                            filename(&self.config.name, &self.config.group, amount, &number).ok()
                        });
                    if let Some(preview) = preview {
                        ui.add(egui::Label::new(RichText::new(preview).size(12.0)).truncate());
                    }
                    if self.error || self.merging || !self.status.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&self.status).size(12.0).color(if self.error {
                                    SEAL
                                } else {
                                    MUTED
                                }),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&self.status);
                    } else if self.config.configured() {
                        match self.config.output_directory(&self.app_dir) {
                            Ok(folder) => {
                                let label = if folder == self.app_dir {
                                    "当前目录".into()
                                } else {
                                    folder
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .into_owned()
                                };
                                ui.label(
                                    RichText::new(format!("→ {label}")).size(11.0).color(MUTED),
                                )
                                .on_hover_text(folder.display().to_string());
                            }
                            Err(_) => {
                                ui.label(RichText::new("保存位置无效").size(11.0).color(SEAL));
                            }
                        }
                    }
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.can_output(),
                        egui::Button::new(
                            RichText::new(if self.merging {
                                "合并中…"
                            } else {
                                "合并 PDF"
                            })
                            .color(PAPER)
                            .size(16.0),
                        )
                        .fill(ACCENT)
                        .corner_radius(2)
                        .min_size(egui::vec2(145.0, 44.0)),
                    )
                    .clicked()
                {
                    self.output();
                }
            });
        });
    }
}

pub(super) fn configure_context(context: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "noto-sc".into(),
        egui::FontData::from_owned(paymerge_pdf::bundle::font_data())
            .tweak(egui::FontTweak {
                coords: egui::epaint::text::VariationCoords::new([("wght", 400.0)]),
                ..Default::default()
            })
            .into(),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "noto-sc".into());
    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .push("noto-sc".into());
    fonts.font_data.insert(
        "ink".into(),
        egui::FontData::from_static(include_bytes!("../../assets/PayMergeInk.ttf")).into(),
    );
    fonts.families.insert(
        egui::FontFamily::Name("ink".into()),
        vec!["ink".into(), "noto-sc".into()],
    );
    context.set_fonts(fonts);
    let mut style = (*context.global_style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(ACCENT);
    style.visuals.panel_fill = PAPER;
    style.visuals.window_fill = PAPER;
    style.visuals.window_corner_radius = egui::CornerRadius::same(3);
    style.visuals.window_stroke = egui::Stroke::new(1.0, LINE);
    style.visuals.text_edit_bg_color = Some(LIGHT);
    style.visuals.selection.bg_fill = Color32::from_rgb(213, 208, 196);
    style.visuals.hyperlink_color = SEAL;
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.corner_radius = egui::CornerRadius::same(2);
    }
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(231, 227, 217);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(221, 216, 204);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(205, 198, 181);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(29.0, egui::FontFamily::Name("ink".into())),
    );
    context.set_global_style(style);
}

pub(super) fn notices_window(context: &egui::Context, open: &mut bool) {
    if !*open {
        return;
    }
    static LINES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let lines = LINES.get_or_init(|| {
        paymerge_pdf::bundle::notices()
            .unwrap_or_else(|error| error.to_string())
            .lines()
            .map(str::to_owned)
            .collect()
    });
    egui::Window::new("第三方许可")
        .open(open)
        .default_width(650.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(context, |ui| {
            egui::ScrollArea::both().max_height(420.0).show_rows(
                ui,
                16.0,
                lines.len(),
                |ui, range| {
                    for index in range {
                        ui.add(
                            egui::Label::new(RichText::new(&lines[index]).monospace().size(11.0))
                                .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    }
                },
            );
        });
}
