//! UI System using bevy_egui - V2 Architecture

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use crate::state::{ApiRequest, AppState, AppView, FilePickerRequest, FilePickerType, PauseAudioEvent, PlaybackState, ResumeAudioEvent, StopAudioEvent, TaskState};

// 颜色主题
mod colors {
    use bevy_egui::egui::Color32;

    pub const BG_DARK: Color32 = Color32::from_rgb(18, 18, 24);
    pub const BG_PANEL: Color32 = Color32::from_rgb(28, 28, 36);
    pub const BG_CARD: Color32 = Color32::from_rgb(38, 38, 48);
    pub const BG_CARD_HOVER: Color32 = Color32::from_rgb(48, 48, 60);
    pub const BG_HIGHLIGHT: Color32 = Color32::from_rgb(60, 80, 120);

    pub const ACCENT: Color32 = Color32::from_rgb(100, 140, 230);
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(120, 160, 255);
    pub const SUCCESS: Color32 = Color32::from_rgb(80, 200, 120);
    pub const WARNING: Color32 = Color32::from_rgb(255, 180, 60);
    pub const DANGER: Color32 = Color32::from_rgb(230, 80, 80);

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 240, 245);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(160, 160, 175);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(100, 100, 115);
}

pub fn ui_system(
    mut contexts: EguiContexts,
    mut app_state: ResMut<AppState>,
    current_view: Res<State<AppView>>,
    mut next_view: ResMut<NextState<AppView>>,
    mut api_events: EventWriter<ApiRequest>,
    mut file_picker_events: EventWriter<FilePickerRequest>,
    mut stop_audio_events: EventWriter<StopAudioEvent>,
    mut pause_audio_events: EventWriter<PauseAudioEvent>,
    mut resume_audio_events: EventWriter<ResumeAudioEvent>,
) {
    let ctx = contexts.ctx_mut();

    // 设置样式
    setup_style(ctx);

    match current_view.get() {
        AppView::NovelList => {
            novel_list_ui(
                ctx,
                &mut app_state,
                &mut next_view,
                &mut api_events,
                &mut file_picker_events,
            );
        }
        AppView::Player => {
            player_ui(ctx, &mut app_state, &mut next_view, &mut api_events, &mut stop_audio_events, &mut pause_audio_events, &mut resume_audio_events);
        }
    }

    // 上传对话框
    upload_novel_dialog(ctx, &mut app_state, &mut api_events, &mut file_picker_events);
    upload_voice_dialog(ctx, &mut app_state, &mut api_events, &mut file_picker_events);

    // 错误提示
    if let Some(error) = &app_state.error.clone() {
        egui::Window::new("⚠ 错误")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(dialog_frame())
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(error).color(colors::DANGER).size(14.0));
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if styled_button(ui, "确定", colors::ACCENT).clicked() {
                        app_state.error = None;
                    }
                });
            });
    }

    // 加载提示
    if app_state.loading {
        egui::Area::new(egui::Id::new("loading_overlay"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(colors::BG_CARD)
                    .rounding(12.0)
                    .inner_margin(24.0)
                    .shadow(egui::epaint::Shadow {
                        spread: 8.0,
                        blur: 16.0,
                        color: egui::Color32::from_black_alpha(60),
                        offset: egui::vec2(0.0, 4.0),
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.add_space(12.0);
                            ui.label(
                                egui::RichText::new("加载中...")
                                    .color(colors::TEXT_PRIMARY)
                                    .size(16.0),
                            );
                        });
                    });
            });
    }
}

fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // 间距
    style.spacing.item_spacing = egui::vec2(12.0, 8.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(16.0);

    // 圆角
    style.visuals.window_rounding = egui::Rounding::same(12.0);
    style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(8.0);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
    style.visuals.widgets.active.rounding = egui::Rounding::same(8.0);

    // 颜色
    style.visuals.panel_fill = colors::BG_DARK;
    style.visuals.window_fill = colors::BG_PANEL;
    style.visuals.extreme_bg_color = colors::BG_CARD;

    style.visuals.widgets.noninteractive.bg_fill = colors::BG_CARD;
    style.visuals.widgets.inactive.bg_fill = colors::BG_CARD;
    style.visuals.widgets.hovered.bg_fill = colors::BG_CARD_HOVER;
    style.visuals.widgets.active.bg_fill = colors::ACCENT;

    style.visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(1.0, colors::TEXT_SECONDARY);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, colors::TEXT_PRIMARY);
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, colors::ACCENT_HOVER);
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, colors::TEXT_PRIMARY);

    style.visuals.selection.bg_fill = colors::ACCENT;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, colors::ACCENT_HOVER);

    ctx.set_style(style);
}

fn dialog_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(colors::BG_PANEL)
        .rounding(16.0)
        .inner_margin(24.0)
        .shadow(egui::epaint::Shadow {
            spread: 8.0,
            blur: 24.0,
            color: egui::Color32::from_black_alpha(80),
            offset: egui::vec2(0.0, 8.0),
        })
}

fn styled_button(ui: &mut egui::Ui, text: &str, color: egui::Color32) -> egui::Response {
    let button = egui::Button::new(egui::RichText::new(text).color(egui::Color32::WHITE).size(14.0))
        .fill(color)
        .rounding(8.0);
    ui.add(button)
}

fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> egui::Response {
    let btn = egui::Button::new(egui::RichText::new(icon).size(16.0))
        .fill(egui::Color32::TRANSPARENT)
        .min_size(egui::vec2(28.0, 28.0))
        .sense(egui::Sense::click())
        .rounding(6.0);
    ui.add(btn).on_hover_text(tooltip)
}

fn novel_list_ui(
    ctx: &egui::Context,
    app_state: &mut AppState,
    next_view: &mut ResMut<NextState<AppView>>,
    api_events: &mut EventWriter<ApiRequest>,
    file_picker_events: &mut EventWriter<FilePickerRequest>,
) {
    // 顶部导航栏
    egui::TopBottomPanel::top("top_panel")
        .frame(
            egui::Frame::none()
                .fill(colors::BG_PANEL)
                .inner_margin(egui::Margin::symmetric(20.0, 16.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("📚 Rovel")
                        .size(24.0)
                        .strong()
                        .color(colors::ACCENT),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("小说有声播放器")
                        .size(14.0)
                        .color(colors::TEXT_SECONDARY),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if styled_button(ui, "🔄 刷新", colors::BG_CARD).clicked() {
                        api_events.send(ApiRequest::LoadNovels);
                        api_events.send(ApiRequest::LoadVoices);
                    }
                    ui.add_space(8.0);
                    if styled_button(ui, "🎤 上传音色", colors::SUCCESS).clicked() {
                        app_state.upload_dialog.show_voice_dialog = true;
                    }
                    ui.add_space(8.0);
                    if styled_button(ui, "📖 上传小说", colors::ACCENT).clicked() {
                        app_state.upload_dialog.show_novel_dialog = true;
                    }
                });
            });
        });

    // 右侧音色面板
    egui::SidePanel::right("voice_panel")
        .min_width(280.0)
        .frame(
            egui::Frame::none()
                .fill(colors::BG_PANEL)
                .inner_margin(16.0),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("🎤 音色列表")
                        .size(18.0)
                        .strong()
                        .color(colors::TEXT_PRIMARY),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, "➕", "添加音色").clicked() {
                        app_state.upload_dialog.show_voice_dialog = true;
                    }
                });
            });

            ui.add_space(12.0);
            ui.add(egui::Separator::default().spacing(1.0));
            ui.add_space(12.0);

            if app_state.voices.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        egui::RichText::new("暂无音色")
                            .size(14.0)
                            .color(colors::TEXT_MUTED),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("点击上方按钮添加")
                            .size(12.0)
                            .color(colors::TEXT_MUTED),
                    );
                });
            } else {
                // 先提取需要的信息，避免在闭包中同时读写 app_state
                let selected_voice_id = app_state.selected_voice.as_ref().map(|v| v.id);
                let voices_display: Vec<_> = app_state.voices.iter().map(|v| {
                    (v.id, v.name.clone(), v.description.clone())
                }).collect();
                
                let mut voice_to_select: Option<uuid::Uuid> = None;
                let mut voice_to_delete: Option<uuid::Uuid> = None;
                
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (voice_id, voice_name, voice_description) in &voices_display {
                        let is_selected = selected_voice_id.map(|id| id == *voice_id).unwrap_or(false);

                        let card_color = if is_selected {
                            colors::BG_HIGHLIGHT
                        } else {
                            colors::BG_CARD
                        };

                        egui::Frame::none()
                            .fill(card_color)
                            .rounding(10.0)
                            .inner_margin(12.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // 左侧：选择按钮（图标+名称）
                                    let select_btn = egui::Button::new(
                                        egui::RichText::new(if is_selected { "🔊" } else { "🎵" })
                                            .size(16.0)
                                            .color(if is_selected { colors::SUCCESS } else { colors::TEXT_MUTED })
                                    )
                                    .fill(egui::Color32::TRANSPARENT)
                                    .frame(false);
                                    
                                    if ui.add(select_btn).clicked() {
                                        voice_to_select = Some(*voice_id);
                                    }

                                    ui.add_space(8.0);

                                    // 名称和描述
                                    ui.vertical(|ui| {
                                        let name_color = if is_selected {
                                            colors::TEXT_PRIMARY
                                        } else {
                                            colors::TEXT_SECONDARY
                                        };
                                        
                                        // 名称也可点击选择
                                        if ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(voice_name)
                                                    .size(14.0)
                                                    .color(name_color)
                                            ).sense(egui::Sense::click())
                                        ).clicked() {
                                            voice_to_select = Some(*voice_id);
                                        }

                                        if let Some(desc) = voice_description {
                                            if !desc.is_empty() {
                                                ui.label(
                                                    egui::RichText::new(desc)
                                                        .size(11.0)
                                                        .color(colors::TEXT_MUTED),
                                                );
                                            }
                                        }
                                    });

                                    // 右侧：删除按钮
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.add(
                                            egui::Button::new("🗑")
                                                .fill(colors::BG_CARD)
                                                .rounding(6.0)
                                        ).on_hover_text("删除音色").clicked() {
                                            voice_to_delete = Some(*voice_id);
                                        }
                                    });
                                });
                            });
                        
                        ui.add_space(6.0);
                    }
                });
                
                // 在循环外处理操作
                if let Some(id) = voice_to_delete {
                    tracing::info!("Sending DeleteVoice request: {}", id);
                    api_events.send(ApiRequest::DeleteVoice(id));
                }
                
                if let Some(id) = voice_to_select {
                    if let Some(voice) = app_state.voices.iter().find(|v| v.id == id).cloned() {
                        app_state.selected_voice = Some(voice);
                    }
                }
            }
        });

    // 中央小说列表
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(colors::BG_DARK)
                .inner_margin(24.0),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("📚 小说列表")
                        .size(20.0)
                        .strong()
                        .color(colors::TEXT_PRIMARY),
                );
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(format!("共 {} 本", app_state.novels.len()))
                        .size(14.0)
                        .color(colors::TEXT_MUTED),
                );
            });

            ui.add_space(16.0);

            if app_state.novels.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.label(
                        egui::RichText::new("📭")
                            .size(48.0)
                            .color(colors::TEXT_MUTED),
                    );
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new("暂无小说")
                            .size(18.0)
                            .color(colors::TEXT_MUTED),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("点击右上角「上传小说」按钮添加")
                            .size(14.0)
                            .color(colors::TEXT_MUTED),
                    );
                });
            } else {
                // 先提取需要的信息，避免在闭包中同时读写 app_state
                let selected_voice_id = app_state.selected_voice.as_ref().map(|v| v.id);
                let novels_display: Vec<_> = app_state.novels.iter().map(|n| {
                    (n.id, n.title.clone(), n.status.clone(), n.total_segments, n.created_at.clone())
                }).collect();
                
                let mut novel_to_delete: Option<uuid::Uuid> = None;
                let mut novel_to_play: Option<(uuid::Uuid, uuid::Uuid, usize)> = None; // (novel_id, voice_id, total_segments)
                
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (novel_id, novel_title, novel_status, total_segments, created_at) in &novels_display {
                        egui::Frame::none()
                            .fill(colors::BG_CARD)
                            .rounding(12.0)
                            .inner_margin(16.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // 左侧图标 - 根据状态显示不同颜色
                                    let (icon, icon_color) = match novel_status.as_str() {
                                        "uploading" => ("📤", colors::ACCENT),
                                        "processing" => ("⏳", colors::WARNING),
                                        "error" => ("❌", colors::DANGER),
                                        _ => ("📖", colors::ACCENT),
                                    };
                                    egui::Frame::none()
                                        .fill(icon_color)
                                        .rounding(8.0)
                                        .inner_margin(12.0)
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(icon)
                                                    .size(24.0)
                                                    .color(egui::Color32::WHITE),
                                            );
                                        });

                                    ui.add_space(16.0);

                                    // 中间信息
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(novel_title)
                                                .size(18.0)
                                                .strong()
                                                .color(colors::TEXT_PRIMARY),
                                        );
                                        ui.add_space(4.0);
                                        ui.horizontal(|ui| {
                                            // 状态标签
                                            let (status_text, status_color) = match novel_status.as_str() {
                                                "uploading" => ("上传中...", colors::ACCENT),
                                                "processing" => ("处理中...", colors::WARNING),
                                                "error" => ("处理失败", colors::DANGER),
                                                _ => ("", colors::SUCCESS),
                                            };
                                            if !status_text.is_empty() {
                                                ui.label(
                                                    egui::RichText::new(status_text)
                                                        .size(12.0)
                                                        .color(status_color),
                                                );
                                                ui.add_space(12.0);
                                            }
                                            
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "📄 {} 段",
                                                    total_segments
                                                ))
                                                .size(12.0)
                                                .color(colors::TEXT_SECONDARY),
                                            );
                                            ui.add_space(16.0);
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "📅 {}",
                                                    if created_at.len() >= 10 {
                                                        &created_at[..10]
                                                    } else {
                                                        created_at
                                                    }
                                                ))
                                                .size(12.0)
                                                .color(colors::TEXT_MUTED),
                                            );
                                        });
                                    });

                                    // 右侧按钮
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            // 只有非上传中状态的小说才能删除
                                            let can_delete = novel_status != "uploading";
                                            if ui.add_enabled(
                                                can_delete,
                                                egui::Button::new("🗑")
                                                    .fill(if can_delete { colors::BG_CARD } else { colors::TEXT_MUTED })
                                                    .rounding(6.0)
                                            ).on_hover_text(if can_delete { "删除小说" } else { "上传中，无法删除" }).clicked() {
                                                novel_to_delete = Some(*novel_id);
                                            }

                                            ui.add_space(8.0);

                                            let is_ready = novel_status == "ready";
                                            let has_voice = selected_voice_id.is_some();
                                            
                                            if is_ready && has_voice {
                                                if styled_button(ui, "▶ 播放", colors::SUCCESS)
                                                    .clicked()
                                                {
                                                    if let Some(voice_id) = selected_voice_id {
                                                        novel_to_play = Some((*novel_id, voice_id, *total_segments));
                                                    }
                                                }
                                            } else if !is_ready {
                                                ui.label(
                                                    egui::RichText::new(match novel_status.as_str() {
                                                        "uploading" => "📤 上传中",
                                                        "processing" => "⏳ 处理中",
                                                        "error" => "❌ 不可用",
                                                        _ => "❌ 不可用",
                                                    })
                                                    .size(12.0)
                                                    .color(colors::TEXT_MUTED),
                                                );
                                            } else {
                                                ui.label(
                                                    egui::RichText::new("← 请先选择音色")
                                                        .size(12.0)
                                                        .color(colors::WARNING),
                                                );
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(12.0);
                    }
                });
                
                // 在循环外处理操作
                if let Some(id) = novel_to_delete {
                    api_events.send(ApiRequest::DeleteNovel(id));
                }
                
                if let Some((novel_id, voice_id, total_segments)) = novel_to_play {
                    // 找到对应的 novel 并设置
                    if let Some(novel) = app_state.novels.iter().find(|n| n.id == novel_id).cloned() {
                        app_state.selected_novel = Some(novel);
                    }
                    // V2: 初始化分页状态
                    app_state.init_segment_pagination(total_segments);
                    // V2: 直接调用 Play API（会自动创建 session）
                    api_events.send(ApiRequest::Play {
                        novel_id,
                        voice_id,
                        start_index: 0,
                    });
                    next_view.set(AppView::Player);
                }
            }
        });

    let _ = file_picker_events;
}

fn upload_novel_dialog(
    ctx: &egui::Context,
    app_state: &mut AppState,
    api_events: &mut EventWriter<ApiRequest>,
    file_picker_events: &mut EventWriter<FilePickerRequest>,
) {
    if !app_state.upload_dialog.show_novel_dialog {
        return;
    }

    egui::Window::new("📖 上传小说")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(dialog_frame())
        .min_width(400.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);

            // 标题输入
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("标题")
                        .size(14.0)
                        .color(colors::TEXT_SECONDARY),
                );
                ui.add_space(24.0);
                ui.add_sized(
                    [280.0, 28.0],
                    egui::TextEdit::singleline(&mut app_state.upload_dialog.novel_title)
                        .hint_text("输入小说标题"),
                );
            });

            ui.add_space(12.0);

            // 文件选择
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("文件")
                        .size(14.0)
                        .color(colors::TEXT_SECONDARY),
                );
                ui.add_space(24.0);

                egui::Frame::none()
                    .fill(colors::BG_CARD)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        if let Some(path) = &app_state.upload_dialog.novel_file_path {
                            ui.label(
                                egui::RichText::new(
                                    path.file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string(),
                                )
                                .color(colors::TEXT_PRIMARY),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("未选择文件").color(colors::TEXT_MUTED),
                            );
                        }
                    });

                ui.add_space(8.0);

                let picking = app_state.upload_dialog.picking_file;
                if ui
                    .add_enabled(
                        !picking,
                        egui::Button::new("选择文件...")
                            .fill(colors::BG_CARD)
                            .rounding(6.0),
                    )
                    .clicked()
                {
                    app_state.upload_dialog.picking_file = true;
                    file_picker_events.send(FilePickerRequest {
                        picker_type: FilePickerType::Novel,
                    });
                }
            });

            ui.add_space(24.0);

            // 按钮
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new("取消")
                                .fill(colors::BG_CARD)
                                .rounding(8.0),
                        )
                        .clicked()
                    {
                        app_state.upload_dialog.reset_novel();
                    }

                    ui.add_space(12.0);

                    let can_upload = !app_state.upload_dialog.novel_title.is_empty()
                        && app_state.upload_dialog.novel_file_path.is_some()
                        && !app_state.upload_dialog.picking_file;

                    if ui
                        .add_enabled(
                            can_upload,
                            egui::Button::new(
                                egui::RichText::new("上传").color(egui::Color32::WHITE),
                            )
                            .fill(if can_upload {
                                colors::ACCENT
                            } else {
                                colors::BG_CARD
                            })
                            .rounding(8.0),
                        )
                        .clicked()
                    {
                        if let Some(path) = app_state.upload_dialog.novel_file_path.take() {
                            let title = std::mem::take(&mut app_state.upload_dialog.novel_title);
                            
                            // 立即创建临时小说对象并添加到列表中
                            let temp_novel = crate::api::NovelResponse::create_temporary(title.clone());
                            app_state.novels.insert(0, temp_novel); // 插入到列表顶部
                            
                            api_events.send(ApiRequest::UploadNovel {
                                title,
                                file_path: path,
                            });
                            app_state.upload_dialog.reset_novel();
                        }
                    }
                });
            });
        });
}

fn upload_voice_dialog(
    ctx: &egui::Context,
    app_state: &mut AppState,
    api_events: &mut EventWriter<ApiRequest>,
    file_picker_events: &mut EventWriter<FilePickerRequest>,
) {
    if !app_state.upload_dialog.show_voice_dialog {
        return;
    }

    egui::Window::new("🎤 上传音色")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(dialog_frame())
        .min_width(400.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);

            // 名称
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("名称")
                        .size(14.0)
                        .color(colors::TEXT_SECONDARY),
                );
                ui.add_space(24.0);
                ui.add_sized(
                    [280.0, 28.0],
                    egui::TextEdit::singleline(&mut app_state.upload_dialog.voice_name)
                        .hint_text("输入音色名称"),
                );
            });

            ui.add_space(12.0);

            // 描述
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("描述")
                        .size(14.0)
                        .color(colors::TEXT_SECONDARY),
                );
                ui.add_space(24.0);
                ui.add_sized(
                    [280.0, 28.0],
                    egui::TextEdit::singleline(&mut app_state.upload_dialog.voice_description)
                        .hint_text("可选"),
                );
            });

            ui.add_space(12.0);

            // 文件
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("文件")
                        .size(14.0)
                        .color(colors::TEXT_SECONDARY),
                );
                ui.add_space(24.0);

                egui::Frame::none()
                    .fill(colors::BG_CARD)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        if let Some(path) = &app_state.upload_dialog.voice_file_path {
                            ui.label(
                                egui::RichText::new(
                                    path.file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string(),
                                )
                                .color(colors::TEXT_PRIMARY),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("未选择文件").color(colors::TEXT_MUTED),
                            );
                        }
                    });

                ui.add_space(8.0);

                let picking = app_state.upload_dialog.picking_file;
                if ui
                    .add_enabled(
                        !picking,
                        egui::Button::new("选择文件...")
                            .fill(colors::BG_CARD)
                            .rounding(6.0),
                    )
                    .clicked()
                {
                    app_state.upload_dialog.picking_file = true;
                    file_picker_events.send(FilePickerRequest {
                        picker_type: FilePickerType::Voice,
                    });
                }
            });

            ui.add_space(24.0);

            // 按钮
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new("取消")
                                .fill(colors::BG_CARD)
                                .rounding(8.0),
                        )
                        .clicked()
                    {
                        app_state.upload_dialog.reset_voice();
                    }

                    ui.add_space(12.0);

                    let can_upload = !app_state.upload_dialog.voice_name.is_empty()
                        && app_state.upload_dialog.voice_file_path.is_some()
                        && !app_state.upload_dialog.picking_file;

                    if ui
                        .add_enabled(
                            can_upload,
                            egui::Button::new(
                                egui::RichText::new("上传").color(egui::Color32::WHITE),
                            )
                            .fill(if can_upload {
                                colors::ACCENT
                            } else {
                                colors::BG_CARD
                            })
                            .rounding(8.0),
                        )
                        .clicked()
                    {
                        if let Some(path) = app_state.upload_dialog.voice_file_path.take() {
                            let name = std::mem::take(&mut app_state.upload_dialog.voice_name);
                            let desc =
                                std::mem::take(&mut app_state.upload_dialog.voice_description);
                            let description = if desc.is_empty() { None } else { Some(desc) };
                            api_events.send(ApiRequest::UploadVoice {
                                name,
                                description,
                                file_path: path,
                            });
                            app_state.upload_dialog.reset_voice();
                        }
                    }
                });
            });
        });
}

fn player_ui(
    ctx: &egui::Context,
    app_state: &mut AppState,
    next_view: &mut ResMut<NextState<AppView>>,
    api_events: &mut EventWriter<ApiRequest>,
    stop_audio_events: &mut EventWriter<StopAudioEvent>,
    pause_audio_events: &mut EventWriter<PauseAudioEvent>,
    resume_audio_events: &mut EventWriter<ResumeAudioEvent>,
) {
    let session = match &app_state.current_session {
        Some(s) => s.clone(),
        None => {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(colors::BG_DARK))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(100.0);
                        ui.label(egui::RichText::new("没有活动会话").size(18.0).color(colors::TEXT_MUTED));
                        ui.add_space(24.0);
                        if styled_button(ui, "← 返回列表", colors::ACCENT).clicked() {
                            next_view.set(AppView::NovelList);
                        }
                    });
                });
            return;
        }
    };

    let total = app_state.segment_pagination.total_segments;
    let current = app_state.current_segment_index;

    // 顶部栏 - 简洁设计
    egui::TopBottomPanel::top("player_top")
        .exact_height(56.0)
        .frame(egui::Frame::none().fill(colors::BG_PANEL).inner_margin(egui::Margin::symmetric(20.0, 10.0)))
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                // 返回按钮
                if ui.add(egui::Button::new(egui::RichText::new("←").size(18.0).color(colors::TEXT_SECONDARY))
                    .fill(colors::BG_CARD).rounding(6.0).min_size(egui::vec2(36.0, 36.0))).clicked() {
                    stop_audio_events.send(StopAudioEvent);
                    api_events.send(ApiRequest::CloseSession(session.session_id.clone()));
                    next_view.set(AppView::NovelList);
                }

                ui.add_space(16.0);

                // 小说标题
                if let Some(novel) = &app_state.selected_novel {
                    ui.label(egui::RichText::new(&novel.title).size(18.0).strong().color(colors::TEXT_PRIMARY));
                }

                ui.add_space(16.0);

                // 音色选择
                ui.label(egui::RichText::new("音色:").size(13.0).color(colors::TEXT_MUTED));
                let current_voice_name = app_state.voices.iter()
                    .find(|v| v.id == session.voice_id)
                    .map(|v| v.name.as_str())
                    .unwrap_or("未知");
                egui::ComboBox::from_id_salt("voice_selector")
                    .selected_text(current_voice_name)
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for voice in &app_state.voices {
                            let is_selected = voice.id == session.voice_id;
                            if ui.selectable_label(is_selected, &voice.name).clicked() && !is_selected {
                                api_events.send(ApiRequest::ChangeVoice {
                                    session_id: session.session_id.clone(),
                                    voice_id: voice.id,
                                });
                            }
                        }
                    });

                // 右侧状态 - 使用剩余空间
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ws_indicator = match app_state.ws_state {
                        crate::state::WsConnectionState::Connected => ("🟢", "已连接"),
                        crate::state::WsConnectionState::Connecting => ("🟡", "连接中"),
                        crate::state::WsConnectionState::Reconnecting => ("🟡", "重连中"),
                        crate::state::WsConnectionState::Disconnected => ("🔴", "未连接"),
                    };
                    ui.label(egui::RichText::new(ws_indicator.0).size(10.0)).on_hover_text(ws_indicator.1);
                    ui.add_space(8.0);
                    
                    let (state_text, state_color) = match app_state.playback_state {
                        PlaybackState::Stopped => ("已停止", colors::TEXT_MUTED),
                        PlaybackState::Playing => ("播放中", colors::SUCCESS),
                        PlaybackState::Paused => ("已暂停", colors::WARNING),
                        PlaybackState::Loading => ("加载中", colors::ACCENT),
                    };
                    ui.label(egui::RichText::new(state_text).size(13.0).color(state_color));
                });
            });
        });

    // 底部控制栏
    egui::TopBottomPanel::bottom("player_controls")
        .exact_height(100.0)
        .frame(egui::Frame::none().fill(colors::BG_PANEL).inner_margin(egui::Margin::symmetric(24.0, 8.0)))
        .show(ctx, |ui| {
            let progress = if total > 0 { current as f32 / total as f32 } else { 0.0 };

            // 控制按钮行 - 放在最上面
            ui.horizontal(|ui| {
                // 左侧进度文字
                ui.label(egui::RichText::new(format!("{}/{}", current + 1, total)).size(12.0).color(colors::TEXT_SECONDARY));
                
                ui.add_space(20.0);
                
                // 居中的控制按钮
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;

                    // 上一段
                    let prev_enabled = current > 0;
                    if ui.add_enabled(prev_enabled,
                        egui::Button::new(egui::RichText::new("⏮").size(18.0)
                            .color(if prev_enabled { colors::TEXT_PRIMARY } else { colors::TEXT_MUTED }))
                        .fill(colors::BG_CARD).rounding(8.0).min_size(egui::vec2(50.0, 44.0))
                    ).on_hover_text("上一段").clicked() {
                        stop_audio_events.send(StopAudioEvent);
                        api_events.send(ApiRequest::Seek { 
                            session_id: session.session_id.clone(), 
                            segment_index: current.saturating_sub(1) as u32,
                        });
                    }

                    // 主控制按钮 - 根据状态显示不同按钮
                    match app_state.playback_state {
                        PlaybackState::Stopped => {
                            // 播放按钮
                            if ui.add(egui::Button::new(egui::RichText::new("▶").size(22.0).color(egui::Color32::WHITE))
                                .fill(colors::SUCCESS).rounding(8.0).min_size(egui::vec2(60.0, 44.0)))
                                .on_hover_text("播放").clicked() {
                                if let Some(session) = &app_state.current_session {
                                    let indices = app_state.task_manager.calculate_prefetch_range(current as u32, total as u32);
                                    if !indices.is_empty() {
                                        app_state.task_manager.add_pending_tasks(&session.session_id, &indices);
                                        api_events.send(ApiRequest::SubmitInfer {
                                            session_id: session.session_id.clone(),
                                            segment_indices: indices,
                                        });
                                    }
                                    if app_state.task_manager.is_segment_ready(current as u32) {
                                        api_events.send(ApiRequest::LoadAudio {
                                            novel_id: session.novel_id,
                                            segment_index: current as u32,
                                            voice_id: session.voice_id,
                                        });
                                    } else {
                                        app_state.playback_state = PlaybackState::Loading;
                                        app_state.waiting_for_audio = true;
                                    }
                                }
                            }
                        }
                        PlaybackState::Loading => {
                            // 加载中 - 显示停止按钮
                            ui.add(egui::Button::new(egui::RichText::new("⏳").size(22.0).color(colors::WARNING))
                                .fill(colors::BG_CARD).rounding(8.0).min_size(egui::vec2(60.0, 44.0)))
                                .on_hover_text("加载中...");
                            if ui.add(egui::Button::new(egui::RichText::new("⏹").size(18.0).color(egui::Color32::WHITE))
                                .fill(colors::DANGER).rounding(8.0).min_size(egui::vec2(50.0, 44.0)))
                                .on_hover_text("停止").clicked() {
                                stop_audio_events.send(StopAudioEvent);
                                app_state.playback_state = PlaybackState::Stopped;
                            }
                        }
                        PlaybackState::Playing => {
                            // 暂停按钮
                            if ui.add(egui::Button::new(egui::RichText::new("⏸").size(22.0).color(egui::Color32::WHITE))
                                .fill(colors::WARNING).rounding(8.0).min_size(egui::vec2(60.0, 44.0)))
                                .on_hover_text("暂停").clicked() {
                                pause_audio_events.send(PauseAudioEvent);
                                app_state.playback_state = PlaybackState::Paused;
                            }
                            // 停止按钮
                            if ui.add(egui::Button::new(egui::RichText::new("⏹").size(18.0).color(egui::Color32::WHITE))
                                .fill(colors::DANGER).rounding(8.0).min_size(egui::vec2(50.0, 44.0)))
                                .on_hover_text("停止").clicked() {
                                stop_audio_events.send(StopAudioEvent);
                                app_state.playback_state = PlaybackState::Stopped;
                            }
                        }
                        PlaybackState::Paused => {
                            // 继续按钮
                            if ui.add(egui::Button::new(egui::RichText::new("▶").size(22.0).color(egui::Color32::WHITE))
                                .fill(colors::SUCCESS).rounding(8.0).min_size(egui::vec2(60.0, 44.0)))
                                .on_hover_text("继续").clicked() {
                                resume_audio_events.send(ResumeAudioEvent);
                                app_state.playback_state = PlaybackState::Playing;
                            }
                            // 停止按钮
                            if ui.add(egui::Button::new(egui::RichText::new("⏹").size(18.0).color(egui::Color32::WHITE))
                                .fill(colors::DANGER).rounding(8.0).min_size(egui::vec2(50.0, 44.0)))
                                .on_hover_text("停止").clicked() {
                                stop_audio_events.send(StopAudioEvent);
                                app_state.playback_state = PlaybackState::Stopped;
                            }
                        }
                    }

                    // 下一段
                    let next_enabled = current < total.saturating_sub(1);
                    if ui.add_enabled(next_enabled,
                        egui::Button::new(egui::RichText::new("⏭").size(18.0)
                            .color(if next_enabled { colors::TEXT_PRIMARY } else { colors::TEXT_MUTED }))
                        .fill(colors::BG_CARD).rounding(8.0).min_size(egui::vec2(50.0, 44.0))
                    ).on_hover_text("下一段").clicked() {
                        stop_audio_events.send(StopAudioEvent);
                        api_events.send(ApiRequest::Seek { 
                            session_id: session.session_id.clone(), 
                            segment_index: (current + 1) as u32,
                        });
                    }
                });
                
                // 右侧百分比
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{:.0}%", progress * 100.0)).size(12.0).color(colors::TEXT_MUTED));
                });
            });

            ui.add_space(6.0);

            // 进度滑块 - 可拖动（全宽）
            let mut slider_value = current as f64;
            let slider_max = (total.saturating_sub(1)) as f64;
            
            let available_width = ui.available_width();
            ui.horizontal(|ui| {
                ui.spacing_mut().slider_width = available_width - 20.0; // 几乎占满整个宽度
                
                let slider = egui::Slider::new(&mut slider_value, 0.0..=slider_max.max(1.0))
                    .show_value(false)
                    .trailing_fill(true);
                
                let response = ui.add(slider);
                
                // 当用户拖动滑块释放时跳转
                if response.drag_stopped() {
                    let new_index = slider_value.round() as usize;
                    if new_index != current {
                        stop_audio_events.send(StopAudioEvent);
                        
                        // 检查目标位置是否在已加载范围内
                        let loaded_start = app_state.segment_pagination.loaded_range.start;
                        let loaded_end = app_state.segment_pagination.loaded_range.end;
                        
                        if new_index >= loaded_start && new_index < loaded_end {
                            // 在已加载范围内，直接跳转并滚动
                            api_events.send(ApiRequest::Seek { 
                                session_id: session.session_id.clone(), 
                                segment_index: new_index as u32,
                            });
                            app_state.scroll_to_segment = Some(new_index);
                        } else {
                            // 不在已加载范围内，需要先加载段落
                            // 计算加载范围：目标位置前后各加载一些
                            let load_start = new_index.saturating_sub(15);
                            app_state.segments.clear();
                            app_state.segment_pagination.loaded_range = 0..0;
                            
                            // 发送加载请求，加载完成后会设置 scroll_to_segment
                            app_state.scroll_to_segment = Some(new_index);
                            api_events.send(ApiRequest::LoadSegments {
                                novel_id: session.novel_id,
                                start: Some(load_start),
                                limit: Some(100),
                            });
                            api_events.send(ApiRequest::Seek { 
                                session_id: session.session_id.clone(), 
                                segment_index: new_index as u32,
                            });
                        }
                    }
                }
            });
        });

    // 段落列表
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(colors::BG_DARK).inner_margin(20.0))
        .show(ctx, |ui| {
            if app_state.segments.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("加载中...").size(14.0).color(colors::TEXT_MUTED));
                });
                return;
            }

            let loaded_start = app_state.segment_pagination.loaded_range.start;
            let has_more = app_state.segment_pagination.has_more;
            let loaded_end = app_state.segment_pagination.loaded_range.end;
            let loading_more = app_state.segment_pagination.loading_more;
            let novel_id = session.novel_id;
            
            // 检查是否需要滚动到指定段落
            let scroll_to = app_state.scroll_to_segment;
            
            // 只有当目标段落在已加载范围内时才执行滚动并清除标记
            let should_scroll = scroll_to.map(|idx| idx >= loaded_start && idx < loaded_end).unwrap_or(false);
            if should_scroll {
                app_state.scroll_to_segment = None;
            }

            let mut scroll_area = egui::ScrollArea::vertical()
                .auto_shrink([false, false]);
            
            // 如果需要滚动到的段落已加载，计算滚动位置
            if should_scroll {
                if let Some(target_index) = scroll_to {
                    // 估算每个段落的高度（约 60px），计算目标位置
                    let row_height = 60.0;
                    let target_offset = (target_index.saturating_sub(loaded_start)) as f32 * row_height;
                    scroll_area = scroll_area.vertical_scroll_offset(target_offset);
                }
            }
            
            let scroll_output = scroll_area.show(ui, |ui| {
                    // 前面还有更多
                    if loaded_start > 0 {
                        ui.label(egui::RichText::new(format!("... 前面还有 {} 段", loaded_start)).size(11.0).color(colors::TEXT_MUTED));
                        ui.add_space(8.0);
                    }

                    for segment in app_state.segments.iter() {
                        let is_current = segment.index == current;
                        let bg = if is_current { colors::BG_HIGHLIGHT } else { colors::BG_CARD };

                        let task_state = app_state.task_manager.tasks.get(&(segment.index as u32))
                            .map(|t| t.state);
                        let state_indicator = match task_state {
                            Some(TaskState::Ready) => "✓",
                            Some(TaskState::Inferring) => "⟳",
                            Some(TaskState::Pending) => "○",
                            Some(TaskState::Failed) => "✗",
                            Some(TaskState::Cancelled) => "–",
                            None => " ",
                        };
                        let state_color = match task_state {
                            Some(TaskState::Ready) => colors::SUCCESS,
                            Some(TaskState::Inferring) => colors::ACCENT,
                            Some(TaskState::Pending) => colors::TEXT_MUTED,
                            Some(TaskState::Failed) => colors::DANGER,
                            _ => colors::TEXT_MUTED,
                        };

                        let response = egui::Frame::none()
                            .fill(bg)
                            .stroke(if is_current { egui::Stroke::new(2.0, colors::ACCENT) } else { egui::Stroke::NONE })
                            .rounding(10.0)
                            .inner_margin(14.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // 任务状态指示器
                                    ui.label(egui::RichText::new(state_indicator).size(12.0).color(state_color));
                                    ui.add_space(6.0);
                                    // 序号
                                    ui.label(egui::RichText::new(format!("{:03}", segment.index + 1))
                                        .size(12.0).color(if is_current { colors::ACCENT } else { colors::TEXT_MUTED }));
                                    ui.add_space(10.0);
                                    // 内容
                                    ui.add(egui::Label::new(
                                        egui::RichText::new(&segment.content)
                                            .size(if is_current { 15.0 } else { 14.0 })
                                            .color(if is_current { colors::TEXT_PRIMARY } else { colors::TEXT_SECONDARY })
                                    ).wrap());
                                });
                            }).response;

                        if response.interact(egui::Sense::click()).clicked() {
                            api_events.send(ApiRequest::Seek { 
                                session_id: session.session_id.clone(), 
                                segment_index: segment.index as u32,
                            });
                        }

                        ui.add_space(8.0);
                    }

                    // 后面还有更多 - 点击加载
                    if has_more {
                        ui.add_space(8.0);
                        ui.vertical_centered(|ui| {
                            let remaining = total.saturating_sub(loaded_end);
                            if loading_more {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(egui::RichText::new("加载中...").size(13.0).color(colors::TEXT_MUTED));
                                });
                            } else if ui.add(egui::Button::new(
                                egui::RichText::new(format!("加载更多 (还有 {} 段)", remaining))
                                    .size(13.0)
                                    .color(colors::ACCENT)
                            ).fill(colors::BG_CARD).rounding(8.0).min_size(egui::vec2(200.0, 36.0))).clicked() {
                                app_state.segment_pagination.loading_more = true;
                                api_events.send(ApiRequest::LoadSegments {
                                    novel_id,
                                    start: Some(loaded_end),
                                    limit: Some(100),
                                });
                            }
                        });
                        ui.add_space(8.0);
                    }
                });

            // 检测滚动到底部自动加载
            if has_more && !loading_more {
                let scroll_offset = scroll_output.state.offset.y;
                let content_size = scroll_output.content_size.y;
                let inner_size = scroll_output.inner_rect.height();
                
                // 当滚动到距离底部 100px 以内时自动加载
                if content_size > inner_size && scroll_offset + inner_size + 100.0 >= content_size {
                    app_state.segment_pagination.loading_more = true;
                    api_events.send(ApiRequest::LoadSegments {
                        novel_id,
                        start: Some(loaded_end),
                        limit: Some(100),
                    });
                }
            }
        });
}
