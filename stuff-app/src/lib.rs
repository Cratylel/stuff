//! Tauri boundary for the Stuff desktop application.
//!
//! `stuff-core` remains the only source of storage, containment, inventory,
//! and persistence rules. This crate only translates its canonical snapshot
//! and requests into webview-friendly DTOs and Tauri commands.

mod commands;
mod dto;
mod rack_commands;

use std::sync::Arc;

use tauri::{Emitter, Manager};

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BackendMode {
    Live,
    Demo,
}

pub(crate) struct AppState {
    pub service: stuff_core::StuffService,
    pub mode: BackendMode,
    pub connection_note: Option<String>,
}

pub fn run() {
    let database_url = std::env::var("STUFF_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
        .filter(|url| !url.trim().is_empty());

    let (service, mode, connection_note) = tauri::async_runtime::block_on(async move {
        match database_url {
            Some(url) => match stuff_core::StuffService::connect(url).await {
                Ok(service) => (service, BackendMode::Live, None),
                Err(error) => (
                    stuff_core::StuffService::demo(),
                    BackendMode::Demo,
                    Some(format!(
                        "Database unavailable; using local demo data ({error})"
                    )),
                ),
            },
            None => (
                stuff_core::StuffService::demo(),
                BackendMode::Demo,
                Some("STUFF_DATABASE_URL is not set; using local demo data".into()),
            ),
        }
    });

    tauri::Builder::default()
        .manage(Arc::new(AppState {
            service,
            mode,
            connection_note,
        }))
        .setup(|app| {
            let state = app.state::<Arc<AppState>>();
            if matches!(state.mode, BackendMode::Live) {
                let service = state.service.clone();
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let Ok(mut listener) = service.listen().await else {
                        return;
                    };
                    while listener.next().await.is_ok() {
                        if service.refresh().await.is_ok() {
                            let _ = handle.emit("stuff://changed", ());
                        }
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::create_node,
            commands::update_node,
            commands::move_node,
            commands::preview_placement,
            commands::create_shelf,
            commands::clone_storage_subtree,
            commands::delete_storage_subtree,
            commands::delete_storage_orphan_children,
            commands::add_or_create_item,
            commands::adjust_stock,
            commands::move_stock,
            commands::update_stock_placement,
            commands::save_organizer_template,
            commands::clone_organizer_template,
            commands::save_organizer_as_template,
            commands::create_organizer_from_template,
            commands::assign_organizer_template,
            commands::create_fresh_organizer,
            rack_commands::save_rack_template,
            rack_commands::clone_rack_template,
            rack_commands::instantiate_rack_template,
            rack_commands::save_rack_as_template,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Stuff desktop application");
}
