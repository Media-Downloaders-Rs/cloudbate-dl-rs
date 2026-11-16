#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if let Err(error) = personal_downloaders::gui::run(personal_downloaders::Site::Cloudbate) {
        eprintln!("No se pudo abrir Cloudbate DL: {error}");
        std::process::exit(1);
    }
}
