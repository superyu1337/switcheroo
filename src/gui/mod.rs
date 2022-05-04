#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod image;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use color_eyre::eyre::Result;

use egui::{Color32, RichText};
use image::Images;
use tegra_rcm::{Error, Payload, Rcm};

pub fn gui() -> Result<()> {
    let rcm = Rcm::new(false);
    let arc = Arc::new(Mutex::new(rcm));
    let arc_clone = arc.clone();

    let images = Images::default();

    let options = eframe::NativeOptions {
        drag_and_drop_support: true,
        icon_data: None,
        ..Default::default()
    };

    eframe::run_native(
        "Switcheroo",
        options,
        Box::new(|cc| {
            let ctx = cc.egui_ctx.clone();
            thread::spawn(move || loop {
                {
                    let lock = arc.lock();
                    if let Ok(mut inner) = lock {
                        let new = Rcm::new(false);
                        *inner = new;
                        ctx.request_repaint();
                    }
                }
                thread::sleep(Duration::from_secs(1));
            });

            Box::new(MyApp {
                switch: arc_clone,
                payload_data: None,
                executable: false,
                images,
                dark_mode: false,
                state: State::NotAvailable,
            })
        }),
    );
}

struct MyApp {
    switch: Arc<Mutex<Result<Rcm, Error>>>,
    payload_data: Option<PayloadData>,
    images: Images,
    state: State,
    executable: bool,
    dark_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Available,
    NotAvailable,
    Done,
}

struct PayloadData {
    payload: Result<Payload, Error>,
    picked_path: String,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(RichText::new("Switcheroo").size(30.0).strong());
            ui.add_space(10.0);
            ui.group(|ui| {
                ui.add_space(10.0);
                if let Some(payload_data) = &self.payload_data {
                    ui.horizontal(|ui| {
                        if payload_data.payload.is_ok() {
                            ui.label(RichText::new("Payload:").size(16.0));
                            ui.monospace(
                                RichText::new(&payload_data.picked_path)
                                    .color(Color32::BLUE)
                                    .size(16.0),
                            );
                        } else {
                            ui.label(RichText::new("Error:").color(Color32::RED).size(16.0));
                            let error = &payload_data.payload.as_ref().unwrap_err().to_string();
                            ui.monospace(RichText::new(error).color(Color32::RED).size(16.0));
                        }
                    });
                    if self.state == State::Available {
                        self.executable = true;
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Payload:").size(16.0));
                        ui.monospace(RichText::new("None").size(16.0));
                        self.executable = false;
                    });
                }
                ui.add_space(10.0);

                ui.separator();

                ui.add_space(10.0);

                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new("Select Payload").size(18.0))
                            .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new().pick_file() {
                                self.payload_data = make_payload_data(&path);
                            }
                        }

                        if ui
                            .add_enabled(
                                self.executable,
                                egui::Button::new(RichText::new("Execute").size(18.0)),
                            )
                            .clicked()
                        {
                            let payload = self
                                .payload_data
                                .as_ref()
                                .unwrap()
                                .payload
                                .as_ref()
                                .unwrap();
                            if let Ok(mut res) = self.switch.try_lock() {
                                // TODO: fix race condition
                                if let Ok(switch) = &mut *res {
                                    match execute(switch, payload) {
                                        Ok(_) => {
                                            self.state = State::Done;
                                            self.executable = false;
                                        }
                                        Err(e) => {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new("Error:")
                                                        .color(Color32::RED)
                                                        .size(18.0),
                                                );
                                                ui.monospace(
                                                    RichText::new(e.to_string())
                                                        .color(Color32::RED)
                                                        .size(18.0),
                                                );
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    });
                });
            });

            ui.centered_and_justified(|ui| {
                match self.state {
                    State::Available => {
                        self.images.connected.show_max_size(ui, ui.available_size())
                    }
                    State::NotAvailable => {
                        self.images.not_found.show_max_size(ui, ui.available_size())
                    }
                    State::Done => self.images.done.show_max_size(ui, ui.available_size()),
                };

                if self.state != State::Done {
                    let arc = self.switch.try_lock();
                    if let Ok(lock) = arc {
                        let res = &*lock;
                        match res {
                            Ok(_) => self.state = State::Available,
                            Err(_) => self.state = State::NotAvailable,
                        }
                    }
                }
            });
        });

        preview_files_being_dropped(ctx);

        // Collect dropped files:
        if !ctx.input().raw.dropped_files.is_empty() {
            // unwrap safe cause we are not empty
            let file = ctx.input().raw.dropped_files.last().unwrap().clone();
            if let Some(path) = file.path {
                self.payload_data = make_payload_data(&path);
            }
        }
    }
}

/// Preview hovering files
fn preview_files_being_dropped(ctx: &egui::Context) {
    use egui::*;

    if !ctx.input().raw.hovered_files.is_empty() {
        let mut text = "Dropping payload:\n".to_owned();
        for file in &ctx.input().raw.hovered_files {
            if let Some(path) = &file.path {
                text += &format!("\n{}", path.display());
            } else if !file.mime.is_empty() {
                text += &format!("\n{}", file.mime);
            } else {
                text += "\n???";
            }
        }

        let painter =
            ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

        let screen_rect = ctx.input().screen_rect();
        painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(192));
        painter.text(
            screen_rect.center(),
            Align2::CENTER_CENTER,
            text,
            TextStyle::Heading.resolve(&ctx.style()),
            Color32::WHITE,
        );
    }
}

/// Executes a payload returning any errors
fn execute(switch: &mut Rcm, payload: &Payload) -> Result<(), Error> {
    // its ok if it gets init more than once, it skips previous inits
    switch.init()?;
    println!("Smashing the stack!");

    // We need to read the device id first
    let _ = switch.read_device_id()?;
    switch.execute(payload)?;
    Ok(())
}

/// Makes a payload from a given file path
/// returns None on an error
fn make_payload_data(path: &Path) -> Option<PayloadData> {
    let file = std::fs::read(&path);
    if let Ok(data) = file {
        let payload_data = PayloadData {
            picked_path: path.display().to_string(),
            payload: Payload::new(&data),
        };
        return Some(payload_data);
    }
    None
}