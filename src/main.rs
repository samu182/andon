//! 構成図の窓（Rust 版）。中身は board.py が書く ~/.herdr/kanban-state.json の "arch"（判断は arch.py）。ここは読んで描くだけ。
//!
//!   arch-view                         窓を開く
//!   arch-view --dump-layout <out> [aspect]   並べた結果を JSON で書く（Python の layout と突き合わせる）

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod draw;
mod layout;
mod model;

use eframe::egui;
use layout::Layout;
use model::{State, View};
use std::sync::mpsc;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--dump-layout") {
        let out = args.get(2).expect("書き出す先を渡す");
        let aspect = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.6);
        let body = match model::load_state().and_then(|st| {
            let a = st.arch.ok_or("arch が無い")?;
            layout::layout(&model::view_all(&a), aspect)
        }) {
            Ok(lay) => serde_json::to_string(&lay).unwrap(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        };
        std::fs::write(out, body).expect("書き出せない");
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 600.0]).with_title("構成図"),
        ..Default::default()
    };
    eframe::run_native(
        "構成図",
        options,
        Box::new(|cc| {
            draw::setup_fonts(&cc.egui_ctx);
            Ok(Box::new(App::new(cc.egui_ctx.clone())))
        }),
    )
}

struct App {
    state: Option<State>,
    view: Option<View>,
    lay: Option<Layout>,
    msg: String,
    /// 裏で並べている最中の結果の受け口（Graphviz に数百ミリ秒かかるので、窓の糸では待たない）
    pending: Option<mpsc::Receiver<Result<Layout, String>>>,
    /// 1 点 = 1 画素にしたか
    zoomed: bool,
}

impl App {
    fn new(ctx: egui::Context) -> Self {
        let mut app = App { state: None, view: None, lay: None, msg: String::new(), pending: None, zoomed: false };
        match model::load_state() {
            Ok(st) => {
                match &st.arch {
                    Some(a) => {
                        let view = model::view_all(a);
                        let (tx, rx) = mpsc::channel();
                        let v = view.clone();
                        std::thread::spawn(move || {
                            let _ = tx.send(layout::layout(&v, 1.6));
                            ctx.request_repaint();
                        });
                        app.view = Some(view);
                        app.pending = Some(rx);
                        app.msg = "並べている".into();
                    }
                    None => app.msg = "構成図の宣言が無い（~/.herdr/kanban-data/<project>/arch.json）".into(),
                }
                app.state = Some(st);
            }
            Err(e) => app.msg = format!("読めない: {e}"),
        }
        app
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.zoomed {
            // 大きさは Python 版と同じく実際の画素で決める（1 点 = 1 画素、窓は 1400×860 画素）。
            // Windows の拡大率（この PC は 150%）をかけると、窓が画面より大きくなった（2026-10-06）。
                // 窓の大きさの指示は拡大率をかけた単位で受け取られるので、拡大率で割って渡す
            if let Some(ppp) = ui.ctx().native_pixels_per_point() {
                ui.ctx().set_zoom_factor(1.0 / ppp);
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1400.0, 860.0) / ppp));
                self.zoomed = true;
                ui.ctx().request_repaint();
            }
        }
        if let Some(rx) = &self.pending {
            if let Ok(r) = rx.try_recv() {
                match r {
                    Ok(lay) => {
                        self.lay = Some(lay);
                        self.msg.clear();
                    }
                    Err(e) => self.msg = format!("並べられない: {e}"),
                }
                self.pending = None;
            }
        }
        let area = ui.max_rect();
        let p = ui.painter();
        let body = egui::Rect::from_min_max(area.min + egui::vec2(0.0, draw::HEAD_H), area.max);
        draw::paper(p, body);
        if let (Some(view), Some(lay), Some(st)) = (&self.view, &self.lay, &self.state) {
            let cam = draw::Camera::fit(body, lay);
            let changes = st.arch.as_ref().map(|a| &a.changes).unwrap_or(&serde_json::Value::Null);
            draw::diagram(&p.with_clip_rect(body), view, lay, changes, &cam);
        }
        let meta = match (&self.state, &self.view) {
            (Some(st), Some(v)) if self.msg.is_empty() => {
                format!("{} 時点・{} 部品", st.generated_at.get(11..16).unwrap_or(""), v.nodes.len())
            }
            _ => self.msg.clone(),
        };
        draw::header(p, area, "構成図 — 全体", &meta);
    }
}
