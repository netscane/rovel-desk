//! File Picker System - uses rfd async API with global tokio runtime
//!
//! Uses the global tokio runtime from main.rs to ensure Winsock stays
//! properly initialized on Windows.

use bevy::prelude::*;
use std::sync::{mpsc, Mutex};

use crate::state::{AppState, FilePickerRequest, FilePickerResult, FilePickerType};
use crate::get_runtime;

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

/// Handle file picker requests using rfd's async API with global runtime
pub fn handle_file_picker_requests(
    mut events: EventReader<FilePickerRequest>,
    channel: Option<Res<FilePickerChannel>>,
) {
    let Some(channel) = channel else { return };
    
    for event in events.read() {
        let picker_type = event.picker_type;
        let sender = channel.sender.clone();
        
        // Spawn task on global runtime - this ensures we use the same
        // Winsock initialization as the rest of the app
        get_runtime().spawn(async move {
            let path = match picker_type {
                FilePickerType::Novel => {
                    rfd::AsyncFileDialog::new()
                        .add_filter("文本文件", &["txt"])
                        .pick_file()
                        .await
                        .map(|f| f.path().to_path_buf())
                }
                FilePickerType::Voice => {
                    rfd::AsyncFileDialog::new()
                        .add_filter("音频文件", &["wav", "mp3", "flac", "ogg"])
                        .pick_file()
                        .await
                        .map(|f| f.path().to_path_buf())
                }
            };
            
            let _ = sender.send(FilePickerResult { picker_type, path });
        });
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
