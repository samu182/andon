//! 構成図の窓（Rust 版）。中身は board.py が書く ~/.herdr/kanban-state.json の "arch"（判断は arch.py）。ここは読んで描くだけ。
//!
//!   arch-view                         窓を開く
//!   arch-view --dump-layout <out> [aspect] [段の上限]   並べた結果を JSON で書く（段の上限 1 なら Python の layout と同じ答え）
//!
//! 環境変数 ARCH_BENCH=<書き出す先> を付けると、図を 1 画素ずつ動かしながら 180 回描き直し、
//! 1 回あたりの時間を書き出して閉じる（CPU で描く版と GPU で描く版を比べるため）。
//! 環境変数 ARCH_SELECT=<部品の id> を付けると、その部品を選んだ状態で開く（右の欄を撮って確かめるため）。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod draw;
mod layout;
mod model;
mod side;

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
/// state のファイルを見に行く間隔
const POLL: std::time::Duration = std::time::Duration::from_secs(2);
/// 組み立て（board.py build）を頼む決まりは、カンバンの画面と同じ: 90 秒（＋余裕 10 秒）より古ければ、20 秒に 1 回まで、重ねない
const REBUILD_EVERY: u64 = 100;
const REBUILD_AT_LEAST: u64 = 20;
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

/// 図の上の、押した・乗せた所にあるもの
enum Hit {
    Node(String),
    Edge(usize),
    Bundle(usize),
}

struct Viewer {
    state: Option<State>,
    view: Option<View>,
    lay: Option<Layout>,
    msg: String,
    /// 裏で並べている最中の結果の受け口（Graphviz に数百ミリ秒かかるので、窓の糸では待たない）。
    /// 中身（View）と並べ方は必ず組で入れ替える
    pending: Option<mpsc::Receiver<(View, Result<Layout, String>)>>,
    /// 読んだ state のファイルの更新時刻
    state_mtime: Option<std::time::SystemTime>,
    last_poll: Option<Instant>,
    /// 裏で頼んだ組み立てと、頼んだ時刻
    builder: Option<std::process::Child>,
    asked_at: Option<Instant>,
    /// 1 点 = 1 画素にしたか
    zoomed: bool,
    /// 窓の中の大きさ（画素）
    size: egui::Vec2,
    ctx: egui::Context,
    bench: Option<Bench>,
    cam: Option<draw::Camera>,
    /// 手で動かした・拡大したら、自動で収め直さない（f で戻す）
    moved: bool,
    selected: Option<String>,
    /// 中身を右に出しているまとめた線
    bundle: Option<usize>,
    hover_node: Option<String>,
    hover_edge: Option<usize>,
    side: side::Side,
    /// 上の帯に少しのあいだ出す知らせ（コピーした、など）
    flash: Option<(String, Instant)>,
}

impl Viewer {
    fn new(ctx: egui::Context) -> Self {
        let bench = std::env::var("ARCH_BENCH").ok().map(|out| Bench { out, last: None, times: vec![] });
        let mut v = Viewer {
            state: None,
            view: None,
            lay: None,
            msg: String::new(),
            pending: None,
            state_mtime: state_mtime(),
            last_poll: Some(Instant::now()),
            builder: None,
            asked_at: None,
            zoomed: false,
            size: egui::vec2(WIN_W, WIN_H),
            ctx,
            bench,
            cam: None,
            moved: false,
            selected: std::env::var("ARCH_SELECT").ok(),
            bundle: None,
            hover_node: None,
            hover_edge: None,
            side: side::Side::default(),
            flash: None,
        };
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
            // 大きさは Python 版と同じく実際の画素で決める（1 点 = 1 画素）。
            // Windows の拡大率（この PC は 150%）をかけると、窓が画面より大きくなった（2026-10-06）。
            // 窓の大きさの指示は拡大率をかけた単位で受け取られるので、拡大率で割って渡す
            if let Some(ppp) = ctx.native_pixels_per_point() {
                ctx.set_zoom_factor(1.0 / ppp);
                ctx.options_mut(|o| o.zoom_with_keyboard = false); // Ctrl+- などで egui 全体の倍率を変えさせない
                // CPU で描く部品は画面の大きさを教えてくれないので、Windows に作業領域（タスクバーを除く）を聞く
                if let Some(wa) = work_area() {
                    self.size = (wa.size() * egui::vec2(SCREEN_W, SCREEN_H)).round();
                    let title = 32.0 * ppp; // 窓の題の帯（おおよそ）
                    let at = wa.min + (wa.size() - self.size - egui::vec2(0.0, title)) / 2.0;
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition((at.to_vec2() / ppp).to_pos2()));
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.size / ppp));
                self.zoomed = true;
                if let Some(view) = self.view.clone() {
                    self.start_layout(view);
                }
                ctx.request_repaint();
            }
        }
        self.poll();
        ctx.request_repaint_after(POLL); // 何も触っていなくても、state を見に行けるように
        if let Some(rx) = &self.pending {
            if let Ok((view, r)) = rx.try_recv() {
                match r {
                    Ok(lay) => {
                        (self.view, self.lay) = (Some(view), Some(lay));
                        self.msg.clear();
                        self.moved = false;
                        self.forget_missing();
                    }
                    Err(e) => self.msg = format!("並べられない: {e}"),
                }
                self.pending = None;
            }
        }
        let area = ui.max_rect();
        let p = ui.painter().clone();
        let body = egui::Rect::from_min_max(area.min + egui::vec2(0.0, draw::HEAD_H), area.max);
        draw::paper(&p, body);
        let (f_key, esc, r_key) = ui.input(|i| (i.key_pressed(egui::Key::F), i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::R)));
        if r_key {
            self.rebuild_if_old(true); // r: 今すぐ組み立て直す（20 秒に 1 回まで）
        }
        if f_key {
            self.moved = false; // f: 全体を収め直す
        }
        if esc {
            (self.selected, self.bundle) = (None, None);
        }
        let side_open = self.selected.is_some() || self.bundle.is_some();
        // 右の欄は図の上に重ねる（出し入れで図の倍率を変えない。選ぶたびに図が縮むと見失う）
        let canvas = if side_open { egui::Rect::from_min_max(body.min, egui::pos2(body.max.x - side::WIDTH, body.max.y)) } else { body };
        let resp = ui.interact(canvas, egui::Id::new("canvas"), egui::Sense::click_and_drag());
        if let (Some(view), Some(lay), Some(st)) = (&self.view, &self.lay, &self.state) {
            if !self.moved || self.cam.is_none() {
                self.cam = Some(draw::Camera::fit(body, lay));
            }
            let cam = self.cam.as_mut().unwrap();
            if resp.dragged() {
                cam.origin += resp.drag_delta(); // ドラッグで移動
                self.moved = true;
            }
            if let (true, Some(at)) = (resp.hovered(), resp.hover_pos()) {
                // ホイールで、指している所を中心に拡大縮小（0.25 倍から 3 倍まで）
                let notches: f32 = ui.input(|i| {
                    i.events
                        .iter()
                        .filter_map(|e| match e {
                            egui::Event::MouseWheel { unit, delta, .. } => {
                                Some(if *unit == egui::MouseWheelUnit::Line { delta.y } else { delta.y / 50.0 })
                            }
                            _ => None,
                        })
                        .sum()
                });
                if notches != 0.0 {
                    let ns = (cam.scale * (notches * 0.12).exp()).clamp(0.25, 3.0);
                    cam.origin = at - (at - cam.origin) * (ns / cam.scale);
                    cam.scale = ns;
                    self.moved = true;
                }
            }
            let cam = self.cam.as_ref().unwrap();
            let fnode = self.hover_node.as_deref().or(self.selected.as_deref());
            let at = if resp.dragged() { None } else { resp.hover_pos() };
            let hit = at.and_then(|at| hit(view, lay, cam, at, fnode));
            (self.hover_node, self.hover_edge) = match &hit {
                Some(Hit::Node(id)) => (Some(id.clone()), None),
                Some(Hit::Edge(i)) => (None, Some(*i)),
                _ => (None, None),
            };
            if hit.is_some() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                (self.selected, self.bundle) = match hit {
                    Some(Hit::Node(id)) => (Some(id), None),
                    Some(Hit::Bundle(b)) => (None, Some(b)), // まとめた線: 中身を右に一覧で出す
                    _ => (None, None),
                };
            }
            let mut shown = draw::Camera { scale: cam.scale, origin: cam.origin };
            if let Some(b) = &self.bench {
                shown.origin.x += b.times.len() as f32; // 引きずって動かしているのと同じ描き直し
            }
            let focus = draw::Focus {
                selected: self.selected.as_deref(),
                hover_node: self.hover_node.as_deref(),
                hover_edge: self.hover_edge,
                bones_only: false,
            };
            let changes = st.arch.as_ref().map(|a| a.changes.clone()).unwrap_or_default();
            draw::diagram(&p.with_clip_rect(body), view, lay, &changes, &shown, &focus);
        }
        if side_open {
            self.side_panel(ui, body);
        }
        let meta = match (&self.flash, &self.state, &self.view) {
            (Some((m, at)), _, _) if at.elapsed().as_secs_f32() < 3.0 => {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
                m.clone()
            }
            (_, Some(st), Some(v)) if self.msg.is_empty() => {
                format!("{} 時点・{} 部品", st.generated_at.get(11..16).unwrap_or(""), v.nodes.len())
            }
            _ => self.msg.clone(),
        };
        draw::header(&p, area, "構成図 — 全体", "押すと詳しく　ドラッグで移動　ホイールで拡大　f 全体　r 更新　Esc 選ぶのをやめる", &meta);
        self.bench_tick(&ctx, body);
    }

    /// 右の欄（部品か、まとめた線を選んだときだけ）
    fn side_panel(&mut self, ui: &mut egui::Ui, body: egui::Rect) {
        let rect = egui::Rect::from_min_max(egui::pos2(body.max.x - side::WIDTH, body.min.y), body.max);
        let p = ui.painter();
        p.rect_filled(rect, 0.0, draw::PANEL);
        p.line_segment([rect.left_top(), rect.left_bottom()], egui::Stroke::new(1.0_f32, draw::LINE));
        let (Some(view), Some(lay), Some(st)) = (&self.view, &self.lay, &self.state) else { return };
        let inner = rect.shrink(16.0);
        let salt = format!("{:?}{:?}", self.selected, self.bundle); // 選び直したら、欄を上から
        let mut act = None;
        let (selected, bundle, side) = (&self.selected, self.bundle, &mut self.side);
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            egui::ScrollArea::vertical().id_salt(salt).auto_shrink([false, false]).show(ui, |ui| {
                ui.set_width(inner.width() - 12.0);
                act = match (selected, bundle) {
                    (Some(id), _) => view.nodes.iter().find(|n| &n.id == id).and_then(|n| side.node(ui, st, view, n)),
                    (None, Some(b)) if b < lay.bundles.len() => side.bundle(ui, view, lay, b),
                    _ => None,
                };
            });
        });
        match act {
            Some(side::Action::Select(id)) => (self.selected, self.bundle) = (Some(id), None),
            Some(side::Action::Open(url)) => ui.ctx().open_url(egui::OpenUrl::new_tab(url)),
            Some(side::Action::Copy(text)) => {
                ui.ctx().copy_text(text);
                self.flash = Some(("AI に渡す文をコピーした".into(), Instant::now()));
            }
            None => {}
        }
    }

    /// 裏で並べる（Graphviz に数百ミリ秒かかるので、窓の糸では待たない）。窓の形に合わせて段を選ぶ。
    /// 並べ終わるまでは前の図のまま。待っているあいだに次の中身が来たら、前の結果は捨てる（受け口ごと替える）
    fn start_layout(&mut self, view: View) {
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        let aspect = (self.size.x / (self.size.y - draw::HEAD_H)) as f64;
        std::thread::spawn(move || {
            let lay = layout::layout(&view, aspect, MAX_ROWS);
            let _ = tx.send((view, lay));
            ctx.request_repaint();
        });
        self.pending = Some(rx);
    }

    /// state のファイルを 2 秒ごとに見て、変わっていれば読み直す。
    /// 部品・枠・待ち・線の組み合わせが変わったときだけ並べ直す（状態が変わっただけなら、並べ方はそのまま）
    fn poll(&mut self) {
        if self.last_poll.is_some_and(|t| t.elapsed() < POLL) {
            return;
        }
        self.last_poll = Some(Instant::now());
        self.rebuild_if_old(false);
        let mtime = state_mtime();
        if mtime.is_none() || mtime == self.state_mtime {
            return;
        }
        self.state_mtime = mtime;
        match model::load_state() {
            Ok(st) => {
                if let Some(a) = &st.arch {
                    let view = model::view_all(a);
                    let same = self.pending.is_none() && self.lay.is_some() && self.view.as_ref().is_some_and(|v| shape(v) == shape(&view));
                    if same {
                        self.view = Some(view);
                        self.forget_missing();
                    } else if self.zoomed {
                        self.start_layout(view);
                    } else {
                        self.view = Some(view); // 窓の大きさが決まったら並べる
                    }
                }
                self.state = Some(st);
                if self.msg.starts_with("読めない") {
                    self.msg.clear();
                }
            }
            Err(e) => self.msg = format!("読めない: {e}"), // 描けなくても閉じない。次の回にやり直す
        }
    }

    /// 選んでいた部品・まとめた線が、新しい中身に無ければ選ぶのをやめる
    fn forget_missing(&mut self) {
        let (Some(view), Some(lay)) = (&self.view, &self.lay) else { return };
        if self.selected.as_ref().is_some_and(|id| !view.nodes.iter().any(|n| &n.id == id)) {
            self.selected = None;
        }
        if self.bundle.is_some_and(|b| b >= lay.bundles.len()) {
            self.bundle = None;
        }
    }

    /// 状態が古ければ、裏で組み立て（board.py build）を頼む（カンバンの画面を閉じていても、構成図だけで新しく保つ）。
    /// カンバンの画面が開いていれば 90 秒ごとに作り直されるので、ここからはほとんど頼まない
    fn rebuild_if_old(&mut self, force: bool) {
        if let Some(c) = &mut self.builder {
            if matches!(c.try_wait(), Ok(None)) {
                return; // まだ組み立て中（重ねない）
            }
        }
        if self.asked_at.is_some_and(|t| t.elapsed().as_secs() < REBUILD_AT_LEAST) {
            return;
        }
        let age = self.state_mtime.and_then(|m| m.elapsed().ok()).map_or(u64::MAX, |d| d.as_secs());
        if !force && age < REBUILD_EVERY {
            return;
        }
        self.asked_at = Some(Instant::now());
        let mut cmd = std::process::Command::new(python());
        layout::hidden(cmd.arg(board_py()).arg("build").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()));
        match cmd.spawn() {
            Ok(c) => {
                self.builder = Some(c);
                self.flash = Some(("組み立て直している…".into(), Instant::now()));
            }
            Err(e) => self.msg = format!("組み立てを頼めない: {e}"),
        }
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

fn state_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(model::state_path()).and_then(|m| m.modified()).ok()
}

/// 並べ方を決めるもの（部品・枠・待ち・線）。これが同じなら並べ直さない
fn shape(v: &View) -> Vec<String> {
    let mut out: Vec<String> = v.nodes.iter().map(|n| format!("{}|{}|{}", n.id, n.zone, n.blocked)).collect();
    out.sort();
    out.extend(v.edges.iter().map(|e| format!("{}>{}|{}", e.from, e.to, e.label)));
    out
}

/// カンバンの組み立て（~/.claude/skills/kanban は personal/kanban へのジャンクション）
fn board_py() -> std::path::PathBuf {
    let home = std::env::var_os("USERPROFILE").unwrap_or_default();
    std::path::PathBuf::from(home).join(".claude").join("skills").join("kanban").join("board.py")
}

/// python.exe の本物の場所（WindowsApps の起動用の別名は避ける）
fn python() -> std::path::PathBuf {
    std::env::var_os("PATH")
        .and_then(|p| {
            std::env::split_paths(&p)
                .filter(|d| !d.to_string_lossy().contains("WindowsApps"))
                .map(|d| d.join("python.exe"))
                .find(|c| c.is_file())
        })
        .unwrap_or_else(|| "python".into())
}

/// その場所のいちばん上にあるもの（部品 → 線 → まとめた線の順）。線は 4 画素まで近ければ当たり
fn hit(view: &View, lay: &Layout, cam: &draw::Camera, at: egui::Pos2, fnode: Option<&str>) -> Option<Hit> {
    for n in view.nodes.iter().rev() {
        if lay.pos.get(&n.id).is_some_and(|&c| draw::node_rect(cam, c).expand(3.0).contains(at)) {
            return Some(Hit::Node(n.id.clone()));
        }
    }
    let zone_of: std::collections::HashMap<&str, &str> = view.nodes.iter().map(|n| (n.id.as_str(), n.zone.as_str())).collect();
    let near = |pts: &[layout::Pt]| draw::distance_to(&pts.iter().map(|q| cam.xy(*q)).collect::<Vec<_>>(), at) <= 4.0;
    for (i, (e, path)) in view.edges.iter().zip(&lay.paths).enumerate() {
        let shown = zone_of[e.from.as_str()] == zone_of[e.to.as_str()] || fnode.is_some_and(|f| e.from == f || e.to == f);
        if shown && near(&path.points) {
            return Some(Hit::Edge(i));
        }
    }
    lay.bundles.iter().position(|b| near(&b.points)).map(Hit::Bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{Edge, Node, Zone};

    fn sample() -> (View, Layout) {
        let node = |id: &str| Node { id: id.into(), zone: "z".into(), status: "live".into(), ..Default::default() };
        let view = View {
            zones: vec![Zone { id: "z".into(), ..Default::default() }],
            nodes: vec![node("a"), node("b")],
            edges: vec![Edge { from: "a".into(), to: "b".into(), status: "live".into(), ..Default::default() }],
            owner: None,
        };
        let lay = Layout {
            pos: [("a".to_string(), (100.0, 100.0)), ("b".to_string(), (500.0, 100.0))].into_iter().collect(),
            frames: vec![],
            paths: vec![layout::Path { points: vec![(190.0, 100.0), (410.0, 100.0)], label_at: None }],
            size: (600.0, 200.0),
            bundles: vec![],
        };
        (view, lay)
    }

    #[test]
    fn 部品の上を押すとその部品() {
        let (view, lay) = sample();
        let cam = draw::Camera { scale: 0.5, origin: egui::pos2(10.0, 20.0) };
        let at = cam.xy((100.0, 100.0)) + egui::vec2(30.0, 5.0);
        assert!(matches!(hit(&view, &lay, &cam, at, None), Some(Hit::Node(id)) if id == "a"));
    }

    #[test]
    fn 線の近くは線_離れると何も無い() {
        let (view, lay) = sample();
        let cam = draw::Camera { scale: 1.0, origin: egui::pos2(0.0, 0.0) };
        assert!(matches!(hit(&view, &lay, &cam, egui::pos2(300.0, 103.0), None), Some(Hit::Edge(0))));
        assert!(hit(&view, &lay, &cam, egui::pos2(300.0, 110.0), None).is_none());
    }
}
