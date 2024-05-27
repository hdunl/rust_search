use eframe::{egui, epi};
use log::{info, error};
use native_dialog::FileDialog;
use rayon::prelude::*;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;
use walkdir::{DirEntry, WalkDir};
use zip::read::ZipArchive;

struct FileSearcherApp {
    query: String,
    results: Arc<Mutex<Vec<(String, Option<PathBuf>, bool)>>>, // (result, zip_path, is_zip_entry)
    searching: Arc<Mutex<bool>>,
    search_directory: String,
    progress: Arc<Mutex<f32>>,
    total_entries: Arc<AtomicUsize>,
    processed_entries: Arc<AtomicUsize>,
    status_message: Arc<Mutex<String>>,
    search_stats: Arc<Mutex<Option<SearchStats>>>,
    show_stats_button: Arc<Mutex<bool>>,
    show_stats: Arc<Mutex<bool>>,
}

struct SearchStats {
    total_files: usize,
    matched_files: usize,
    total_time: Duration,
}

impl FileSearcherApp {
    fn new() -> Self {
        env_logger::init();
        Self {
            query: String::new(),
            results: Arc::new(Mutex::new(Vec::new())),
            searching: Arc::new(Mutex::new(false)),
            search_directory: String::new(),
            progress: Arc::new(Mutex::new(0.0)),
            total_entries: Arc::new(AtomicUsize::new(0)),
            processed_entries: Arc::new(AtomicUsize::new(0)),
            status_message: Arc::new(Mutex::new(String::new())),
            search_stats: Arc::new(Mutex::new(None)),
            show_stats_button: Arc::new(Mutex::new(false)),
            show_stats: Arc::new(Mutex::new(false)),
        }
    }

    fn search_files(&self) {
        let query = self.query.clone().to_lowercase();
        let search_directory = if self.search_directory.is_empty() {
            "C:/".to_string()
        } else {
            self.search_directory.clone()
        };
        let results = Arc::clone(&self.results);
        let searching = Arc::clone(&self.searching);
        let progress = Arc::clone(&self.progress);
        let total_entries = Arc::clone(&self.total_entries);
        let processed_entries = Arc::clone(&self.processed_entries);
        let status_message = Arc::clone(&self.status_message);
        let search_stats = Arc::clone(&self.search_stats);
        let show_stats_button = Arc::clone(&self.show_stats_button);
        let show_stats = Arc::clone(&self.show_stats);

        thread::spawn(move || {
            let start_time = Instant::now();
            *searching.lock().unwrap() = true;
            *progress.lock().unwrap() = 0.0;
            results.lock().unwrap().clear();
            total_entries.store(0, Ordering::SeqCst);
            processed_entries.store(0, Ordering::SeqCst);
            *status_message.lock().unwrap() = format!("Counting items in {}...", search_directory);

            let total_count: usize = WalkDir::new(&search_directory)
                .into_iter()
                .par_bridge()
                .filter_map(|entry| entry.ok())
                .map(|_| {
                    total_entries.fetch_add(1, Ordering::SeqCst) + 1
                })
                .count();

            *status_message.lock().unwrap() = "Processing items...".to_string();

            let matched_files: Vec<(String, Option<PathBuf>, bool)> = WalkDir::new(&search_directory)
                .follow_links(true)
                .into_iter()
                .par_bridge()
                .filter_map(|e| e.ok())
                .filter_map(|entry| {
                    if entry.path().extension().and_then(|s| s.to_str()) == Some("zip") {
                        search_in_zip(&entry.path(), &query, &results);
                    }
                    if is_match(&entry, &query) {
                        return Some((entry.path().display().to_string(), None::<PathBuf>, false));
                    }
                    let processed = processed_entries.fetch_add(1, Ordering::SeqCst) + 1;
                    *progress.lock().unwrap() = processed as f32 / total_count as f32;
                    if processed % 1000 == 0 {
                        *status_message.lock().unwrap() = format!(
                            "Processing items: {}/{} processed. Please wait...",
                            processed,
                            total_count
                        );
                    }
                    None
                })
                .collect();

            let mut all_results = results.lock().unwrap();
            all_results.extend(matched_files);

            let total_time = start_time.elapsed();
            *search_stats.lock().unwrap() = Some(SearchStats {
                total_files: total_count,
                matched_files: all_results.len(),
                total_time,
            });

            info!("Search completed with {} results found.", all_results.len());
            *searching.lock().unwrap() = false;
            *progress.lock().unwrap() = 1.0;
            *status_message.lock().unwrap() = "Search completed.".to_string();
            *show_stats_button.lock().unwrap() = true;
        });
    }

    fn open_file_explorer(&self, path: &str, zip_path: Option<PathBuf>) {
        if let Some(zip_path) = zip_path {
            Command::new("explorer")
                .arg("/select,")
                .arg(zip_path)
                .spawn()
                .expect("Failed to open file explorer");
        } else {
            Command::new("explorer")
                .arg("/select,")
                .arg(path)
                .spawn()
                .expect("Failed to open file explorer");
        }
    }

    fn open_file(&self, path: &str, zip_path: Option<PathBuf>, is_zip_entry: bool) {
        info!("Attempting to open file: path = {}, zip_path = {:?}, is_zip_entry = {}", path, zip_path, is_zip_entry);
        println!("Attempting to open file: path = {}, zip_path = {:?}, is_zip_entry = {}", path, zip_path, is_zip_entry);
        if is_zip_entry {
            if let Some(zip_path) = zip_path {
                info!("Opening zip entry: {}", path);
                println!("Opening zip entry: {}", path);
                let temp_dir = match tempdir() {
                    Ok(dir) => dir,
                    Err(e) => {
                        error!("Failed to create temporary directory: {}", e);
                        println!("Failed to create temporary directory: {}", e);
                        return;
                    }
                };
                let extract_path = temp_dir.path().join(Path::new(path.split(':').last().unwrap().trim()));
                info!("Temporary directory created at: {}", temp_dir.path().display());
                println!("Temporary directory created at: {}", temp_dir.path().display());
                info!("Extract path set to: {}", extract_path.display());
                println!("Extract path set to: {}", extract_path.display());

                let file = match File::open(&zip_path) {
                    Ok(file) => file,
                    Err(e) => {
                        error!("Failed to open zip file: {}", e);
                        println!("Failed to open zip file: {}", e);
                        return;
                    }
                };
                let mut archive = match ZipArchive::new(file) {
                    Ok(archive) => archive,
                    Err(e) => {
                        error!("Failed to read zip archive: {}", e);
                        println!("Failed to read zip archive: {}", e);
                        return;
                    }
                };

                for i in 0..archive.len() {
                    let mut file = match archive.by_index(i) {
                        Ok(file) => file,
                        Err(e) => {
                            error!("Failed to access zip entry: {}", e);
                            println!("Failed to access zip entry: {}", e);
                            continue;
                        }
                    };
                    let entry_name = file.name().replace("\\", "/");
                    let target_name = path.split(':').last().unwrap().trim().replace("\\", "/");
                    info!("Checking zip entry: {}", entry_name);
                    println!("Checking zip entry: {}", entry_name);
                    info!("Target name: {}", target_name);
                    println!("Target name: {}", target_name);
                    if entry_name == target_name {
                        let mut outfile = match File::create(&extract_path) {
                            Ok(outfile) => outfile,
                            Err(e) => {
                                error!("Failed to create output file: {}", e);
                                println!("Failed to create output file: {}", e);
                                return;
                            }
                        };
                        if let Err(e) = std::io::copy(&mut file, &mut outfile) {
                            error!("Failed to extract file: {}", e);
                            println!("Failed to extract file: {}", e);
                            return;
                        }
                        info!("Extracted file to: {}", extract_path.display());
                        println!("Extracted file to: {}", extract_path.display());
                        if let Err(e) = Command::new("explorer")
                            .arg("/select,")
                            .arg(extract_path.to_str().unwrap())
                            .spawn() {
                            error!("Failed to open extracted file: {}", e);
                            println!("Failed to open extracted file: {}", e);
                        } else {
                            info!("Successfully opened extracted file: {}", extract_path.display());
                            println!("Successfully opened extracted file: {}", extract_path.display());
                        }
                        return;
                    }
                }
                error!("File not found in zip: {}", path);
                println!("File not found in zip: {}", path);
            } else {
                error!("Zip path is None for: {}", path);
                println!("Zip path is None for: {}", path);
            }
        } else {
            info!("Opening file: {}", path);
            println!("Opening file: {}", path);
            if let Err(e) = Command::new("explorer")
                .arg(path)
                .spawn() {
                error!("Failed to open file: {}", e);
                println!("Failed to open file: {}", e);
            } else {
                info!("Successfully opened file: {}", path);
                println!("Successfully opened file: {}", path);
            }
        }
    }
}

fn is_match(entry: &DirEntry, query: &str) -> bool {
    if let Some(file_name) = entry.file_name().to_str() {
        file_name.to_lowercase().contains(query)
    } else {
        false
    }
}

fn search_in_zip(path: &Path, query: &str, results: &Arc<Mutex<Vec<(String, Option<PathBuf>, bool)>>>) {
    if let Ok(file) = File::open(path) {
        if let Ok(mut archive) = ZipArchive::new(file) {
            for i in 0..archive.len() {
                if let Ok(mut file) = archive.by_index(i) {
                    let entry_name = file.name().replace("\\", "/");
                    info!("Checking zip entry: {}", entry_name);
                    println!("Checking zip entry: {}", entry_name);
                    if entry_name.to_lowercase().contains(query) {
                        results.lock().unwrap().push((
                            format!("{}: {}", path.display(), file.name()),
                            Some(path.to_path_buf()),
                            true,
                        ));
                        info!("Found match in zip: {}", file.name());
                        println!("Found match in zip: {}", file.name());
                    }
                }
            }
        } else {
            error!("Failed to read zip archive: {}", path.display());
            println!("Failed to read zip archive: {}", path.display());
        }
    } else {
        error!("Failed to open zip file: {}", path.display());
        println!("Failed to open zip file: {}", path.display());
    }
}

impl epi::App for FileSearcherApp {
    fn setup(&mut self, _ctx: &egui::CtxRef, _frame: &epi::Frame, _storage: Option<&dyn epi::Storage>) {
        let mut style = (*_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        _ctx.set_style(style);
    }

    fn update(&mut self, ctx: &egui::CtxRef, _frame: &epi::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("File Searcher");

            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.text_edit_singleline(&mut self.query);
                if ui.button("Go").clicked() {
                    if !*self.searching.lock().unwrap() {
                        self.search_files();
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("Directory:");
                ui.text_edit_singleline(&mut self.search_directory);
                if ui.button("Select Directory").clicked() {
                    if let Some(path) = FileDialog::new().show_open_single_dir().ok().flatten() {
                        self.search_directory = path.to_str().unwrap().to_string();
                    }
                }

                if *self.show_stats_button.lock().unwrap() {
                    if ui.button("Show Search Statistics").clicked() {
                        *self.show_stats.lock().unwrap() = true;
                    }
                }
            });

            ui.separator();

            let status_message = self.status_message.lock().unwrap();
            ui.label(&*status_message);

            let progress = *self.progress.lock().unwrap();
            if *self.searching.lock().unwrap() {
                ui.add(egui::ProgressBar::new(progress).show_percentage().animate(true));
                ui.label(format!("Searching... {:.2}%", progress * 100.0));
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                let results = self.results.lock().unwrap();
                for (result, zip_path, is_zip_entry) in &*results {
                    ui.group(|ui| {
                        ui.label(result);
                        if ui.button("Open Location").clicked() {
                            self.open_file_explorer(result, zip_path.clone());
                        }
                        if ui.button("Open").clicked() {
                            self.open_file(result, zip_path.clone(), *is_zip_entry);
                        }
                    });
                    ui.add_space(10.0);
                }
            });

            if *self.show_stats.lock().unwrap() {
                if let Some(stats) = &*self.search_stats.lock().unwrap() {
                    egui::Window::new("Search Statistics")
                        .open(&mut *self.show_stats.lock().unwrap())
                        .show(ctx, |ui| {
                            ui.label(format!("Total files scanned: {}", stats.total_files));
                            ui.label(format!("Files matching the query: {}", stats.matched_files));
                            ui.label(format!("Total time taken: {:.2?}", stats.total_time));
                            ui.label(format!("Files processed per second: {:.2}", stats.total_files as f64 / stats.total_time.as_secs_f64()));
                        });
                }
            }
        });

        ctx.request_repaint();
    }

    fn name(&self) -> &str {
        "File Searcher"
    }
}

fn main() {
    let app = FileSearcherApp::new();
    let native_options = eframe::NativeOptions {
        initial_window_size: Some(egui::Vec2::new(800.0, 600.0)),
        ..Default::default()
    };
    eframe::run_native(Box::new(app), native_options);
}
