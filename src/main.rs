//! あんどん（構成図の窓・Rust 版）。中身は board.py が書く ~/.herdr/kanban-state.json の "arch"（判断は arch.py）。ここは読んで描くだけ。
//!
//!   andon                         窓を開く（範囲は herdr で今選んでいるペインのプロジェクト。リポジトリでなければ全体）
//!   andon <project>|all           その範囲のタブで開く
//! タブは最初から全体と、構成図にあるプロジェクト全部（kanban.json の "order" の順）。閉じない。
//! すでに開いていれば、開いている窓に「その範囲（渡された範囲か、今選んでいるペインのプロジェクト）のタブに切り替えて前に出て」と
//! 知らせて終わる（窓は増やさない）。
//!   andon --dump-layout <out> [aspect] [段の上限] [future]   並べた結果を JSON で書く（段の上限 1 なら Python の layout と
//!                                                    同じ答え。future を付けると完成図）
//!   andon --export [<project>|all] [書き出す先]      その範囲を、ブラウザで操作できる HTML に書き出す（既定は全体・
//!                                                    ~/Downloads）。窓の中では上の帯の「書き出す」か e キー
//!   andon --export-share [<project>|all] [書き出す先]  人に渡す用（AI に渡す文・ブランチ・ずれ・確かめた結果を抜く）。
//!                                                    窓の中では上の帯の「共有用」か Shift+E
//!
//! 環境変数 ARCH_BENCH=<書き出す先> を付けると、図を 1 画素ずつ動かしながら 180 回描き直し、
//! 1 回あたりの時間を書き出して閉じる（CPU で描く版と GPU で描く版を比べるため）。
//! 環境変数 ARCH_SELECT=<部品の id> を付けると、その部品を選んだ状態で開く（右の欄を撮って確かめるため）。
//! ARCH_SELECT=list:drift / list:changes なら、図と現物のずれ・前回からの変化の一覧を出して開く。
//! 環境変数 ARCH_FUTURE=1 を付けると、完成図（構想を薄く足した図）で開く。窓の中では上の帯の「今｜完成図」か k キー。
//! 環境変数 ARCH_BOARD=1 を付けると、カンバンを出した状態で開く（b で図と切り替え）。
//! 環境変数 ARCH_PORT=<番号> を付けると、2 つ目を開かないための口の番号を替える（開いている窓に触らずに確かめるため）。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod draw;
mod er;
mod export;
mod kanban;
mod layout;
mod model;
mod side;
mod win;

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
/// 状態がこれより古ければ、上の帯の右に「何時時点」を出す（秒）
const STALE_AFTER: u64 = 300;
const REBUILD_AT_LEAST: u64 = 20;
const BENCH_FRAMES: usize = 180;
/// 作業中のライトが灯る・消えるを入れ替える間隔（ミリ秒）
const BLINK: u64 = 800;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--dump-layout") {
        let out = args.get(2).expect("書き出す先を渡す");
        let aspect = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.6);
        let max_rows = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1);
        let full = args.get(5).map(String::as_str) == Some("future");
        let body = match model::load_state().and_then(|st| {
            let a = st.arch.ok_or("arch が無い")?;
            let view = if full { model::view_mode(&a, "", None, true) } else { model::view_all(&a, None) };
            layout::layout(&view, aspect, max_rows)
        }) {
            Ok(lay) => serde_json::to_string(&lay).unwrap(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        };
        std::fs::write(out, body).expect("書き出せない");
        return;
    }
    if let Some(share) = args.get(1).and_then(|a| match a.as_str() {
        "--export" => Some(false),
        "--export-share" => Some(true),
        _ => None,
    }) {
        let scope = match args.get(2).map(String::as_str) {
            None | Some("all") => String::new(),
            Some(p) => p.to_string(),
        };
        let r = model::load_state().and_then(|st| {
            let a = st.arch.as_ref().ok_or("arch が無い")?;
            let view = model::view_for(a, &scope, None);
            if !scope.is_empty() && view.nodes.is_empty() {
                return Err(format!("{scope} の部品が無い"));
            }
            let lay = layout::layout(&view, 1.6, MAX_ROWS)?;
            // 構想があれば完成図も並べて入れる（書き出した HTML でも「今｜完成図」を切り替えられる）
            let full = if a.future.is_empty() {
                None
            } else {
                let v = model::view_mode(a, &scope, None, true);
                let l = layout::layout(&v, 1.6, MAX_ROWS)?;
                Some((v, l))
            };
            let path = args.get(3).map(std::path::PathBuf::from).unwrap_or_else(|| export::default_path(&st, &scope, share));
            let full = full.as_ref().map(|(v, l)| (v, l));
            export::write(&path, &st, (&view, &lay), full, &scope, &Default::default(), share, false)?;
            Ok(path)
        });
        // release では窓のプログラム（windows_subsystem）なので、コンソールには出ない。結果はファイルがあるかで見る
        match r {
            Ok(p) => println!("{}", p.display()),
            Err(e) => {
                eprintln!("書き出せない: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let fixed = args.get(1).filter(|a| !a.starts_with("--")).cloned();
    let repos = || model::load_state().ok().and_then(|s| s.arch).map(|a| a.repos).unwrap_or_default();
    // 今選んでいるペインの範囲（リポジトリならそのプロジェクト・並べたフォルダなら全体・関係ないフォルダなら無し）
    let focused = || win::focused_cwd().and_then(|c| model::project_of(&c, &repos()));
    // 範囲を渡されていればそれを、無ければ今選んでいるペインの範囲を知らせる
    let wanted = || match fixed.as_deref() {
        Some("all") => Some(String::new()),
        Some(p) => Some(p.to_string()),
        None => focused(),
    };
    let Some(listener) = win::claim(|| wanted().map_or("show".to_string(), |s| format!("open:{s}"))) else {
        return; // もう開いている（知らせは送った）
    };
    let scope = wanted().unwrap_or_default();
    run(scope, listener);
}

/// CPU で描く（既定）。GPU のドライバーを読み込まないので、空の窓が約 13 MB（GPU 版は約 97 MB・2026-10-06 に測った）
#[cfg(feature = "cpu")]
fn run(scope: String, listener: std::net::TcpListener) {
    use egui_software_backend::{SoftwareBackend, SoftwareBackendAppConfiguration};

    struct Cpu(Viewer);
    impl egui_software_backend::App for Cpu {
        fn ui(&mut self, ui: &mut egui::Ui, _backend: &mut SoftwareBackend) {
            self.0.ui(ui);
        }
    }
    // 描いた絵をためておく仕組み（caching）は切る。図を動かすたびに作り直すので、約 37 MB 使ったうえに遅かった
    // （描き直し 1 回 25.5 ms → 切ると 20.0 ms・2026-10-06 に測った）
    let settings = SoftwareBackendAppConfiguration::new()
        .inner_size(Some(egui::vec2(960.0, 600.0)))
        .title(Some("あんどん".into()))
        .icon(Some(std::sync::Arc::new(icon())))
        .caching(false);
    egui_software_backend::run_app_with_software_backend(settings, |ctx: egui::Context| {
        draw::setup_fonts(&ctx);
        Cpu(Viewer::new(ctx, scope.clone(), Some(listener.try_clone().expect("口を写せない"))))
    })
    .expect("窓を作れない");
}

/// GPU で描く（eframe + glow）
#[cfg(all(feature = "gpu", not(feature = "cpu")))]
fn run(scope: String, listener: std::net::TcpListener) {
    struct Gpu(Viewer);
    impl eframe::App for Gpu {
        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            self.0.ui(ui);
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 600.0]).with_title("あんどん").with_icon(icon()),
        ..Default::default()
    };
    eframe::run_native(
        "あんどん",
        options,
        Box::new(|cc| {
            draw::setup_fonts(&cc.egui_ctx);
            Ok(Box::new(Gpu(Viewer::new(cc.egui_ctx.clone(), scope, Some(listener)))))
        }),
    )
    .expect("窓を作れない");
}

/// 窓の題の帯とタスクバーのアイコン（あんどん。assets/make_icon.py が作る 64×64 の RGBA）
fn icon() -> egui::IconData {
    egui::IconData { rgba: include_bytes!("../assets/icon-64.rgba").to_vec(), width: 64, height: 64 }
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

/// タブ（範囲ごと）。見ていた場所を覚えておき、戻ったら同じ場所から
#[derive(Default)]
struct Tab {
    /// プロジェクト名。空なら全体
    scope: String,
    cam: Option<draw::Camera>,
    moved: bool,
    selected: Option<String>,
    bundle: Option<usize>,
    flow: Option<usize>,
    picked: Option<String>,
    view: Option<View>,
    lay: Option<Layout>,
    stash: Option<(View, Layout)>,
}

/// DB の図。key（DB の部品と表の中身）が替わったら並べ直す
struct ErState {
    key: String,
    lay: Result<er::ErLayout, String>,
    cam: Option<draw::Camera>,
    moved: bool,
    /// 押した表（下に、書く部品・読む部品を出す）
    picked: Option<String>,
    hover: Option<String>,
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
    /// 今と完成図のうち、出していない方の中身と並べ方（切り替えるたびに並べ直さない）
    stash: Option<(View, Layout)>,
    /// 完成図（構想を薄く足した図）を出しているか。全部のタブで共通（タブごとには覚えない）
    full: bool,
    /// 図の代わりにカンバンを出しているか（b・上の帯の「図｜カンバン」）。全部のタブで共通
    board_mode: bool,
    board: kanban::Board,
    /// DB の図（DB の部品を選んでいるとき、左半分に出す）
    er: Option<ErState>,
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
    /// 右の欄に出している一覧（図と現物のずれ・前回からの変化）
    list: Option<draw::HeadHit>,
    /// 右の欄に前回出していたもの（替わったら、欄をいちばん上から見せる）
    side_shown: String,
    hover_node: Option<String>,
    hover_edge: Option<usize>,
    side: side::Side,
    /// 上の帯に少しのあいだ出す知らせ（コピーした、など）
    flash: Option<(String, Instant)>,
    /// s で、ウォーキングスケルトン以外を薄くする
    bones_only: bool,
    /// 骨格をたどっているときの番号（bones の添字）と、再生で次に進む時刻
    flow: Option<usize>,
    playing: Option<Instant>,
    /// 全体で、帯の名前を押して選んだ骨格のプロジェクト
    picked: Option<String>,
    /// 図を描いている所（骨格をたどるときに、部品が見えているかを調べる）
    canvas: egui::Rect,
    /// 今のタブの範囲（プロジェクト名、空なら全体）。ほかのタブは tabs に覚えておく（今のタブの中身は self の欄が正）
    scope: String,
    tabs: Vec<Tab>,
    cur: usize,
    /// 2 つ目を開こうとしたときの知らせの受け口
    inbox: Option<mpsc::Receiver<win::Msg>>,
    /// 開いてから少し待って前に出す時刻（裏から立てた窓は Windows が前に出さない）
    front_at: Option<Instant>,
    /// 作業中の部品と、ペインのブランチの受け口（裏の糸が 5 秒ごとに送ってくる）
    working: std::collections::HashMap<String, model::Work>,
    branches: std::collections::BTreeMap<String, bool>,
    branch_rx: Option<mpsc::Receiver<std::collections::BTreeMap<String, bool>>>,
}

impl Viewer {
    fn new(ctx: egui::Context, scope: String, listener: Option<std::net::TcpListener>) -> Self {
        let real = listener.is_some(); // テストでは herdr も git も呼ばない
        let inbox = listener.map(|l| {
            let (tx, rx) = mpsc::channel();
            win::listen(l, tx, ctx.clone());
            rx
        });
        let branch_rx = real.then(|| {
            let (tx, rx) = mpsc::channel();
            win::watch_branches(tx, ctx.clone());
            rx
        });
        let bench = std::env::var("ARCH_BENCH").ok().map(|out| Bench { out, last: None, times: vec![] });
        let mut v = Viewer {
            state: None,
            view: None,
            lay: None,
            stash: None,
            full: std::env::var("ARCH_FUTURE").is_ok_and(|s| s == "1"),
            board_mode: std::env::var("ARCH_BOARD").is_ok_and(|s| s == "1"),
            board: Default::default(),
            er: None,
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
            selected: std::env::var("ARCH_SELECT").ok().filter(|s| !s.starts_with("list:")),
            bundle: None,
            list: match std::env::var("ARCH_SELECT").as_deref() {
                Ok("list:drift") => Some(draw::HeadHit::Drift),
                Ok("list:changes") => Some(draw::HeadHit::Changes),
                _ => None,
            },
            hover_node: None,
            hover_edge: None,
            side_shown: String::new(),
            side: side::Side::default(),
            flash: None,
            bones_only: false,
            flow: None,
            playing: None,
            picked: None,
            canvas: egui::Rect::NOTHING,
            tabs: vec![Tab { scope: scope.clone(), ..Default::default() }],
            scope,
            cur: 0,
            inbox,
            front_at: Some(Instant::now() + std::time::Duration::from_millis(300)),
            working: Default::default(),
            branches: Default::default(),
            branch_rx,
        };
        match model::load_state() {
            Ok(st) => {
                match &st.arch {
                    Some(a) => {
                        v.view = Some(model::view_mode(a, &v.scope, None, v.full));
                        v.msg = "並べている".into();
                    }
                    None => v.msg = "構成図の宣言が無い（~/.herdr/kanban-data/<project>/arch.json）".into(),
                }
                v.state = Some(st);
                v.sync_tabs();
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
        if let Some(b) = self.branch_rx.as_ref().and_then(|rx| rx.try_iter().last()) {
            self.branches = b;
        }
        self.working = match self.state.as_ref() {
            Some(st) => model::working(st.arch.as_ref().map_or(&[][..], |a| &a.nodes), &st.cards, &st.projects, &self.branches),
            None => Default::default(),
        };
        let msgs: Vec<win::Msg> = self.inbox.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for m in msgs {
            if let win::Msg::Open(scope) = m {
                self.open_tab(scope); // 2 回目に押されたら: そのプロジェクトのタブに切り替える
            }
            self.front_at = Some(Instant::now());
        }
        if let Some(at) = self.front_at {
            if Instant::now() >= at {
                win::bring_front();
                self.front_at = None;
            } else {
                ctx.request_repaint_after(at - Instant::now());
            }
        }
        // タブは全部決まったもの（閉じない）。Ctrl+Tab で次へ
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Tab)) {
            self.switch((self.cur + 1) % self.tabs.len());
        }
        if let Some(rx) = &self.pending {
            if let Ok((view, r)) = rx.try_recv() {
                match r {
                    Ok(lay) => {
                        // 今と完成図を切り替えて並べ直したなら、前の図の並べ方は覚えておく（戻すときに並べ直さない）
                        if self.view.as_ref().is_some_and(|v| v.full != view.full) {
                            if let (Some(v), Some(l)) = (self.view.take(), self.lay.take()) {
                                self.stash = Some((v, l));
                            }
                        }
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
        // 下の帯（窓の幅いっぱい）: ウォーキングスケルトンを通る順に部品を並べ、どこまで本番で通ったかを一目で見せる
        let strip = egui::Rect::from_min_max(egui::pos2(area.min.x, area.max.y - draw::STRIP_H), area.max);
        let tabbar = egui::Rect::from_min_size(area.min + egui::vec2(0.0, draw::HEAD_H), egui::vec2(area.width(), draw::TAB_H));
        let body = egui::Rect::from_min_max(egui::pos2(area.min.x, tabbar.max.y), egui::pos2(area.max.x, strip.min.y));
        draw::paper(&p, body);
        let keys = ui.input(|i| {
            use egui::Key;
            [Key::F, Key::Escape, Key::R, Key::S, Key::ArrowRight, Key::ArrowLeft, Key::Space, Key::E, Key::K, Key::B].map(|k| i.key_pressed(k))
        });
        let [f_key, esc, r_key, s_key, right, left, space, e_key, k_key, b_key] = keys;
        if b_key {
            self.board_mode = !self.board_mode; // b: 図とカンバンを切り替える
        }
        if k_key {
            self.set_full(!self.full); // k: 今の図と完成図（構想を薄く足した図）を切り替える（f は「全体を収め直す」に使っている）
        }
        if e_key {
            // e: 今のタブを、ブラウザで操作できる HTML に書き出して開く。Shift+E は共有用
            self.export(ui.input(|i| i.modifiers.shift));
        }
        if s_key {
            self.bones_only = !self.bones_only; // s: 骨格（ウォーキングスケルトン）以外を薄くする
        }
        if right {
            self.flow_go(1);
        }
        if left {
            self.flow_go(-1);
        }
        if space {
            self.flow_play();
        }
        self.flow_tick(&ctx);
        if r_key {
            self.rebuild_if_old(true); // r: 今すぐ組み立て直す（20 秒に 1 回まで）
        }
        if f_key {
            self.moved = false; // f: 全体を収め直す
        }
        if esc {
            (self.selected, self.bundle, self.flow, self.playing, self.list) = (None, None, None, None, None); // 骨格をたどるのも終わる
        }
        if self.selected.is_some() || self.bundle.is_some() {
            self.list = None; // 部品やまとめた線を選んだら、一覧はたたむ
        }
        let side_open = !self.board_mode && (self.selected.is_some() || self.bundle.is_some() || self.list.is_some());
        // 右の欄は図の上に重ねる（出し入れで図の倍率を変えない。選ぶたびに図が縮むと見失う）
        let canvas = if side_open { egui::Rect::from_min_max(body.min, egui::pos2(body.max.x - side::WIDTH, body.max.y)) } else { body };
        // DB の図を開いているあいだは、あんどんの図をその右（右の欄との間）に収め直す。かぶせると部品が途中で切れて見えた
        let er_w = self.er_width(body);
        let canvas = egui::Rect::from_min_max(egui::pos2(canvas.min.x + er_w, canvas.min.y), canvas.max);
        let fit_area = if er_w > 0.0 { canvas } else { body };
        self.canvas = canvas;
        if self.board_mode {
            self.kanban(ui, body);
        }
        let resp = ui.interact(if self.board_mode { egui::Rect::NOTHING } else { canvas }, egui::Id::new("canvas"), egui::Sense::click_and_drag());
        if let (false, Some(view), Some(lay), Some(st)) = (self.board_mode, &self.view, &self.lay, &self.state) {
            if !self.moved || self.cam.is_none() {
                self.cam = Some(draw::Camera::fit(fit_area, lay));
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
            // 作業中のライトの点滅。描き直しは BLINK に 1 回だけで、作業中の部品が図に無いときと窓を最小化しているときは頼まない
            // （CPU で描くので 1 回 12 ms ほどかかる。0.8 秒ごとなら CPU のコア 1 つの 2% ほど）
            let blinking = view.nodes.iter().any(|n| self.working.contains_key(&n.id))
                && !ctx.input(|i| i.viewport().minimized.unwrap_or(false));
            let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
            if blinking {
                ctx.request_repaint_after(std::time::Duration::from_millis(BLINK - ms % BLINK));
            }
            let focus = draw::Focus {
                selected: self.selected.as_deref(),
                hover_node: self.hover_node.as_deref(),
                hover_edge: self.hover_edge,
                bones_only: self.bones_only,
                working: Some(&self.working),
                lit: !blinking || (ms / BLINK) % 2 == 0,
            };
            let none = model::Changes::default();
            let changes = st.arch.as_ref().map_or(&none, |a| &a.changes);
            let clip = egui::Rect::from_min_max(egui::pos2(body.min.x + er_w, body.min.y), body.max);
            draw::diagram(&p.with_clip_rect(clip), view, lay, changes, &st.projects, &shown, &focus);
        }
        if !self.board_mode {
            self.er_panel(ui, body);
        }
        self.strip(ui, &p, strip);
        self.tabbar(ui, &p, tabbar);
        if side_open {
            self.side_panel(ui, body);
        }
        let meta = match (&self.flash, &self.state, &self.view) {
            (Some((m, at)), _, _) if at.elapsed().as_secs_f32() < 3.0 => {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
                m.clone()
            }
            (_, Some(st), Some(_)) if self.msg.is_empty() => {
                // 時刻は、組み立て直しが追いついていない（古い）ときだけ出す
                let age = self.state_mtime.and_then(|m| m.elapsed().ok()).map_or(0, |d| d.as_secs());
                if age > STALE_AFTER { format!("{} 時点", st.generated_at.get(11..16).unwrap_or("")) } else { String::new() }
            }
            _ => self.msg.clone(),
        };
        let (n_drift, n_change) = match (&self.state, &self.view) {
            (Some(st), Some(v)) => {
                let ids: std::collections::HashSet<&str> = v.nodes.iter().map(|n| n.id.as_str()).collect();
                let changed = st.arch.as_ref().map_or(0, |a| a.changes.nodes.keys().filter(|k| ids.contains(k.as_str())).count());
                (side::drift_count(st, v), changed)
            }
            _ => (0, 0),
        };
        // 構想があるときだけ「今｜完成図」を出す（無ければ上の帯は前のまま）
        let has_future = self.state.as_ref().and_then(|s| s.arch.as_ref()).is_some_and(|a| !a.future.is_empty());
        let mode = (!self.board_mode && (has_future || self.full)).then_some(self.full);
        let head = draw::header(&p, area, &meta, n_drift, n_change, mode, self.board_mode);
        let head_resp = ui.interact(egui::Rect::from_min_size(area.min, egui::vec2(area.width(), draw::HEAD_H)), egui::Id::new("header"), egui::Sense::click());
        let over = head_resp.hover_pos().and_then(|at| head.iter().find(|h| h.0.contains(at)).map(|h| h.1));
        if over.is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        match (head_resp.clicked(), over) {
            (true, Some(draw::HeadHit::Export)) => self.export(false),
            (true, Some(draw::HeadHit::Share)) => self.export(true),
            (true, Some(draw::HeadHit::Present)) => self.set_full(false),
            (true, Some(draw::HeadHit::Full)) => self.set_full(true),
            (true, Some(draw::HeadHit::Map)) => self.board_mode = false,
            (true, Some(draw::HeadHit::Board)) => self.board_mode = true,
            (true, Some(hit)) => (self.list, self.selected, self.bundle) = (Some(hit), None, None),
            _ => {}
        }
        self.bench_tick(&ctx, body);
    }

    /// 今の図と完成図を切り替える。もう一方の並べ方を覚えていれば並べ直さない（rebuild が入れ替える）
    fn set_full(&mut self, full: bool) {
        if self.full == full {
            return;
        }
        self.full = full;
        self.pending = None; // 並べている途中の結果は前の図のもの
        self.bundle = None; // まとめた線の番号は図ごとに違う
        self.rebuild();
    }

    /// 並べるときの窓の形（幅 / 高さ）
    fn aspect(&self) -> f64 {
        (self.size.x / (self.size.y - draw::HEAD_H - draw::TAB_H - draw::STRIP_H)) as f64
    }

    /// 今のタブ（窓で見ている中身と並べ方そのまま）を HTML に書き出し、既定のブラウザで開く。share なら共有用。
    /// 構想があれば、出していない方の図（今か完成図）も入れる。覚えている並べ方が無ければここで並べる
    fn export(&mut self, share: bool) {
        let (Some(view), Some(lay), Some(st)) = (&self.view, &self.lay, &self.state) else {
            self.flash = Some(("まだ並べている（並べ終わってから書き出す）".into(), Instant::now()));
            return;
        };
        let other = match st.arch.as_ref().filter(|a| !a.future.is_empty()) {
            None => None,
            Some(a) => {
                let fresh = model::view_mode(a, &self.scope, self.picked.as_deref(), !view.full);
                match &self.stash {
                    Some((v, l)) if v.full == fresh.full && shape(v) == shape(&fresh) => Some((fresh, l.clone())),
                    _ => match layout::layout(&fresh, self.aspect(), MAX_ROWS) {
                        Ok(l) => Some((fresh, l)),
                        Err(e) => {
                            self.flash = Some((format!("書き出せない: {e}"), Instant::now()));
                            return;
                        }
                    },
                }
            }
        };
        let other = other.as_ref().map(|(v, l)| (v, l));
        let ((now, full), start_full) = match other {
            Some(o) if view.full => ((o, Some((view, lay))), true),
            o => (((view, lay), o), false),
        };
        let path = export::default_path(st, &self.scope, share);
        let msg = match export::write(&path, st, now, full, &self.scope, &self.working, share, start_full) {
            Ok(()) => {
                self.ctx.open_url(egui::OpenUrl::new_tab(export::file_url(&path)));
                format!("書き出した: {}", path.display())
            }
            Err(e) => format!("書き出せない: {e}"),
        };
        self.flash = Some((msg, Instant::now()));
    }

    /// 下の帯を描き、押されたら: 名前 → 出す骨格を次のプロジェクトへ / 部品 → その部品を選んで図の真ん中へ
    fn strip(&mut self, ui: &mut egui::Ui, p: &egui::Painter, r: egui::Rect) {
        let Some(view) = &self.view else {
            p.rect_filled(r, 0.0, draw::PANEL);
            return;
        };
        let hits = draw::strip(p, r, view, self.selected.as_deref());
        let resp = ui.interact(r, egui::Id::new("strip"), egui::Sense::click());
        let at = resp.hover_pos();
        let over = at.and_then(|at| hits.iter().find(|h| h.0.contains(at)));
        if over.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if !resp.clicked() {
            return;
        }
        match over.map(|h| &h.1) {
            Some(draw::StripHit::Owner) => {
                let i = view.owners.iter().position(|o| Some(o) == view.owner.as_ref()).map_or(0, |i| (i + 1) % view.owners.len());
                self.picked = view.owners.get(i).cloned();
                self.flow = None;
                self.rebuild(); // 骨格の番号だけ変わる（並べ方はそのまま）
            }
            Some(draw::StripHit::Node(id)) => {
                let id = id.clone();
                (self.selected, self.bundle) = (Some(id.clone()), None);
                self.center_on(&id, true);
            }
            None => {}
        }
    }

    /// その部品を図の真ん中へ（always でなければ、見えていないときだけ）
    fn center_on(&mut self, id: &str, always: bool) {
        let (Some(lay), Some(cam)) = (&self.lay, &mut self.cam) else { return };
        let Some(&c) = lay.pos.get(id) else { return };
        let at = cam.xy(c);
        if always || !self.canvas.shrink(40.0).contains(at) {
            cam.origin += self.canvas.center() - at;
            self.moved = true;
        }
    }

    /// 骨格を 1 つ進める・戻す。その部品を選んだ状態にして、出入りする線を出す
    fn flow_go(&mut self, d: i32) -> Option<String> {
        let ids: Vec<String> = model::bones(&self.view.as_ref()?.nodes).iter().map(|n| n.id.clone()).collect();
        if ids.is_empty() {
            return None;
        }
        let i = match self.flow {
            None => 0,
            Some(f) => (f as i32 + d).clamp(0, ids.len() as i32 - 1) as usize,
        };
        self.flow = Some(i);
        (self.selected, self.bundle) = (Some(ids[i].clone()), None);
        self.center_on(&ids[i], false); // 見えていなければ真ん中へ
        Some(ids[i].clone())
    }

    /// Space: 骨格を 1 番から順にたどる。本番でない部品に来たら、そこで止まる（次に通す所）。もう一度押すと止める
    fn flow_play(&mut self) {
        if self.playing.take().is_some() {
            return;
        }
        let n = self.view.as_ref().map_or(0, |v| model::bones(&v.nodes).len());
        if self.flow.is_none_or(|f| f + 1 >= n) {
            self.flow = None;
        }
        self.flow_step();
    }

    fn flow_step(&mut self) {
        let d = if self.flow.is_some() { 1 } else { 0 };
        let Some(id) = self.flow_go(d) else {
            self.playing = None;
            return;
        };
        let view = self.view.as_ref().unwrap();
        let n = view.nodes.iter().find(|n| n.id == id).unwrap();
        let last = self.flow.is_some_and(|f| f + 1 >= model::bones(&view.nodes).len());
        self.playing = (n.status == "live" && n.blocked.is_empty() && !last).then(|| Instant::now() + std::time::Duration::from_millis(1100));
    }

    fn flow_tick(&mut self, ctx: &egui::Context) {
        let Some(at) = self.playing else { return };
        if Instant::now() >= at {
            self.flow_step();
        } else {
            ctx.request_repaint_after(at - Instant::now());
        }
    }

    /// 右の欄（部品か、まとめた線を選んだときだけ）
    /// DB の部品を選んでいるとき、左半分に DB の図を出す。ドラッグで動かし、ホイールで拡大縮小、表を押すと下に
    /// 書く部品・読む部品（押すとその部品へ）。× か Esc で閉じる（部品を選ぶのをやめる）
    /// DB の図の幅（開いていなければ 0）。DB の部品を選んでいて、カンバンを出していないとき
    fn er_width(&self, body: egui::Rect) -> f32 {
        let open = !self.board_mode
            && self.selected.as_ref().is_some_and(|id| self.state.as_ref().and_then(|s| s.arch.as_ref()).is_some_and(|a| model::db_of(a, id).is_some()));
        if open { (body.width() * 0.5).max(420.0).min(body.width() - side::WIDTH - 40.0) } else { 0.0 }
    }

    fn er_panel(&mut self, ui: &mut egui::Ui, body: egui::Rect) {
        let db = match (&self.selected, self.state.as_ref().and_then(|s| s.arch.as_ref())) {
            (Some(id), Some(a)) => model::db_of(a, id).cloned(),
            _ => None,
        };
        let Some(db) = db else {
            self.er = None;
            return;
        };
        let w = self.er_width(body);
        let panel = egui::Rect::from_min_max(body.min, egui::pos2(body.min.x + w, body.max.y));
        let head = egui::Rect::from_min_size(panel.min, egui::vec2(panel.width(), 40.0));
        let area = egui::Rect::from_min_max(egui::pos2(panel.min.x, head.max.y), panel.max);
        let key = format!("{}|{}", db.node, serde_json::to_string(&db.tables).unwrap_or_default());
        if self.er.as_ref().is_none_or(|e| e.key != key) {
            let picked = self.er.take().and_then(|e| e.picked).filter(|p| db.tables.iter().any(|t| &t.name == p));
            let aspect = (area.width() / area.height().max(1.0)) as f64;
            self.er = Some(ErState { key, lay: er::er_layout(&db, aspect), cam: None, moved: false, picked, hover: None });
        }
        let p = ui.painter().with_clip_rect(panel);
        p.rect_filled(panel, 0.0, er::BACK);
        let edge = ui.painter();
        for (i, a) in [0.10_f32, 0.06, 0.03].iter().enumerate() {
            let x = panel.max.x + 1.0 + i as f32;
            edge.line_segment([egui::pos2(x, panel.min.y), egui::pos2(x, panel.max.y)], egui::Stroke::new(1.0_f32, egui::Color32::BLACK.gamma_multiply(*a)));
        }
        edge.line_segment([panel.right_top(), panel.right_bottom()], egui::Stroke::new(1.0_f32, draw::CARD_LINE));
        let resp = ui.interact(panel, egui::Id::new("er"), egui::Sense::click_and_drag());
        let e = self.er.as_mut().unwrap();
        let mut close = false;
        let mut go: Option<String> = None;
        // 見出し: 何の図か・数・閉じる
        p.rect_filled(head, 0.0, draw::PANEL);
        p.line_segment([head.left_bottom(), head.right_bottom()], egui::Stroke::new(1.0_f32, draw::LINE));
        let fks = er::foreign_keys(&db);
        let none = if fks.is_empty() { "（外部キーの宣言なし）" } else { "" };
        let title = format!("DB の図　表 {}・外部キー {}{none}", db.tables.len(), fks.len());
        let t = p.text(egui::pos2(head.min.x + 16.0, head.center().y), egui::Align2::LEFT_CENTER, title, draw::font(10.0, true), draw::TEXT);
        let mut hubs: Vec<String> = er::hubs(&db).into_iter().collect();
        hubs.sort();
        if !hubs.is_empty() {
            let n = fks.iter().filter(|k| hubs.contains(&k.2)).count();
            let note = format!("{} への外部キー（{n} 本）は線にせず、列に → で書く", hubs.join("・"));
            p.text(egui::pos2(t.max.x + 16.0, head.center().y), egui::Align2::LEFT_CENTER, note, draw::font(9.0, false), draw::FAINT);
        }
        let x = p.text(egui::pos2(head.max.x - 16.0, head.center().y), egui::Align2::RIGHT_CENTER, "×", draw::font(13.0, false), draw::FAINT);
        let x = x.expand(6.0);
        match &e.lay {
            Err(msg) => {
                p.text(area.center(), egui::Align2::CENTER_CENTER, format!("並べられない: {msg}"), draw::font(9.0, false), draw::FAINT);
            }
            Ok(lay) => {
                if !e.moved || e.cam.is_none() {
                    e.cam = Some(er::fit(area, lay));
                }
                let cam = e.cam.as_mut().unwrap();
                if resp.dragged() {
                    cam.origin += resp.drag_delta();
                    e.moved = true;
                }
                if let (true, Some(at)) = (resp.hovered(), resp.hover_pos()) {
                    let notches: f32 = ui.input(|i| {
                        i.events
                            .iter()
                            .filter_map(|ev| match ev {
                                egui::Event::MouseWheel { unit, delta, .. } => {
                                    Some(if *unit == egui::MouseWheelUnit::Line { delta.y } else { delta.y / 50.0 })
                                }
                                _ => None,
                            })
                            .sum()
                    });
                    if notches != 0.0 {
                        let ns = (cam.scale * (notches * 0.12).exp()).clamp(0.1, 3.0);
                        cam.origin = at - (at - cam.origin) * (ns / cam.scale);
                        cam.scale = ns;
                        e.moved = true;
                    }
                }
                let cam = draw::Camera { scale: cam.scale, origin: cam.origin };
                let at = if resp.dragged() { None } else { resp.hover_pos().filter(|q| area.contains(*q)) };
                e.hover = at.and_then(|q| er::table_at(&cam, lay, q));
                if e.hover.is_some() || resp.hover_pos().is_some_and(|q| x.contains(q)) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    let q = resp.interact_pointer_pos();
                    if q.is_some_and(|q| x.contains(q)) {
                        close = true;
                    } else {
                        let hit = q.filter(|q| area.contains(*q)).and_then(|q| er::table_at(&cam, lay, q));
                        e.picked = if hit == e.picked { None } else { hit };
                    }
                }
                let focus = e.hover.as_deref().or(e.picked.as_deref());
                er::paint(&p.with_clip_rect(area), &db, lay, &cam, focus);
            }
        }
        if matches!(e.lay, Err(_)) && resp.clicked() && resp.interact_pointer_pos().is_some_and(|q| x.contains(q)) {
            close = true;
        }
        // 押した表: 下に、書く部品・読む部品（押すとその部品へ）と口
        if let Some(t) = e.picked.as_ref().and_then(|n| db.tables.iter().find(|t| &t.name == n)) {
            let names: std::collections::HashMap<&str, &str> =
                self.view.as_ref().map(|v| v.nodes.iter().map(|n| (n.id.as_str(), n.title.as_str())).collect()).unwrap_or_default();
            let foot = egui::Rect::from_min_max(egui::pos2(panel.min.x, panel.max.y - 120.0), egui::pos2(panel.max.x - 1.0, panel.max.y));
            p.rect_filled(foot, 0.0, draw::PANEL);
            p.line_segment([foot.left_top(), foot.right_top()], egui::Stroke::new(1.0_f32, draw::LINE));
            ui.scope_builder(egui::UiBuilder::new().max_rect(foot.shrink2(egui::vec2(16.0, 10.0))), |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                egui::ScrollArea::vertical().id_salt("er-foot").auto_shrink([false, false]).show(ui, |ui| {
                    let head = format!("{}　列 {}", t.name, t.columns.len());
                    ui.label(egui::RichText::new(head).font(draw::font(10.0, true)).color(draw::TEXT));
                    for (title, ids) in [("書く", &t.write), ("読む", &t.read)] {
                        if ids.is_empty() {
                            continue;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.label(egui::RichText::new(title).font(draw::font(9.0, false)).color(draw::FAINT));
                            for id in ids {
                                let name = names.get(id.as_str()).copied().unwrap_or(id);
                                let l = ui.add(
                                    egui::Label::new(egui::RichText::new(name).font(draw::font(9.0, false)).color(draw::BONE)).sense(egui::Sense::click()),
                                );
                                if l.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    go = Some(id.clone());
                                }
                            }
                        });
                    }
                    if t.write.is_empty() && t.read.is_empty() {
                        ui.label(egui::RichText::new("触る部品が分からない").font(draw::font(9.0, false)).color(draw::FAINT));
                    }
                    if !t.via.is_empty() {
                        let via = format!("口: {}", t.via.join("・"));
                        ui.label(egui::RichText::new(via).font(draw::font(9.0, false)).color(draw::FAINT));
                    }
                });
            });
        }
        if close {
            (self.selected, self.er) = (None, None);
        }
        if let Some(id) = go {
            (self.selected, self.bundle) = (Some(id), None);
        }
    }

    /// 図の代わりにカンバンを出す。「図で見る」を押したら図に切り替えて、その部品を選ぶ
    fn kanban(&mut self, ui: &mut egui::Ui, body: egui::Rect) {
        let (Some(view), Some(st)) = (&self.view, &self.state) else { return };
        let mut act = None;
        let (board, scope) = (&mut self.board, &self.scope);
        ui.scope_builder(egui::UiBuilder::new().max_rect(body.shrink2(egui::vec2(20.0, 16.0))), |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            act = board.show(ui, st, view, scope);
        });
        if let Some(kanban::Action::Show(id)) = act {
            (self.board_mode, self.selected, self.bundle, self.list) = (false, Some(id), None, None);
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui, body: egui::Rect) {
        let rect = egui::Rect::from_min_max(egui::pos2(body.max.x - side::WIDTH, body.min.y), body.max);
        let p = ui.painter();
        p.rect_filled(rect, 0.0, draw::PANEL);
        p.line_segment([rect.left_top(), rect.left_bottom()], egui::Stroke::new(1.0_f32, draw::LINE));
        let (Some(view), Some(lay), Some(st)) = (&self.view, &self.lay, &self.state) else { return };
        let inner = rect.shrink(16.0);
        let salt = format!("{:?}{:?}{}", self.selected, self.bundle, self.list.map_or(0, |l| l as u8 + 1)); // 選び直したら、欄を上から
        let fresh = salt != self.side_shown;
        self.side_shown = salt.clone();
        let mut act = None;
        let (selected, bundle, side, working, list) = (&self.selected, self.bundle, &mut self.side, &self.working, self.list);
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            let mut area = egui::ScrollArea::vertical().id_salt(&salt).auto_shrink([false, false]);
            if fresh {
                area = area.vertical_scroll_offset(0.0); // 長い一覧が、開いた時点で少し下までずれていた（題が隠れた）
            }
            area.show(ui, |ui| {
                ui.set_width(inner.width() - 12.0);
                act = match (selected, bundle, list) {
                    (Some(id), _, _) => match view.nodes.iter().find(|n| &n.id == id) {
                        Some(n) if n.future => side.future_node(ui, st, view, n),
                        Some(n) => side.node(ui, st, view, n, working.get(&n.id)),
                        None => None,
                    },
                    (None, Some(b), _) if b < lay.bundles.len() => side.bundle(ui, view, lay, b),
                    (None, None, Some(draw::HeadHit::Drift)) => side::list(ui, "図と現物のずれ", &side::drift_lines(st, view)),
                    (None, None, Some(draw::HeadHit::Changes)) => side::list(ui, "前回からの変化", &side::change_lines(st)),
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
        let aspect = self.aspect();
        let keep = self.lay.as_ref().and_then(|l| l.keep.clone());
        std::thread::spawn(move || {
            let lay = layout::layout_keep(&view, aspect, MAX_ROWS, keep.as_ref());
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
                self.state = Some(st);
                self.sync_tabs();
                self.rebuild();
                if self.msg.starts_with("読めない") {
                    self.msg.clear();
                }
            }
            Err(e) => self.msg = format!("読めない: {e}"), // 描けなくても閉じない。次の回にやり直す
        }
    }

    /// 今のタブの中身を、最新の state から作り直す。並べ方が変わっていなければ描き直すだけ
    fn rebuild(&mut self) {
        let Some(a) = self.state.as_ref().and_then(|s| s.arch.as_ref()) else { return };
        let view = model::view_mode(a, &self.scope, self.picked.as_deref(), self.full);
        let same = self.pending.is_none() && self.lay.is_some() && self.view.as_ref().is_some_and(|v| v.full == view.full && shape(v) == shape(&view));
        let stashed = self.stash.as_ref().is_some_and(|(v, _)| v.full == view.full && shape(v) == shape(&view));
        if same {
            self.view = Some(view);
            self.forget_missing();
        } else if stashed {
            // 今と完成図を切り替えた: もう一方の並べ方を覚えていたので、並べ直さずに入れ替える
            let (_, lay) = self.stash.take().unwrap();
            if let (Some(v), Some(l)) = (self.view.take(), self.lay.take()) {
                self.stash = Some((v, l));
            }
            (self.view, self.lay, self.pending, self.moved) = (Some(view), Some(lay), None, false);
            self.forget_missing();
        } else if self.zoomed {
            self.start_layout(view);
        } else {
            self.view = Some(view); // 窓の大きさが決まったら並べる
        }
    }

    /// 今のタブの見ていた場所を覚える
    fn save_tab(&mut self) {
        self.tabs[self.cur] = Tab {
            scope: self.scope.clone(),
            cam: self.cam,
            moved: self.moved,
            selected: self.selected.take(),
            bundle: self.bundle.take(),
            flow: self.flow.take(),
            picked: self.picked.take(),
            view: self.view.take(),
            lay: self.lay.take(),
            stash: self.stash.take(),
        };
    }

    /// タブ i の見ていた場所を取り出して描く（今のタブを覚えるのは呼ぶ側）
    fn load(&mut self, i: usize) {
        self.cur = i;
        let t = std::mem::take(&mut self.tabs[i]);
        self.tabs[i].scope = t.scope.clone();
        (self.scope, self.cam, self.moved, self.selected, self.bundle) = (t.scope, t.cam, t.moved, t.selected, t.bundle);
        (self.flow, self.picked, self.view, self.lay, self.stash) = (t.flow, t.picked, t.view, t.lay, t.stash);
        (self.hover_node, self.hover_edge, self.playing, self.pending) = (None, None, None, None);
        self.rebuild(); // 中身は最新の state で
    }

    /// タブを切り替える。見ていた場所は覚えておき、戻ったら同じ場所から
    fn switch(&mut self, i: usize) {
        if i == self.cur || i >= self.tabs.len() {
            return;
        }
        self.save_tab();
        self.load(i);
    }

    /// その範囲のタブへ切り替える（prefix+o）。タブは全部のプロジェクトぶん最初からあるので、
    /// 足すのは構成図にまだ部品が無いプロジェクトのときだけ
    fn open_tab(&mut self, scope: String) {
        if let Some(i) = self.tabs.iter().enumerate().position(|(i, t)| (if i == self.cur { &self.scope } else { &t.scope }) == &scope) {
            return self.switch(i);
        }
        self.save_tab();
        self.tabs.push(Tab { scope, ..Default::default() });
        self.load(self.tabs.len() - 1);
    }

    /// タブを、全体 ＋ 構成図にあるプロジェクト全部にそろえる（開いたときと、state を読み直したとき）。
    /// 並びは tab_scopes の順。今あるタブは見ていた場所ごと残し、プロジェクトが増えたら足す。
    /// state に無い範囲のタブ（prefix+o で開いたもの）は後ろに残す。今のタブは替えない
    fn sync_tabs(&mut self) {
        let Some(st) = &self.state else { return };
        let want = model::tab_scopes(st);
        let have: Vec<String> = self.tabs.iter().enumerate().map(|(i, t)| if i == self.cur { self.scope.clone() } else { t.scope.clone() }).collect();
        let mut order: Vec<String> = want.clone();
        order.extend(have.iter().filter(|h| !want.contains(h)).cloned());
        if order == have {
            return;
        }
        let mut old: Vec<Option<Tab>> = std::mem::take(&mut self.tabs).into_iter().map(Some).collect();
        self.tabs = order
            .iter()
            .map(|sc| match have.iter().position(|h| h == sc) {
                Some(k) => old[k].take().unwrap(),
                None => Tab { scope: sc.clone(), ..Default::default() },
            })
            .collect();
        self.cur = order.iter().position(|sc| sc == &self.scope).unwrap();
    }

    /// タブの帯を描き、押されたら切り替える・閉じる
    fn tabbar(&mut self, ui: &mut egui::Ui, p: &egui::Painter, r: egui::Rect) {
        let names: Vec<String> = self.tabs.iter().enumerate().map(|(i, t)| if i == self.cur { self.scope.clone() } else { t.scope.clone() }).collect();
        let colors: Vec<Option<String>> =
            names.iter().map(|n| self.state.as_ref().and_then(|st| st.projects.get(n)).and_then(|p| p.color.clone())).collect();
        let hits = draw::tabbar(p, r, &names, &colors, self.cur);
        let resp = ui.interact(r, egui::Id::new("tabbar"), egui::Sense::click());
        let over = resp.hover_pos().and_then(|at| hits.iter().rev().find(|h| h.0.contains(at)).map(|h| h.1));
        if over.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            match over {
                Some(i) => self.switch(i),
                None => {}
            }
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
            owners: vec![],
            full: false,
        };
        let lay = Layout {
            pos: [("a".to_string(), (100.0, 100.0)), ("b".to_string(), (500.0, 100.0))].into_iter().collect(),
            frames: vec![],
            paths: vec![layout::Path { points: vec![(190.0, 100.0), (410.0, 100.0)], label_at: None }],
            size: (600.0, 200.0),
            bundles: vec![],
            keep: None,
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

    /// 骨格が 3 つ（1・2 は本番、3 は作りかけ）の state を読ませた Viewer
    fn flow_viewer() -> Viewer {
        let dir = std::env::temp_dir().join("andon-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let node = |id: &str, step: u32, status: &str| {
            serde_json::json!({ "id": id, "zone": "z", "title": id, "status": status, "blocked": "", "steps": { "p": step } })
        };
        let state = serde_json::json!({
            "generated_at": "2026-10-06T00:00:00",
            "arch": { "zones": [{ "id": "z" }], "nodes": [node("c", 3, "wip"), node("a", 1, "live"), node("b", 2, "live")], "edges": [] }
        });
        std::fs::write(&path, state.to_string()).unwrap();
        let _one = ENV.lock().unwrap();
        unsafe { std::env::set_var("ARCH_STATE", &path) };
        Viewer::new(egui::Context::default(), String::new(), None)
    }

    #[test]
    fn 矢印で骨格を番号順にたどり_端で止まる() {
        let mut v = flow_viewer();
        assert_eq!(v.flow_go(1).as_deref(), Some("a")); // 初めは 1 番から
        assert_eq!(v.flow_go(1).as_deref(), Some("b"));
        assert_eq!(v.flow_go(1).as_deref(), Some("c"));
        assert_eq!(v.flow_go(1).as_deref(), Some("c")); // 最後より先へは行かない
        assert_eq!(v.flow_go(-1).as_deref(), Some("b"));
        assert_eq!(v.selected.as_deref(), Some("b"));
    }

    #[test]
    fn 再生は本番でない部品で止まる() {
        let mut v = flow_viewer();
        let ctx = egui::Context::default();
        v.flow_play();
        assert_eq!(v.selected.as_deref(), Some("a"));
        for _ in 0..5 {
            if v.playing.is_some() {
                v.playing = Some(Instant::now()); // 1.1 秒待たずに進める
                v.flow_tick(&ctx);
            }
        }
        assert_eq!(v.selected.as_deref(), Some("c")); // 作りかけの 3 番で止まる
        assert!(v.playing.is_none());
    }

    #[test]
    fn 完成図に切り替えても骨格は今の部品だけをたどり_戻すと構想は消える() {
        let mut v = flow_viewer();
        let node = |id: &str, step: u32, status: &str| {
            serde_json::json!({ "id": id, "zone": "z", "title": id, "status": status, "blocked": "", "steps": { "p": step } })
        };
        let state = serde_json::json!({
            "arch": { "zones": [{ "id": "z" }], "nodes": [node("a", 1, "live"), node("b", 2, "wip")], "edges": [],
                      "future": { "nodes": [{ "id": "later", "zone": "z", "title": "あとで", "future": true }] } }
        });
        v.state = Some(serde_json::from_value(state).unwrap());
        v.rebuild();
        assert_eq!(v.view.as_ref().unwrap().nodes.len(), 2);
        v.set_full(true);
        let view = v.view.as_ref().unwrap();
        assert!(view.full && view.nodes.iter().any(|n| n.id == "later"));
        assert_eq!(v.flow_go(1).as_deref(), Some("a"));
        assert_eq!(v.flow_go(1).as_deref(), Some("b"));
        assert_eq!(v.flow_go(1).as_deref(), Some("b")); // 構想へは進まない
        v.selected = Some("later".into());
        v.set_full(false);
        assert!(!v.view.as_ref().unwrap().nodes.iter().any(|n| n.id == "later"));
    }

    /// ARCH_STATE を書き換えて読むまでを 1 本ずつ（テストは並べて走るので、読む前にほかのテストに書き換えられないように）
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// プロジェクト 3 つ（b は order 1・a は order 2・c は order なし）の state を読む窓。開いたときの範囲は c
    fn tabs_viewer(dir: &str) -> Viewer {
        let dir = std::env::temp_dir().join(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let node = |id: &str, project: &str| serde_json::json!({ "id": id, "zone": "z", "project": project, "title": id, "status": "live" });
        let state = serde_json::json!({
            "generated_at": "2026-10-23T00:00:00",
            "projects": { "a": { "github": "", "order": 2 }, "b": { "github": "", "order": 1, "color": "green3" }, "c": { "github": "" } },
            "arch": { "zones": [{ "id": "z" }], "nodes": [node("x", "a"), node("y", "b"), node("w", "c"), node("s", "shared")], "edges": [] }
        });
        std::fs::write(&path, state.to_string()).unwrap();
        let _one = ENV.lock().unwrap();
        unsafe { std::env::set_var("ARCH_STATE", &path) };
        Viewer::new(egui::Context::default(), "c".into(), None)
    }

    fn scopes(v: &Viewer) -> Vec<String> {
        v.tabs.iter().enumerate().map(|(i, t)| if i == v.cur { v.scope.clone() } else { t.scope.clone() }).collect()
    }

    #[test]
    fn タブは最初から全体と全部のプロジェクトで_開いた範囲を選ぶ() {
        let mut v = tabs_viewer("andon-test-tabs");
        assert_eq!(scopes(&v), ["", "b", "a", "c"]);
        assert_eq!(v.scope, "c");
        // prefix+o はタブを増やさず切り替えるだけ
        v.open_tab("b".into());
        assert_eq!((scopes(&v).len(), v.scope.as_str()), (4, "b"));
        // state に無い範囲は後ろに足し、読み直しても消さない
        v.open_tab("later".into());
        v.sync_tabs();
        assert_eq!(scopes(&v), ["", "b", "a", "c", "later"]);
        assert_eq!(v.scope, "later");
    }
}
