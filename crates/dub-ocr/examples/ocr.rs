//! Пример ocr: детекция вшитого текста на видео -> регионы + субтитр-полоса (блюр-боксы).
//!
//!   ocr --video in.mp4 --work <dir> --models <models_root> [--fps 4]
//!
//! Печатает JSON: regions (text/bbox/t0/t1), caption_boxes, sub_y. Требует ORT_DYLIB_PATH на 1.28.2
//! (или авто-резолв через models/runtime, как в dub-asr).

use dub_ocr::{analyze_layout, detect_regions, OcrPaths};
use std::collections::HashSet;
use std::path::PathBuf;

fn main() {
    let mut video = None;
    let mut work = PathBuf::from(".");
    let mut models = PathBuf::from("models");
    let mut fps = 4i32;
    let mut spoken_arg = String::new();
    let mut frame_h = 824i64;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut next = || it.next().expect("флаг требует значение");
        match a.as_str() {
            "--video" => video = Some(PathBuf::from(next())),
            "--work" => work = PathBuf::from(next()),
            "--models" => models = PathBuf::from(next()),
            "--fps" => fps = next().parse().unwrap(),
            "--frame-h" => frame_h = next().parse().unwrap(),
            "--spoken" => spoken_arg = next(), // разделитель — запятая
            _ => {}
        }
    }
    let video = video.expect("нужен --video");
    let paths = OcrPaths::under(&models);
    if !paths.all_exist() {
        eprintln!("модели OCR не найдены в {}/ocr", models.display());
        std::process::exit(1);
    }
    let t0 = std::time::Instant::now();
    let (regions, raw) = detect_regions(&video, &work, &paths, fps, 0.3, 0.3, 8, 20.0, 0.4)
        .expect("detect_regions");
    let spoken: HashSet<String> = spoken_arg
        .split(',')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();
    let (localize, caption_boxes, sub_y) = analyze_layout(&regions, frame_h, &raw, &spoken);
    eprintln!(
        "detect: {} regions, {} raw dets, {} caption_boxes, sub_y={:?} ({:.1}s)",
        regions.len(),
        raw.len(),
        caption_boxes.len(),
        sub_y,
        t0.elapsed().as_secs_f32()
    );
    for r in regions.iter().take(20) {
        println!(
            "  [{:.2}-{:.2}] ({},{},{},{}) {:?}",
            r.t0, r.t1, r.x, r.y, r.w, r.h, r.text
        );
    }
    println!("localize={} caption_boxes={} sub_y={:?}", localize.len(), caption_boxes.len(), sub_y);
}
