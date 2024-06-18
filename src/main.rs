#![warn(clippy::pedantic, clippy::nursery)]
#![feature(let_chains)]

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{BufWriter, Cursor, Write};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;
use std::{array, thread};

use eframe::egui::load::Bytes;
use eframe::egui::{CentralPanel, Context, Grid, ImageSource, ViewportBuilder};
use eframe::{App, Error as EfError, Frame, NativeOptions};
use egui_plot::{Bar, BarChart, Plot};
use image::imageops::FilterType;
use image::{ImageBuffer, imageops, ImageOutputFormat, Rgb};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::Camera;

type Image = ImageBuffer<Rgb<u8>, Vec<u8>>;

fn main() -> Result<(), EfError> {
    run()
    // query_camera(ApiBackend::MediaFoundation);
    // Ok(())
}

fn run() -> Result<(), EfError> {
    let options = NativeOptions {
        viewport: ViewportBuilder::default().with_inner_size([1300.0, 740.0]).with_resizable(false),
        ..Default::default()
    };
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || process_camera(sender));
    eframe::run_native(
        "Fizyka projekt",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Box::new(MyApp {
                handle,
                receiver,
                counter: 0,
                generated: [0; 100],
                vec: VecDeque::new(),
            })
        }),
    )
}

struct Data {
    frame: Arc<[u8]>,
    difference: Arc<[u8]>,
    number: f64,
}

fn process_camera(sender: Sender<Data>) {
    let mut camera = find_camera("USB Camera").unwrap();
    let Resolution { width_x: width, height_y: height } = camera.resolution();
    let mut previous = Image::new(width, height);
    loop {
        let buffer = camera.frame().unwrap().decode_image::<RgbFormat>().unwrap();
        let mut difference = Image::new(width, height);
        let mut sum = 0u64;
        for x in 0..width {
            for y in 0..height {
                let (pixel, value) = color_difference(buffer[(x, y)], previous[(x, y)]);
                difference[(x, y)] = pixel;
                sum = sum.wrapping_add(value.wrapping_mul((x * width + height) as u64));
            }
        }
        let number = (sum % u32::MAX as u64) as f64 / u32::MAX as f64;
        let frame = convert_image(&buffer);
        let difference = convert_image(&difference);
        previous = buffer;
        sender.send(Data { frame, difference, number }).unwrap();
    }
}

fn color_difference(a: Rgb<u8>, b: Rgb<u8>) -> (Rgb<u8>, u64) {
    let mut sum = 0;
    let dif = array::from_fn(|i| {
        let mut dif = a[i].abs_diff(b[i]);
        if dif < 30 {
            dif = 0;
        }
        sum += (dif as u64) << (16 - 8 * i as u64);
        dif.saturating_mul(3)
    });
    (Rgb(dif), sum)
}

fn convert_image(buffer: &Image) -> Arc<[u8]> {
    let mut writer = BufWriter::new(Cursor::new(Vec::new()));
    imageops::resize(buffer, 640, 360, FilterType::Nearest)
        .write_to(&mut writer, ImageOutputFormat::Bmp)
        .unwrap();
    writer.flush().unwrap();
    writer.into_inner().unwrap().into_inner().into()
}

struct MyApp {
    receiver: Receiver<Data>,
    counter: usize,
    handle: JoinHandle<()>,
    generated: [usize; 100],
    vec: VecDeque<f64>,
}

impl App for MyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        let Ok(data) = self.receiver.recv() else {
            return;
        };
        CentralPanel::default().show(ctx, |ui| {
            Grid::new("grid_main").min_row_height(360.0).min_col_width(640.0).show(ui, |ui| {
                self.vec.push_back(data.number);
                if self.vec.len() > 13 * 16 {
                    for _ in 0..13 {
                        self.vec.pop_front();
                    }
                }
                let num = (data.number * 100.0) as usize;
                self.generated[num] += 1;
                let source = ImageSource::Bytes {
                    uri: Cow::from(format!("camera{}.bmp", 2 * self.counter)),
                    bytes: Bytes::Shared(data.frame),
                };
                ui.image(source);
                Plot::new("rozklad").auto_bounds_x().auto_bounds_y().show(ui, |ui| {
                    ui.bar_chart(BarChart::new(
                        self.generated
                            .into_iter()
                            .enumerate()
                            .map(|(n, h)| Bar::new(n as f64 / 100.0 + 0.005, h as f64).width(0.01))
                            .collect(),
                    ));
                });
                ui.end_row();
                let source = ImageSource::Bytes {
                    uri: Cow::from(format!("camera{}.bmp", 2 * self.counter + 1)),
                    bytes: Bytes::Shared(data.difference),
                };
                ui.image(source);
                Grid::new("numbers").show(ui, |ui| {
                    for (i, n) in self.vec.iter().enumerate() {
                        ui.label(format!("{n:.4}"));
                        if i % 13 == 12 {
                            ui.end_row();
                        }
                    }
                });
                ui.end_row();
            })
        });
        self.counter += 1;
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}

fn find_camera(name: &str) -> Option<Camera> {
    let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);
    for i in 0..1000 {
        let index = CameraIndex::Index(i);
        if let Ok(camera) = Camera::new(index, format)
            && camera.info().human_name() == name
        {
            return Some(camera);
        }
    }
    None
}

fn query_camera(backend: ApiBackend) {
    println!("{:?}", nokhwa::query(backend));
}
