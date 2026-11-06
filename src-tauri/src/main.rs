// Kein zusätzliches Konsolenfenster in Release-Builds unter Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Dünne Tauri-Schicht: Jeder Command validiert über die Typen des Kerns (Serde-Parsing)
//! und ruft die Befehlsschicht `openmail_core::Api` auf. Fehler gehen nur als generische
//! Meldung an das Frontend.

use openmail_core::types::{CalEvent, Contact, Id, MailFilter, Task};
use openmail_core::{Api, AppError};
use tauri::State;

struct AppState {
    api: Api,
}

fn public(error: AppError) -> String {
    error.public_message()
}

#[tauri::command]
fn list_contacts(state: State<'_, AppState>) -> Result<Vec<Contact>, String> {
    state.api.list_contacts().map_err(public)
}

#[tauri::command]
fn save_contact(contact: Contact, state: State<'_, AppState>) -> Result<Contact, String> {
    state.api.save_contact(contact).map_err(public)
}

#[tauri::command]
fn delete_contact(id: Id, state: State<'_, AppState>) -> Result<(), String> {
    state.api.delete_contact(&id).map_err(public)
}

#[tauri::command]
fn list_events(state: State<'_, AppState>) -> Result<Vec<CalEvent>, String> {
    state.api.list_events().map_err(public)
}

#[tauri::command]
fn save_event(event: CalEvent, state: State<'_, AppState>) -> Result<CalEvent, String> {
    state.api.save_event(event).map_err(public)
}

#[tauri::command]
fn delete_event(id: Id, state: State<'_, AppState>) -> Result<(), String> {
    state.api.delete_event(&id).map_err(public)
}

#[tauri::command]
fn list_tasks(state: State<'_, AppState>) -> Result<Vec<Task>, String> {
    state.api.list_tasks().map_err(public)
}

#[tauri::command]
fn save_task(task: Task, state: State<'_, AppState>) -> Result<Task, String> {
    state.api.save_task(task).map_err(public)
}

#[tauri::command]
fn delete_task(id: Id, state: State<'_, AppState>) -> Result<(), String> {
    state.api.delete_task(&id).map_err(public)
}

#[tauri::command]
fn list_filters(state: State<'_, AppState>) -> Result<Vec<MailFilter>, String> {
    state.api.list_filters().map_err(public)
}

#[tauri::command]
fn save_filter(filter: MailFilter, state: State<'_, AppState>) -> Result<MailFilter, String> {
    state.api.save_filter(filter).map_err(public)
}

#[tauri::command]
fn delete_filter(id: Id, state: State<'_, AppState>) -> Result<(), String> {
    state.api.delete_filter(&id).map_err(public)
}

fn main() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .manage(AppState { api: Api::new() })
        .invoke_handler(tauri::generate_handler![
            list_contacts,
            save_contact,
            delete_contact,
            list_events,
            save_event,
            delete_event,
            list_tasks,
            save_task,
            delete_task,
            list_filters,
            save_filter,
            delete_filter,
        ])
        .run(tauri::generate_context!())
}
