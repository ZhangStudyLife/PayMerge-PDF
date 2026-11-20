use super::*;
use paymerge_pdf::update;

const INK: Color32 = Color32::from_rgb(28, 38, 51);
const PAPER: Color32 = Color32::from_rgb(247, 246, 242);
const LINE: Color32 = Color32::from_rgb(225, 229, 230);
const AMBER: Color32 = Color32::from_rgb(162, 104, 28);

fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, LINE))
        .inner_margin(18)
        .corner_radius(14)
}

fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.10))
        .corner_radius(100)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(12.0).strong());
        });
}

fn section(ui: &mut egui::Ui, number: &str, title: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(number).monospace().color(ACCENT).size(14.0));
        ui.label(RichText::new(title).size(18.0).strong());
    });
}

fn file_icon(ui: &mut egui::Ui, color: Color32, image: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 40.0), egui::Sense::hover());
    let bounds = rect.shrink2(egui::vec2(5.0, 3.0));
    let painter = ui.painter();
    painter.rect_filled(bounds, 5.0, color.gamma_multiply(0.10));
    painter.rect_stroke(
        bounds,
        5.0,
        egui::Stroke::new(1.4, color),
        egui::StrokeKind::Inside,
    );
    if image {
        painter.circle_filled(bounds.min + egui::vec2(8.0, 9.0), 2.5, color);
        painter.line_segment(
            [
                bounds.min + egui::vec2(5.0, 26.0),
                bounds.min + egui::vec2(12.0, 18.0),
            ],
            egui::Stroke::new(1.5, color),
        );
        painter.line_segment(
            [
                bounds.min + egui::vec2(12.0, 18.0),
                bounds.min + egui::vec2(22.0, 27.0),
            ],
            egui::Stroke::new(1.5, color),
        );
    } else {
        for y in [12.0, 19.0, 26.0] {
            painter.line_segment(
                [
                    bounds.min + egui::vec2(6.0, y),
                    bounds.min + egui::vec2(20.0, y),
                ],
                egui::Stroke::new(1.5, color),
            );
        }
    }
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
        egui::Panel::left("navigation")
            .exact_size(204.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(INK).inner_margin(22))
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(PAPER).inner_margin(26))
            .show(ui, |ui| self.content(ui));
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().visuals.override_text_color = Some(Color32::from_rgb(218, 225, 233));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            egui::Frame::new()
                .fill(ACCENT)
                .corner_radius(9)
                .inner_margin(egui::Margin::symmetric(11, 5))
                .show(ui, |ui| {
                    ui.label(RichText::new("P").color(Color32::WHITE).strong().size(22.0));
                });
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("PAYMERGE")
                        .size(17.0)
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.label(
                    RichText::new("ASC 实验室")
                        .size(12.0)
                        .color(Color32::from_rgb(140, 156, 171)),
                );
            });
        });
        ui.add_space(36.0);
        ui.label(
            RichText::new("报销材料工作台")
                .size(12.0)
                .color(Color32::from_rgb(130, 147, 162)),
        );
        ui.add_space(18.0);
        let step = if self.can_output() {
            2
        } else if self.pdf.is_some() {
            1
        } else {
            0
        };
        for (index, (number, title, description)) in [
            ("01", "添加材料", "发票 · 购买 · 支付"),
            ("02", "核对信息", "确认金额与发票号码"),
            ("03", "生成 PDF", "整理后即可提交报销"),
        ]
        .into_iter()
        .enumerate()
        {
            let active = index == step;
            egui::Frame::new()
                .fill(if active {
                    Color32::from_rgb(44, 61, 70)
                } else {
                    Color32::TRANSPARENT
                })
                .inner_margin(10)
                .corner_radius(9)
                .show(ui, |ui| {
                    ui.set_min_width(138.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(number)
                                .monospace()
                                .size(13.0)
                                .color(if active {
                                    Color32::from_rgb(108, 220, 182)
                                } else {
                                    Color32::from_rgb(119, 137, 153)
                                }),
                        );
                        ui.label(RichText::new(title).size(15.0).strong().color(if active {
                            Color32::WHITE
                        } else {
                            Color32::from_rgb(185, 198, 209)
                        }));
                    });
                    ui.label(
                        RichText::new(description)
                            .size(11.0)
                            .color(Color32::from_rgb(141, 159, 171)),
                    );
                });
            ui.add_space(8.0);
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.label(
                RichText::new("票据始终在本机处理")
                    .size(11.0)
                    .color(Color32::from_rgb(140, 156, 171)),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("v{}", update::CURRENT_VERSION))
                        .monospace()
                        .size(12.0)
                        .color(Color32::from_rgb(140, 156, 171)),
                );
                ui.hyperlink_to("GitHub", update::REPOSITORY_URL);
            });
            if ui
                .add_sized(
                    [160.0, 34.0],
                    egui::Button::new(
                        RichText::new(self.updates.badge()).color(Color32::from_rgb(210, 234, 224)),
                    )
                    .fill(Color32::from_rgb(41, 59, 67)),
                )
                .clicked()
            {
                self.updates.open = true;
                self.updates.check();
            }
            ui.add_space(14.0);
            if ui
                .add_enabled(
                    !self.merging,
                    egui::Button::new(
                        RichText::new("使用者与保存位置").color(Color32::from_rgb(210, 221, 231)),
                    )
                    .fill(Color32::from_rgb(39, 51, 64))
                    .min_size(egui::vec2(160.0, 34.0)),
                )
                .clicked()
            {
                self.open_configuration();
            }
            ui.label(
                RichText::new(if self.config.configured() {
                    format!("{} · {}", self.config.name, self.config.group)
                } else {
                    "首次使用，请配置姓名与组别".into()
                })
                .size(12.0)
                .color(Color32::from_rgb(164, 184, 198)),
            );
        });
    }

    pub(super) fn content(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("INVOICE WORKSPACE")
                        .size(11.0)
                        .color(ACCENT)
                        .strong(),
                );
                ui.heading("三份材料，一次整理。");
                ui.label(
                    RichText::new("将发票、购买记录和支付凭证，整理成可提交的报销 PDF。")
                        .size(14.0)
                        .color(MUTED),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                let (label, color) = if self.merging {
                    ("正在生成", ACCENT)
                } else if self.extracting {
                    ("正在识别", ACCENT)
                } else if self.can_output() {
                    ("可以输出", ACCENT)
                } else if self.data.is_some() {
                    ("待核对", AMBER)
                } else {
                    ("等待材料", MUTED)
                };
                pill(ui, label, color);
            });
        });
        ui.add_space(16.0);
        let body_height = (ui.available_height() - 168.0).max(300.0);
        egui::ScrollArea::vertical()
            .id_salt("workspace-scroll")
            .max_height(body_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.materials(ui);
                ui.add_space(16.0);
                self.invoice_details(ui);
            });
        ui.add_space(14.0);
        self.output_card(ui);
        ui.add_space(8.0);
        ui.add(
            egui::Label::new(RichText::new(&self.status).size(12.0).color(if self.error {
                Color32::from_rgb(175, 57, 49)
            } else {
                MUTED
            }))
            .wrap(),
        );
    }

    fn materials(&mut self, ui: &mut egui::Ui) {
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                section(ui, "01", "准备材料");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !self.merging,
                            egui::Button::new("添加文件").min_size(egui::vec2(100.0, 32.0)),
                        )
                        .clicked()
                        && let Some(files) = rfd::FileDialog::new()
                            .set_title("选择发票及两张截图")
                            .add_filter("报销材料", &["pdf", "jpg", "jpeg", "png"])
                            .pick_files()
                    {
                        self.add_files(files);
                    }
                });
            });
            ui.add_space(10.0);
            ui.columns(3, |columns| {
                let selected = [
                    self.pdf.as_deref(),
                    self.images.first().map(PathBuf::as_path),
                    self.images.get(1).map(PathBuf::as_path),
                ];
                for (index, (title, kind, color)) in [
                    ("发票 PDF", "支持多页发票", Color32::from_rgb(65, 100, 155)),
                    ("购买记录", "订单或购买截图", ACCENT),
                    ("支付记录", "付款成功凭证", Color32::from_rgb(169, 121, 51)),
                ]
                .into_iter()
                .enumerate()
                {
                    let ui = &mut columns[index];
                    egui::Frame::new()
                        .fill(if selected[index].is_some() {
                            Color32::from_rgb(248, 251, 249)
                        } else {
                            Color32::from_rgb(249, 250, 251)
                        })
                        .stroke(egui::Stroke::new(1.0, LINE))
                        .corner_radius(10)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                file_icon(ui, color, index != 0);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(title).size(15.0).strong());
                                    ui.label(RichText::new(kind).size(11.0).color(MUTED));
                                });
                            });
                            ui.add_space(8.0);
                            if let Some(path) = selected[index] {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(
                                            path.file_name().unwrap_or_default().to_string_lossy(),
                                        )
                                        .size(12.0),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(path.display().to_string());
                                ui.label(RichText::new("已添加").size(11.0).color(ACCENT));
                            } else {
                                ui.label(RichText::new("等待添加").size(12.0).color(MUTED));
                                ui.label(
                                    RichText::new("PDF / JPG / PNG")
                                        .size(10.0)
                                        .color(Color32::from_rgb(153, 162, 173)),
                                );
                            }
                        });
                }
            });
            ui.add_space(10.0);
            ui.label(
                RichText::new("也可直接拖放到窗口；两张截图按“购买 → 支付”的顺序添加。")
                    .size(12.0)
                    .color(MUTED),
            );
        });
    }

    fn invoice_details(&mut self, ui: &mut egui::Ui) {
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            section(ui, "02", "核对发票信息");
            ui.add_space(10.0);
            if self.extracting {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("正在读取发票，请稍候…");
                });
                ui.add_space(25.0);
            } else if let Some(data) = &self.data {
                if data.number.value().is_none() || data.amount.value().is_none() {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(252, 246, 230))
                        .inner_margin(10)
                        .corner_radius(7)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(if data.has_text {
                                    "关键字段缺失或存在冲突，请核对原件后补填。"
                                } else {
                                    "这是图片型 PDF，请核对原件并填写金额、完整发票号码。"
                                })
                                .size(13.0)
                                .color(AMBER),
                            );
                        });
                    ui.add_space(10.0);
                }
                ui.columns(2, |columns| {
                    let label =
                        columns[0].label(RichText::new("完整发票号码").size(13.0).color(MUTED));
                    columns[0]
                        .add(
                            egui::TextEdit::singleline(&mut self.review.number)
                                .desired_width(f32::INFINITY)
                                .margin(egui::vec2(10.0, 10.0)),
                        )
                        .labelled_by(label.id);
                    columns[0].label(
                        RichText::new(field_status(&data.number))
                            .size(11.0)
                            .color(MUTED),
                    );
                    let label =
                        columns[1].label(RichText::new("价税合计（元）").size(13.0).color(MUTED));
                    columns[1]
                        .add(
                            egui::TextEdit::singleline(&mut self.review.amount)
                                .desired_width(f32::INFINITY)
                                .font(egui::FontId::proportional(22.0))
                                .margin(egui::vec2(10.0, 7.0)),
                        )
                        .labelled_by(label.id);
                    columns[1].label(
                        RichText::new(field_status(&data.amount))
                            .size(11.0)
                            .color(MUTED),
                    );
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("开票日期").size(12.0).color(MUTED));
                    ui.label(
                        RichText::new(data.date.value().map(String::as_str).unwrap_or("待核对"))
                            .size(13.0),
                    );
                    ui.add_space(20.0);
                    ui.label(
                        RichText::new(format!("共 {} 页", data.page_count))
                            .size(12.0)
                            .color(MUTED),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("销售方").size(12.0).color(MUTED));
                    ui.label(
                        RichText::new(data.seller.value().map(String::as_str).unwrap_or("待核对"))
                            .size(13.0),
                    );
                });
                if data.number.value().is_none() || data.amount.value().is_none() {
                    ui.add_space(6.0);
                    ui.checkbox(
                        &mut self.review.confirmed,
                        "已核对原件，并确认金额与完整发票号码",
                    );
                }
            } else {
                egui::Frame::new()
                    .fill(Color32::from_rgb(249, 250, 251))
                    .corner_radius(9)
                    .inner_margin(22)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(
                            RichText::new("添加发票后，信息将显示在这里。")
                                .size(15.0)
                                .color(MUTED),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("金额和票号支持更正，输出前请与原件核对。")
                                .size(12.0)
                                .color(MUTED),
                        );
                    });
            }
        });
    }

    fn output_card(&mut self, ui: &mut egui::Ui) {
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let preview = self
                .data
                .as_ref()
                .and_then(|data| self.review.validated(data).ok())
                .and_then(|(number, amount)| {
                    filename(&self.config.name, &self.config.group, amount, &number).ok()
                });
            let ready = self.can_output();
            let info_width = (ui.available_width() - 285.0).max(200.0);
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(info_width, 64.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.label(
                            RichText::new("03  输出报销材料")
                                .size(12.0)
                                .color(ACCENT)
                                .strong(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(
                                    preview.unwrap_or_else(|| "准备好材料后，即可生成 PDF".into()),
                                )
                                .size(14.0)
                                .strong(),
                            )
                            .truncate(),
                        );
                        match self.config.output_directory(&self.app_dir) {
                            Ok(output) => {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!(
                                            "保存到：{}",
                                            output
                                                .file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                        ))
                                        .size(11.0)
                                        .color(MUTED),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(output.display().to_string());
                            }
                            Err(_) => {
                                ui.label(
                                    RichText::new("请在设置中选择有效的输出目录")
                                        .size(11.0)
                                        .color(AMBER),
                                );
                            }
                        }
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            ready,
                            egui::Button::new(
                                RichText::new(if self.merging {
                                    "正在生成…"
                                } else {
                                    "一键输出 PDF"
                                })
                                .color(Color32::WHITE)
                                .strong(),
                            )
                            .fill(ACCENT)
                            .corner_radius(9)
                            .min_size(egui::vec2(152.0, 46.0)),
                        )
                        .clicked()
                    {
                        self.output();
                    }
                    if ui
                        .add_enabled(!self.merging, egui::Button::new("清除本次").frame(false))
                        .clicked()
                    {
                        self.clear();
                        self.set_status("已清除文件", false);
                    }
                });
            });
        });
    }
}

pub(super) fn configure_context(egui_context: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "noto-sc".into(),
        egui::FontData::from_static(include_bytes!("../../assets/NotoSansSC.ttf"))
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
        "noto-display".into(),
        egui::FontData::from_static(include_bytes!("../../assets/NotoSansSC.ttf"))
            .tweak(egui::FontTweak {
                coords: egui::epaint::text::VariationCoords::new([("wght", 600.0)]),
                ..Default::default()
            })
            .into(),
    );
    fonts.families.insert(
        egui::FontFamily::Name("display".into()),
        vec!["noto-display".into(), "noto-sc".into()],
    );
    egui_context.set_fonts(fonts);
    let mut style = (*egui_context.global_style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(Color32::from_rgb(29, 43, 64));
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(239, 241, 244);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(227, 236, 232);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(208, 228, 221);
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.text_edit_bg_color = Some(Color32::from_rgb(244, 247, 248));
    style.visuals.hyperlink_color = Color32::from_rgb(107, 206, 176);
    style.visuals.window_corner_radius = egui::CornerRadius::same(16);
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(16.0));
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(28.0, egui::FontFamily::Name("display".into())),
    );
    egui_context.set_global_style(style);
}
