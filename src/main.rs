//! 構成図の窓（Rust 版）。中身は board.py が書く ~/.herdr/kanban-state.json の "arch"（判断は arch.py）。ここは読んで描くだけ。
//!
//!   arch-view                         窓を開く
//!   arch-view --dump-layout <out> [aspect] [段の上限]   並べた結果を JSON で書く（段の上限 1 なら Python の layout と同じ答え）
//!
//! 環境変数 ARCH_BENCH=<書き出す先> を付けると、図を 1 画素ずつ動かしながら 180 回描き直し、
//! 1 回あたりの時間を書き出して閉じる（CPU で描く版と GPU で描く版を比べるため）。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod draw;
mod layout;
mod model;

use layout::Layout;
use model::{State, View};
use std::sync::mpsc;
use std::time::Instant;

/// 画面の大きさが分からないときの窓（Python 版と同じ 1400×860 画素）
const WIN_W: f32 = 1400.0;
const WIN_H: f32 = 860.0;
/// 窓は画面のこの割合（全体の図を窓に収めると小さくなるので、画面をなるべく使う・2026-10-06）
const SCREEN_W: f32 = 0.92;
const SCREEN_H: f32 = 0.90;
/// 枠を並べる段の上限
const MAX_ROWS: usize = 3;
const BENCH_FRAMES: usize = 180;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--dump-layout") {
        let out = args.get(2).expect("書き出す先を渡す");
        let aspect = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.6);
        let max_rows = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1);
        let body = match model::load_state().and_then(|st| {
            let a = st.arch.ok_or("arch が無い")?;
            layout::layout(&model::view_all(&a), aspect, max_rows)
        }) {
            Ok(lay) => serde_json::to_string(&lay).unwrap(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        };
        std::fs::write(out, body).expect("書き出せない");
        return;
    }
    run();
}

/// CPU で描く（既定）。GPU のドライバーを読み込まないので、空の窓が約 13 MB（GPU 版は約 97 MB・2026-10-06 に測った）
#[cfg(feature = "cpu")]
fn run() {
    use egui_software_backend::{SoftwareBackend, SoftwareBackendAppConfiguration};

    struct Cpu(Viewer);
    impl egui_software_backend::App for Cpu {
        fn ui(&mut self, ui: &mut egui::Ui, _backend: &mut SoftwareBackend) {
            self.0.ui(ui);
        }
    }
    let settings = SoftwareBackendAppConfiguration::new().inner_size(Some(egui::vec2(960.0, 600.0))).title(Some("構成図".into()));
    egui_software_backend::run_app_with_software_backend(settings, |ctx: egui::Context| {
        draw::setup_fonts(&ctx);
        Cpu(Viewer::new(ctx))
    })
    .expect("窓を作れない");
}

/// GPU で描く（eframe + glow）
#[cfg(all(feature = "gpu", not(feature = "cpu")))]
fn run() {
    struct Gpu(Viewer);
    impl eframe::App for Gpu {
        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            self.0.ui(ui);
        }
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
            Ok(Box::new(Gpu(Viewer::new(cc.egui_ctx.clone()))))
        }),
    )
    .expect("窓を作れない");
}

/// 画面の作業領域（タスクバーを除いた所・実際の画素）
#[cfg(windows)]
fn work_area() -> Option<egui::Rect> {
    #[repr(C)]
    struct Rect32 {
        l: i32,
        t: i32,
        r: i32,
        b: i32,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, out: *mut Rect32, ini: u32) -> i32;
    }
    const SPI_GETWORKAREA: u32 = 0x0030;
    let mut r = Rect32 { l: 0, t: 0, r: 0, b: 0 };
    let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r, 0) } != 0;
    (ok && r.r > r.l).then(|| egui::Rect::from_min_max(egui::pos2(r.l as f32, r.t as f32), egui::pos2(r.r as f32, r.b as f32)))
}

#[cfg(not(windows))]
fn work_area() -> Option<egui::Rect> {
    None
}

/// 描き直しの速さを測る（ARCH_BENCH）
struct Bench {
    out: String,
    last: Option<Instant>,
    times: Vec<f64>,
}

struct Viewer {
    state: Option<State>,
    view: Option<View>,
    lay: Option<Layout>,
    msg: String,
    /// 裏で並べている最中の結果の受け口（Graphviz に数百ミリ秒かかるので、窓の糸では待たない）
    pending: Option<mpsc::Receiver<Result<Layout, String>>>,
    /// 1 点 = 1 画素にしたか
    zoomed: bool,
    /// 窓の中の大きさ（画素）
    size: egui::Vec2,
    ctx: egui::Context,
    bench: Option<Bench>,
}

impl Viewer {
    fn new(ctx: egui::Context) -> Self {
        let bench = std::env::var("ARCH_BENCH").ok().map(|out| Bench { out, last: None, times: vec![] });
        let size = egui::vec2(WIN_W, WIN_H);
        let mut v = Viewer { state: None, view: None, lay: None, msg: String::new(), pending: None, zoomed: false, size, ctx, bench };
        match model::load_state() {
            Ok(st) => {
                match &st.arch {
                    Some(a) => {
                        v.view = Some(model::view_all(a));
                        v.msg = "並べている".into();
                    }
                    None => v.msg = "構成図の宣言が無い（~/.herdr/kanban-data/<project>/arch.json）".into(),
                }
                v.state = Some(st);
            }
            Err(e) => v.msg = format!("読めない: {e}"),
        }
        v
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.zoomed {
            // 大きさは Python 版と同じく実際の画素で決める（1 点 = 1 画素、窓は 1400×860 画素）。
            // Windows の拡大率（この PC は 150%）をかけると、窓が画面より大きくなった（2026-10-06）。
            // 窓の大きさの指示は拡大率をかけた単位で受け取られるので、拡大率で割って渡す
            if let Some(ppp) = ctx.native_pixels_per_point() {
                ctx.set_zoom_factor(1.0 / ppp);
                // CPU で描く部品は画面の大きさを教えてくれないので、Windows に作業領域（タスクバーを除く）を聞く
                if let Some(wa) = work_area() {
                    self.size = (wa.size() * egui::vec2(SCREEN_W, SCREEN_H)).round();
                    let title = 32.0 * ppp; // 窓の題の帯（おおよそ）
                    let at = wa.min + (wa.size() - self.size - egui::vec2(0.0, title)) / 2.0;
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition((at.to_vec2() / ppp).to_pos2()));
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.size / ppp));
                self.zoomed = true;
                self.start_layout();
                ctx.request_repaint();
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
            let mut cam = draw::Camera::fit(body, lay);
            if let Some(b) = &self.bench {
                cam.origin.x += b.times.len() as f32; // 引きずって動かしているのと同じ描き直し
            }
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
        self.bench_tick(&ctx, body);
    }

    /// 裏で並べる（Graphviz に数百ミリ秒かかるので、窓の糸では待たない）。窓の形に合わせて段を選ぶ
    fn start_layout(&mut self) {
        let Some(view) = &self.view else { return };
        let (tx, rx) = mpsc::channel();
        let (copy, ctx) = (view.clone(), self.ctx.clone());
        let aspect = (self.size.x / (self.size.y - draw::HEAD_H)) as f64;
        std::thread::spawn(move || {
            let _ = tx.send(layout::layout(&copy, aspect, MAX_ROWS));
            ctx.request_repaint();
        });
        self.pending = Some(rx);
    }

    /// 図が出て窓が決めた大きさになってから、毎回の間隔を測る。最初の 10 回は捨てる
    fn bench_tick(&mut self, ctx: &egui::Context, body: egui::Rect) {
        let ready = self.lay.is_some() && body.width() >= self.size.x - 2.0;
        let Some(b) = &mut self.bench else { return };
        if !ready {
            ctx.request_repaint();
            return;
        }
        let now = Instant::now();
        if let Some(last) = b.last {
            b.times.push((now - last).as_secs_f64() * 1000.0);
        }
        b.last = Some(now);
        if b.times.len() < BENCH_FRAMES + 10 {
            ctx.request_repaint();
            return;
        }
        let mut t: Vec<f64> = b.times[10..].to_vec();
        t.sort_by(f64::total_cmp);
        let avg = t.iter().sum::<f64>() / t.len() as f64;
        let p95 = t[t.len() * 95 / 100];
        let _ = std::fs::write(&b.out, format!("{{\"avg_ms\": {avg:.2}, \"p95_ms\": {p95:.2}, \"frames\": {}}}", t.len()));
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
