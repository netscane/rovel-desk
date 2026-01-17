//! File Picker System - uses native-dialog for better Windows compatibility
//!
//! native-dialog uses Windows native APIs directly without COM initialization
//! issues that can interfere with Winsock.

use bevy::prelude::*;
use std::sync::{mpsc, Mutex};

use crate::state::{AppState, FilePickerRequest, FilePickerResult, FilePickerType};

/// Channel receiver for file picker results
#[derive(Resource)]
pub struct FilePickerChannel {
    receiver: Mutex<mpsc::Receiver<FilePickerResult>>,
    sender: mpsc::Sender<FilePickerResult>,
}

impl Default for FilePickerChannel {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { 
            sender, 
            receiver: Mutex::new(receiver),
        }
    }
}

/// Setup file picker channel
pub fn setup_file_picker_channel(mut commands: Commands) {
    commands.insert_resource(FilePickerChannel::default());
}

/// Handle file picker requests using native-dialog
pub fn handle_file_picker_requests(
    mut events: EventReader<FilePickerRequest>,
    channel: Option<Res<FilePickerChannel>>,
) {
    let Some(channel) = channel else { return };
    
    for event in events.read() {
        let picker_type = event.picker_type;
        let sender = channel.sender.clone();
        
        // Spawn a thread for the file dialog
        // native-dialog handles Windows APIs more gracefully
        std::thread::Builder::new()
            .name("file_picker".to_string())
            .spawn(move || {
                use native_dialog::FileDialog;
                
                let result = match picker_type {
                    FilePickerType::Novel => {
                        FileDialog::new()
                            .add_filter("文本文件", &["txt"])
                            .show_open_single_file()
                    }
                    FilePickerType::Voice => {
                        FileDialog::new()
                            .add_filter("音频文件", &["wav", "mp3", "flac", "ogg"])
                            .show_open_single_file()
                    }
                };
                
                let path = match result {
                    Ok(Some(p)) => Some(p),
                    Ok(None) => None,
                    Err(e) => {
                        tracing::warn!("File dialog error: {}", e);
                        None
                    }
                };
                
                let _ = sender.send(FilePickerResult { picker_type, path });
            })
            .expect("Failed to spawn file picker thread");
    }
}

/// Poll file picker results from channel
pub fn poll_file_picker_tasks(
    channel: Option<Res<FilePickerChannel>>,
    mut result_events: EventWriter<FilePickerResult>,
) {
    let Some(channel) = channel else { return };
    
    // Non-blocking receive with mutex
    if let Ok(receiver) = channel.receiver.lock() {
        while let Ok(result) = receiver.try_recv() {
            result_events.send(result);
        }
    };
}

/// Handle file picker results
pub fn handle_file_picker_results(
    mut events: EventReader<FilePickerResult>,
    mut app_state: ResMut<AppState>,
) {
    for event in events.read() {
        app_state.upload_dialog.picking_file = false;
        
        if let Some(path) = &event.path {
            match event.picker_type {
                FilePickerType::Novel => {
                    // Auto-fill title from filename
                    if app_state.upload_dialog.novel_title.is_empty() {
                        if let Some(stem) = path.file_stem() {
                            app_state.upload_dialog.novel_title = stem.to_string_lossy().to_string();
                        }
                    }
                    app_state.upload_dialog.novel_file_path = Some(path.clone());
                }
                FilePickerType::Voice => {
                    // Auto-fill name from filename
                    if app_state.upload_dialog.voice_name.is_empty() {
                        if let Some(stem) = path.file_stem() {
                            app_state.upload_dialog.voice_name = stem.to_string_lossy().to_string();
                        }
                    }
                    app_state.upload_dialog.voice_file_path = Some(path.clone());
                }
            }
        }
    }
}
