//! DB の図（ER 図）。上の帯の「DB」か d、または図の DB の部品を押すと、図の代わりに出す（カンバンと同じ切り替え）。
//! 表ごとに箱を描いて列を並べ、表どうしの関係を線でつなぐ。表は、それを主に書く部品ごとの枠に入れる。
//! 表・列・関係・名前・説明・どの部品が書く・読むかは、カンバンの arch.py の db が決める（state の arch.db）。ここは並べて描くだけ。
//! 並べ方は Graphviz（dot）。参照される表を手前に置く（上下に流すか横に流すかは、窓の形に近い方）。

use crate::draw::{self, BONE, Camera, DIM, FAINT, LINE, PANEL, TEXT, font, measure};
use crate::layout::{self, Pt};
use crate::model::{Column, Db, Table};
use egui::text::LayoutJob;
use egui::{Align2, Color32, CursorIcon, FontFamily, FontId, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, TextFormat, Ui, pos2, vec2};
use std::collections::{BTreeMap, HashMap, HashSet};

/// 表の箱の幅・見出しの高さ・1 行の高さ（点）
pub const BOX_W: f64 = 250.0;
pub const HEAD: f64 = 30.0;
pub const ROW: f64 = 18.0;
const BOX_PAD: f64 = 6.0;
/// 線の色。選んだ表の線は紺
const QUIET: Color32 = Color32::from_rgb(0x9a, 0xa3, 0xad);
const LOUD: Color32 = BONE;
const HEAD_FILL: Color32 = Color32::from_rgb(0xf3, 0xf4, 0xf6);
const FRAME_FILL: Color32 = Color32::from_rgb(0xe9, 0xec, 0xf0);
/// 列が読める倍率（行の高さが文字の下限より高い）。全体を収めるとこれより小さくなるときは、この倍率で左上から出す
pub const READABLE: f32 = 0.8;
/// 図の背景（構成図の紙と分ける）
pub const BACK: Color32 = Color32::from_rgb(0xf6, 0xf7, 0xf9);
/// 畳む列（どの表にもある、読まなくても表の意味が分かる列）
const FOLD: [&str; 2] = ["created_at", "updated_at"];

/// 表どうしの関係。child の column が parent を指す。implied は宣言の無い関係（同じ値で引いている）。
/// one は 1 対 1（指す列が子の表の 1 列だけの主キー）
#[derive(Clone, Debug, PartialEq)]
pub struct Rel {
    pub child: String,
    pub column: String,
    pub parent: String,
    pub implied: bool,
    pub one: bool,
}

#[derive(Clone, Debug)]
pub struct ErBox {
    pub name: String,
    /// 左上と大きさ（点）
    pub at: Pt,
    pub size: Pt,
}

/// 線。points は親から子へ
#[derive(Clone, Debug)]
pub struct ErEdge {
    pub rel: Rel,
    pub points: Vec<Pt>,
}

/// まとめる枠（部品の id と、左上・右下）
#[derive(Clone, Debug)]
pub struct ErFrame {
    pub part: String,
    pub bx: [f64; 4],
}

#[derive(Clone, Debug, Default)]
pub struct ErLayout {
    pub boxes: Vec<ErBox>,
    pub edges: Vec<ErEdge>,
    pub frames: Vec<ErFrame>,
    pub size: Pt,
}

/// 表の呼び名（日本語の名前。無ければ英語の名前）
pub fn label(t: &Table) -> &str {
    if t.label.is_empty() { &t.name } else { &t.label }
}

/// 表どうしの関係（宣言された外部キーと、宣言の無い関係）。親が図に無い・自分を指すものは除く
pub fn rels(db: &Db) -> Vec<Rel> {
    let names: HashSet<&str> = db.tables.iter().map(|t| t.name.as_str()).collect();
    let mut out = vec![];
    for t in &db.tables {
        let pks: Vec<&str> = t.columns.iter().filter(|c| c.pk).map(|c| c.name.as_str()).collect();
        for c in &t.columns {
            let (parent, implied) = if !c.fk.is_empty() { (&c.fk, false) } else { (&c.r#ref, true) };
            if !parent.is_empty() && parent != &t.name && names.contains(parent.as_str()) {
                out.push(Rel { child: t.name.clone(), column: c.name.clone(), parent: parent.clone(), implied, one: pks == [c.name.as_str()] });
            }
        }
    }
    out
}

/// 宣言された外部キーの数
pub fn foreign_keys(db: &Db) -> usize {
    rels(db).iter().filter(|r| !r.implied).count()
}

/// 多くの表が指す表（指す表が 6 つ以上かつ全体の 4 割以上）と、指す表の数。ここへの関係は線にせず、列に「→ 表」と書く。
/// どの表もその表を指していると、線が 1 か所に集まって図が横に広がり、ほかの線が読めなくなった（2026-10-23）
pub fn hubs(db: &Db) -> BTreeMap<String, usize> {
    let mut by: HashMap<&str, HashSet<&str>> = HashMap::new();
    let all = rels(db);
    for r in &all {
        by.entry(r.parent.as_str()).or_default().insert(r.child.as_str());
    }
    let need = 6usize.max((db.tables.len() as f64 * 0.4).ceil() as usize);
    by.into_iter().filter(|(_, c)| c.len() >= need).map(|(p, c)| (p.to_string(), c.len())).collect()
}

/// 線にする関係（多くの表が指す表への関係は除く）
pub fn drawn(db: &Db) -> Vec<Rel> {
    let h = hubs(db);
    rels(db).into_iter().filter(|r| !h.contains_key(&r.parent)).collect()
}

fn folded(c: &Column) -> bool {
    FOLD.contains(&c.name.as_str()) && !c.pk && c.fk.is_empty() && c.r#ref.is_empty()
}

/// 箱の中の行
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// 多くの表から指される（指す表の数）
    Hub(usize),
    Col(usize),
    /// 畳んだ列（数）。押すと開く
    More(usize),
}

pub fn rows(t: &Table, hubs: &BTreeMap<String, usize>, open: bool) -> Vec<Row> {
    let mut out = vec![];
    if let Some(n) = hubs.get(&t.name) {
        out.push(Row::Hub(*n));
    }
    let hidden = t.columns.iter().filter(|c| folded(c)).count();
    for (i, c) in t.columns.iter().enumerate() {
        if open || !folded(c) {
            out.push(Row::Col(i));
        }
    }
    if hidden > 0 && !open {
        out.push(Row::More(hidden));
    }
    out
}

fn box_h(n_rows: usize) -> f64 {
    HEAD + ROW * n_rows.max(1) as f64 + BOX_PAD
}

/// 表のひと固まり（線でつながる・同じ部品の枠に入る表）を並べる。h と rels は図全体のもの（固まりの中だけで数え直さない）
fn one(db: &Db, h: &BTreeMap<String, usize>, rels: &[Rel], opened: &HashSet<String>, rankdir: &str) -> Result<ErLayout, String> {
    let names: HashMap<&str, String> = db.tables.iter().enumerate().map(|(i, t)| (t.name.as_str(), format!("t{i}"))).collect();
    let mut lines = vec![
        "digraph G {".to_string(),
        format!("rankdir={rankdir}; nodesep=0.4; ranksep=0.7; splines=spline;"),
        r#"node [shape=box, fixedsize=true, label=""];"#.to_string(),
    ];
    // 主に書く部品ごとの枠（名前を書く場所の分だけ上をあける）。部品の無い表は枠に入れない
    let mut parts: Vec<&str> = db.tables.iter().map(|t| t.part.as_str()).filter(|p| !p.is_empty()).collect();
    parts.sort();
    parts.dedup();
    let node = |t: &Table| {
        let hh = box_h(rows(t, &h, opened.contains(&t.name)).len());
        format!("{} [width={:.3}, height={:.3}];", names[t.name.as_str()], BOX_W / 72.0, hh / 72.0)
    };
    for (gi, p) in parts.iter().enumerate() {
        let body: Vec<String> = db.tables.iter().filter(|t| t.part == *p).map(node).collect();
        lines.push(format!(r#"subgraph cluster_g{gi} {{ label=" "; fontsize=22; margin=14; {} }}"#, body.join(" ")));
    }
    for t in db.tables.iter().filter(|t| t.part.is_empty()) {
        lines.push(node(t));
    }
    let rels: Vec<&Rel> = rels.iter().filter(|r| names.contains_key(r.parent.as_str()) && names.contains_key(r.child.as_str())).collect();
    for r in &rels {
        lines.push(format!("{} -> {};", names[r.parent.as_str()], names[r.child.as_str()]));
    }
    lines.push("}".to_string());
    let g = layout::run_dot(&lines.join("\n"))?;
    let bb = layout::floats(&g["bb"]);
    let (x0, y1) = (bb[0], bb[3]);
    let flip = move |x: f64, y: f64| (x - x0, y1 - y);
    let by_name: HashMap<&str, &Table> = db.tables.iter().map(|t| (names[t.name.as_str()].as_str(), t)).collect();
    let (mut boxes, mut frames) = (vec![], vec![]);
    for o in g["objects"].as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
        let oname = o["name"].as_str().unwrap_or("");
        if let Some(gi) = oname.strip_prefix("cluster_g").and_then(|s| s.parse::<usize>().ok()) {
            let b = layout::floats(&o["bb"]);
            if b.len() == 4 {
                frames.push(ErFrame { part: parts[gi].to_string(), bx: [b[0] - x0, y1 - b[3], b[2] - x0, y1 - b[1]] });
            }
        } else if let Some(t) = by_name.get(oname) {
            let p = layout::floats(&o["pos"]);
            let (cx, cy) = flip(p[0], p[1]);
            let hh = box_h(rows(t, &h, opened.contains(&t.name)).len());
            boxes.push(ErBox { name: t.name.clone(), at: (cx - BOX_W / 2.0, cy - hh / 2.0), size: (BOX_W, hh) });
        }
    }
    boxes.sort_by_key(|b| db.tables.iter().position(|t| t.name == b.name));
    frames.sort_by(|a, b| a.part.cmp(&b.part));
    let edges = g["edges"]
        .as_array()
        .map(|v| v.as_slice())
        .unwrap_or(&[])
        .iter()
        .take(rels.len())
        .zip(&rels)
        .map(|(e, r)| ErEdge { rel: (*r).clone(), points: layout::spline(e["pos"].as_str().unwrap_or(""), &flip) })
        .collect();
    Ok(ErLayout { boxes, edges, frames, size: (bb[2] - bb[0], bb[3] - bb[1]) })
}

/// 上下に流す向きと横に流す向きを並べ、窓の形（aspect = 幅 / 高さ）に近い方を選ぶ。opened は畳んだ列を開いた表
pub fn er_layout(db: &Db, opened: &HashSet<String>, aspect: f64) -> Result<ErLayout, String> {
    if db.tables.is_empty() {
        return Ok(ErLayout::default());
    }
    let h = hubs(db);
    let rels = drawn(db);
    // 固まりに分ける（線でつながる表と、同じ部品の枠に入る表）。1 枚の図で並べると、つながらない枠が横一列に並んで
    // 4:1 の横長になった（2026-10-23）。固まりごとに並べ、窓の形に近づくように段に詰める
    let idx: HashMap<&str, usize> = db.tables.iter().enumerate().map(|(i, t)| (t.name.as_str(), i)).collect();
    let mut up: Vec<usize> = (0..db.tables.len()).collect();
    fn root(up: &mut Vec<usize>, i: usize) -> usize {
        let mut r = i;
        while up[r] != r {
            r = up[r];
        }
        up[i] = r;
        r
    }
    let join = |a: usize, b: usize, up: &mut Vec<usize>| {
        let (x, y) = (root(up, a), root(up, b));
        up[x] = y;
    };
    for r in &rels {
        join(idx[r.parent.as_str()], idx[r.child.as_str()], &mut up);
    }
    let mut by_part: HashMap<&str, usize> = HashMap::new();
    for (i, t) in db.tables.iter().enumerate() {
        if t.part.is_empty() {
            continue;
        }
        match by_part.get(t.part.as_str()) {
            Some(&j) => join(i, j, &mut up),
            None => {
                by_part.insert(&t.part, i);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<Table>> = BTreeMap::new();
    for i in 0..db.tables.len() {
        let r = root(&mut up, i);
        groups.entry(r).or_default().push(db.tables[i].clone());
    }
    // 固まりごとに、上下と横のうち正方形に近い方で並べる。dot は 1 回の起動に 1 秒ほどかかるので、全部を同時に走らせる
    let subs: Vec<Db> = groups.into_values().map(|tables| Db { tables, ..Default::default() }).collect();
    let pieces: Vec<ErLayout> = std::thread::scope(|s| {
        let jobs: Vec<_> = subs
            .iter()
            .map(|sub| {
                let (h, rels) = (&h, &rels);
                let tb = s.spawn(move || one(sub, h, rels, opened, "TB"));
                let lr = (sub.tables.len() > 2).then(|| s.spawn(move || one(sub, h, rels, opened, "LR")));
                (tb, lr)
            })
            .collect();
        jobs.into_iter()
            .map(|(tb, lr)| {
                let tb = tb.join().map_err(|_| "並べる糸が落ちた".to_string())??;
                let Some(lr) = lr else { return Ok(tb) };
                let lr = lr.join().map_err(|_| "並べる糸が落ちた".to_string())??;
                let sq = |l: &ErLayout| (l.size.0 / l.size.1.max(1.0)).ln().abs();
                Ok(if sq(&lr) < sq(&tb) { lr } else { tb })
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    Ok(pack(pieces, aspect))
}

/// 固まりを、背の高い順に左から段に詰める。段の幅は、全体が窓の形（aspect）に近くなるように決める
fn pack(mut pieces: Vec<ErLayout>, aspect: f64) -> ErLayout {
    const GAP: f64 = 40.0;
    pieces.sort_by(|a, b| b.size.1.total_cmp(&a.size.1).then(b.size.0.total_cmp(&a.size.0)));
    let area: f64 = pieces.iter().map(|p| (p.size.0 + GAP) * (p.size.1 + GAP)).sum();
    let widest = pieces.iter().map(|p| p.size.0).fold(0.0, f64::max);
    let room = (area * aspect).sqrt().max(widest);
    let mut out = ErLayout::default();
    let (mut x, mut y, mut row_h, mut width) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for p in pieces {
        if x > 0.0 && x + p.size.0 > room {
            (x, y, row_h) = (0.0, y + row_h + GAP, 0.0);
        }
        let mv = |q: Pt| (q.0 + x, q.1 + y);
        out.boxes.extend(p.boxes.into_iter().map(|b| ErBox { at: mv(b.at), ..b }));
        out.frames.extend(p.frames.into_iter().map(|f| ErFrame { bx: [f.bx[0] + x, f.bx[1] + y, f.bx[2] + x, f.bx[3] + y], ..f }));
        out.edges.extend(p.edges.into_iter().map(|e| ErEdge { points: e.points.into_iter().map(mv).collect(), ..e }));
        width = width.max(x + p.size.0);
        row_h = row_h.max(p.size.1);
        x += p.size.0 + GAP;
    }
    out.size = (width, y + row_h);
    out.frames.sort_by(|a, b| a.part.cmp(&b.part));
    out
}

/// 全体を収める。列が読めない倍率まで縮めるなら、読める倍率で左上から（残りはドラッグで動かして見る）
pub fn fit(area: Rect, lay: &ErLayout) -> Camera {
    let (sw, sh) = (lay.size.0 as f32 + 20.0, lay.size.1 as f32 + 20.0);
    let scale = ((area.width() - 24.0) / sw).min((area.height() - 24.0) / sh).min(1.2);
    if scale < READABLE {
        return Camera { scale: READABLE, origin: area.min + vec2(16.0, 16.0) };
    }
    let origin = area.min + vec2((area.width() - sw * scale) / 2.0 + 10.0 * scale, (area.height() - sh * scale) / 2.0 + 10.0 * scale);
    Camera { scale, origin }
}

fn rect_of(cam: &Camera, b: &ErBox) -> Rect {
    Rect::from_min_size(cam.xy(b.at), vec2(b.size.0 as f32 * cam.scale, b.size.1 as f32 * cam.scale))
}

/// 図の上で押せるもの
#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Table(String),
    /// 畳んだ列を開く・閉じる（その表）
    Fold(String),
    /// 線（edges の番号）
    Edge(usize),
}

/// その場所にあるもの（表の畳んだ行 → 表 → 線の順）
pub fn hit(db: &Db, lay: &ErLayout, cam: &Camera, opened: &HashSet<String>, at: Pos2) -> Option<Hit> {
    let h = hubs(db);
    for b in lay.boxes.iter().rev() {
        let r = rect_of(cam, b);
        if !r.contains(at) {
            continue;
        }
        let Some(t) = db.tables.iter().find(|t| t.name == b.name) else { continue };
        let rs = rows(t, &h, opened.contains(&t.name));
        let i = ((at.y - r.min.y - HEAD as f32 * cam.scale) / (ROW as f32 * cam.scale)).floor();
        if i >= 0.0 && matches!(rs.get(i as usize), Some(Row::More(_))) {
            return Some(Hit::Fold(t.name.clone()));
        }
        if opened.contains(&t.name) && i >= 0.0 && i as usize >= rs.len() {
            return Some(Hit::Fold(t.name.clone())); // 開いた表は、いちばん下の余白を押すと畳む
        }
        return Some(Hit::Table(t.name.clone()));
    }
    lay.edges
        .iter()
        .enumerate()
        .find(|(_, e)| draw::distance_to(&e.points.iter().map(|q| cam.xy(*q)).collect::<Vec<_>>(), at) <= 5.0)
        .map(|(i, _)| Hit::Edge(i))
}

/// 線の意味を文にする（「1 件の問い合わせに、日程が何件も」）
pub fn sentence(db: &Db, r: &Rel) -> String {
    let name = |n: &str| db.tables.iter().find(|t| t.name == n).map_or(n.to_string(), |t| label(t).to_string());
    let how = if r.one { "1 件" } else { "何件も" };
    let why = if r.implied { "（宣言は無く、同じ値で引いている）" } else { "" };
    format!("1 件の{}に、{}が{how}{why}\n{}.{} → {}", name(&r.parent), name(&r.child), r.child, r.column, r.parent)
}

/// 子の側の「多」の印（カラスの足）。end が線の端（子の表の辺）、dir は線の向き（親 → 子）
fn crow(p: &Painter, end: Pos2, dir: egui::Vec2, s: f32, color: Color32) {
    let d = dir.normalized();
    let n = vec2(-d.y, d.x);
    let back = end - d * 10.0 * s;
    for k in [-1.0, 0.0, 1.0] {
        p.line_segment([back, end + n * k * 6.0 * s], Stroke::new(1.2_f32, color));
    }
}

/// 「1」の短い横棒。at から dir の向きに少し入った所
fn bar(p: &Painter, at: Pos2, dir: egui::Vec2, s: f32, color: Color32) {
    let d = dir.normalized();
    let m = at + d * 7.0 * s;
    let n = vec2(-d.y, d.x) * 5.0 * s;
    p.line_segment([m - n, m + n], Stroke::new(1.2_f32, color));
}

fn dashed_line(p: &Painter, pts: &[Pos2], width: f32, color: Color32) {
    let (on, off) = (5.0_f32, 4.0_f32);
    let (mut left, mut drawing) = (on, true);
    let mut cur = vec![pts[0]];
    for w in pts.windows(2) {
        let (mut a, b) = (w[0], w[1]);
        let mut len = (b - a).length();
        while len > 0.0 {
            let step = left.min(len);
            let q = a + (b - a).normalized() * step;
            if drawing {
                cur.push(q);
            }
            len -= step;
            left -= step;
            a = q;
            if left <= 0.0 {
                if drawing && cur.len() > 1 {
                    p.add(Shape::line(std::mem::take(&mut cur), Stroke::new(width, color)));
                }
                drawing = !drawing;
                left = if drawing { on } else { off };
                cur = vec![q];
            }
        }
    }
    if drawing && cur.len() > 1 {
        p.add(Shape::line(cur, Stroke::new(width, color)));
    }
}

/// 描くときに見るもの
pub struct Look<'a> {
    pub cam: &'a Camera,
    /// 選んでいる表か、乗せている表（その表の線を濃く、つながらない表を薄く）
    pub focus: Option<&'a str>,
    /// 乗せている線（文を出す）
    pub hover_edge: Option<usize>,
    pub pointer: Option<Pos2>,
    pub opened: &'a HashSet<String>,
    /// 部品の id → 名前（枠の名前）
    pub parts: &'a HashMap<String, String>,
}

pub fn paint(p: &Painter, db: &Db, lay: &ErLayout, look: &Look) {
    let cam = look.cam;
    let s = cam.scale;
    let focus = look.focus;
    let h = hubs(db);
    let near: HashSet<&str> = match focus {
        Some(f) => lay
            .edges
            .iter()
            .filter(|e| e.rel.child == f || e.rel.parent == f)
            .flat_map(|e| [e.rel.child.as_str(), e.rel.parent.as_str()])
            .chain([f])
            .collect(),
        None => HashSet::new(),
    };
    // 枠（主に書く部品）
    for fr in &lay.frames {
        let r = Rect::from_min_max(cam.xy((fr.bx[0], fr.bx[1])), cam.xy((fr.bx[2], fr.bx[3])));
        p.rect(r, 8.0 * s, FRAME_FILL, Stroke::new(1.0_f32, LINE), StrokeKind::Inside);
        let name = look.parts.get(&fr.part).map_or(fr.part.as_str(), |x| x.as_str());
        let f = font(9.0 * s, true);
        p.text(r.min + vec2(12.0 * s, 10.0 * s), Align2::LEFT_TOP, draw::clip(p, name, &f, r.width() - 24.0 * s), f, DIM);
    }
    // 線（薄いものを先に、濃いものを上に）
    for loud in [false, true] {
        for (i, e) in lay.edges.iter().enumerate() {
            let on = focus.is_some_and(|f| e.rel.child == f || e.rel.parent == f) || look.hover_edge == Some(i);
            if on != loud || e.points.len() < 2 {
                continue;
            }
            let pts: Vec<Pos2> = e.points.iter().map(|q| cam.xy(*q)).collect();
            let (color, width) = if on { (LOUD, 1.6_f32) } else { (QUIET, 1.1) };
            if e.rel.implied {
                dashed_line(p, &pts, width, color);
            } else {
                p.add(Shape::line(pts.clone(), Stroke::new(width, color)));
            }
            let n = pts.len();
            if e.rel.one {
                bar(p, pts[n - 1], pts[n - 2] - pts[n - 1], s.max(0.6), color);
            } else {
                crow(p, pts[n - 1], pts[n - 1] - pts[n - 2], s.max(0.6), color);
            }
            bar(p, pts[0], pts[1] - pts[0], s.max(0.6), color);
        }
    }
    let tables: HashMap<&str, &Table> = db.tables.iter().map(|t| (t.name.as_str(), t)).collect();
    let row_px = ROW as f32 * s;
    let rows_readable = row_px >= draw::MIN_FONT + 2.0;
    for b in &lay.boxes {
        let Some(t) = tables.get(b.name.as_str()) else { continue };
        let r = rect_of(cam, b);
        let dim = focus.is_some() && !near.contains(b.name.as_str());
        let on = focus == Some(b.name.as_str());
        let a = |c: Color32| if dim { c.gamma_multiply(0.35) } else { c };
        p.rect(r, 4.0 * s, PANEL, Stroke::new(if on { 2.0_f32 } else { 1.0 }, if on { BONE } else { a(LINE) }), StrokeKind::Inside);
        let head = Rect::from_min_size(r.min, vec2(r.width(), HEAD as f32 * s));
        p.rect_filled(head.shrink(1.0), 3.0 * s, a(HEAD_FILL));
        p.line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0_f32, a(LINE)));
        let pad = 8.0 * s;
        let (hf, ef) = (font(10.0 * s, true), font(7.5 * s, false));
        let en_w = if t.label.is_empty() { 0.0 } else { measure(p, &t.name, &ef).min(head.width() * 0.45) };
        let title = draw::clip(p, label(t), &hf, head.width() - pad * 2.0 - en_w - 6.0 * s);
        p.text(pos2(head.min.x + pad, head.center().y), Align2::LEFT_CENTER, title, hf, a(TEXT));
        if !t.label.is_empty() {
            p.text(pos2(head.max.x - pad, head.center().y), Align2::RIGHT_CENTER, draw::clip(p, &t.name, &ef, en_w), ef, a(FAINT));
        }
        if !rows_readable {
            continue; // 縮めているときは表の名前だけ（文字の下限より行が低いと重なる）
        }
        let f = font(8.5 * s, false);
        let tag = font(7.0 * s, true);
        for (i, row) in rows(t, &h, look.opened.contains(&t.name)).iter().enumerate() {
            let y = head.max.y + row_px * (i as f32 + 0.5) + 2.0 * s;
            match row {
                Row::Hub(n) => {
                    p.text(pos2(r.min.x + pad, y), Align2::LEFT_CENTER, format!("← {n} 表から指される（線は引かない）"), f.clone(), a(FAINT));
                }
                Row::More(n) => {
                    p.text(pos2(r.min.x + pad + 26.0 * s, y), Align2::LEFT_CENTER, format!("ほか {n} 列（押すと開く）"), f.clone(), a(FAINT));
                }
                Row::Col(ci) => {
                    let c = &t.columns[*ci];
                    if c.pk {
                        p.text(pos2(r.min.x + pad, y), Align2::LEFT_CENTER, "鍵", tag.clone(), a(BONE));
                    }
                    let x = r.min.x + pad + 26.0 * s;
                    let target = if !c.fk.is_empty() { &c.fk } else { &c.r#ref };
                    let (right, ink) = if target.is_empty() {
                        (c.r#type.clone(), FAINT)
                    } else {
                        let to = tables.get(target.as_str()).map_or(target.as_str(), |t| label(t));
                        (format!("→ {to}"), if c.fk.is_empty() { DIM } else { BONE })
                    };
                    let right_w = measure(p, &right, &f).min(r.width() * 0.5);
                    p.text(pos2(r.max.x - pad, y), Align2::RIGHT_CENTER, draw::clip(p, &right, &f, right_w), f.clone(), a(ink));
                    let room = r.max.x - pad - right_w - 8.0 * s - x;
                    p.text(pos2(x, y), Align2::LEFT_CENTER, draw::clip(p, &c.name, &f, room), f.clone(), a(if c.pk { TEXT } else { DIM }));
                }
            }
        }
    }
    // 乗せている線の意味
    if let (Some(i), Some(at)) = (look.hover_edge, look.pointer) {
        if let Some(e) = lay.edges.get(i) {
            let g = p.layout_no_wrap(sentence(db, &e.rel), font(9.0, false), TEXT);
            let r = Rect::from_min_size(at + vec2(14.0, 14.0), g.size() + vec2(20.0, 14.0));
            p.rect(r, 6.0, PANEL, Stroke::new(1.0_f32, LINE), StrokeKind::Inside);
            p.galley(r.min + vec2(10.0, 7.0), g, TEXT);
        }
    }
}

// ---- 右の欄（押した表の中身・押していなければ図の見方）----

/// 右の欄で押されたもの
pub enum Action {
    /// 図に切り替えて、その部品を選ぶ
    Part(String),
    /// その表を選ぶ
    Table(String),
    /// 既定のブラウザで開く
    Open(String),
}

fn px(size: f32) -> f32 {
    (size * 1.33).round()
}

fn bold(size: f32) -> FontId {
    FontId::new(px(size), FontFamily::Name("bold".into()))
}

fn plain(size: f32) -> FontId {
    FontId::new(px(size), FontFamily::Proportional)
}

fn para(ui: &mut Ui, parts: &[(&str, FontId, Color32)]) {
    let mut job = LayoutJob::default();
    for (text, font, color) in parts {
        job.append(text, 0.0, TextFormat { font_id: font.clone(), color: *color, ..Default::default() });
    }
    job.wrap.max_width = ui.available_width();
    ui.label(job);
}

fn heading(ui: &mut Ui, s: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(s).font(bold(9.0)).color(FAINT));
}

fn link(ui: &mut Ui, s: &str, act: Action, out: &mut Option<Action>) {
    let r = ui.add(egui::Label::new(RichText::new(s).font(plain(10.0)).color(BONE)).sense(Sense::click())).on_hover_cursor(CursorIcon::PointingHand);
    if r.clicked() {
        *out = Some(act);
    }
}

/// 押した表の中身。picked が無ければ、図の見方と数
pub fn side(ui: &mut Ui, project: &str, db: &Db, picked: Option<&str>, parts: &HashMap<String, String>, github: &str) -> Option<Action> {
    let mut out = None;
    let names: HashMap<&str, &str> = db.tables.iter().map(|t| (t.name.as_str(), label(t))).collect();
    let part_name = |id: &str| parts.get(id).cloned().unwrap_or_else(|| id.split_once(':').map_or(id, |x| x.1).to_string());
    let Some(t) = picked.and_then(|n| db.tables.iter().find(|t| t.name == n)) else {
        ui.label(RichText::new(format!("{project} の DB")).font(bold(12.0)).color(TEXT));
        ui.add_space(6.0);
        let implied = rels(db).iter().filter(|r| r.implied).count();
        let counts = format!("表 {}・外部キー {}・宣言の無い関係 {implied}", db.tables.len(), foreign_keys(db));
        para(ui, &[(&counts, plain(10.0), TEXT)]);
        heading(ui, "見方");
        for (k, v) in [
            ("枠", "その表を主に書く部品。構成図の部品と同じ名前"),
            ("鍵", "この表の行を決める列（主キー）"),
            ("→ 表", "ほかの表の行を指す列。紺は宣言された外部キー、灰色は宣言は無く同じ値で引いている列"),
            ("実線", "宣言された外部キー"),
            ("点線", "宣言は無いが、同じ値で引いている関係"),
            ("線の端", "親の側に横棒（1）、子の側に 3 本の足（何件も）。子の側も横棒なら 1 対 1"),
            ("線が無い表", "ほかの表を指さない表（記録・鍵など）か、多くの表から指される表（列の → で示す）"),
        ] {
            para(ui, &[(&format!("{k}　"), bold(9.0), DIM), (v, plain(9.0), TEXT)]);
        }
        para(ui, &[("線に乗せると関係を文で出す。表を押すと中身。「ほか N 列」を押すと畳んだ列を開く", plain(9.0), FAINT)]);
        let h = hubs(db);
        if !h.is_empty() {
            heading(ui, "線にしない関係");
            for (p, n) in &h {
                let line = format!("{}（{p}）への関係 {n} 表。列に → で書く", names.get(p.as_str()).unwrap_or(&p.as_str()));
                para(ui, &[(&line, plain(9.0), TEXT)]);
            }
        }
        if !db.unlabeled.is_empty() {
            heading(ui, "日本語の名前が無い表");
            para(ui, &[(&db.unlabeled.join("・"), plain(9.0), TEXT)]);
        }
        for n in &db.notes {
            para(ui, &[(n, plain(9.0), FAINT)]);
        }
        if !db.unknown.is_empty() {
            heading(ui, "どの部品か分からないファイル");
            para(ui, &[(&db.unknown.join("・"), plain(9.0), TEXT)]);
        }
        return out;
    };
    ui.label(RichText::new(label(t)).font(bold(12.0)).color(TEXT));
    para(ui, &[(&t.name, plain(9.0), FAINT)]);
    if !t.note.is_empty() {
        ui.add_space(4.0);
        para(ui, &[(&t.note, plain(10.0), TEXT)]);
    }
    if !t.part.is_empty() {
        heading(ui, "主に書く部品");
        link(ui, &part_name(&t.part), Action::Part(t.part.clone()), &mut out);
    }
    for (title, ids) in [("書く部品", &t.write), ("読む部品", &t.read)] {
        if ids.is_empty() {
            continue;
        }
        heading(ui, title);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for id in ids {
                link(ui, &part_name(id), Action::Part(id.clone()), &mut out);
            }
        });
    }
    if !t.via.is_empty() {
        para(ui, &[(&format!("口: {}", t.via.join("・")), plain(9.0), FAINT)]);
    }
    heading(ui, &format!("列（{}）", t.columns.len()));
    for c in &t.columns {
        let target = if !c.fk.is_empty() { &c.fk } else { &c.r#ref };
        let ty = format!("  {}", c.r#type);
        let to = if target.is_empty() {
            String::new()
        } else {
            format!("  → {}{}", names.get(target.as_str()).unwrap_or(&target.as_str()), if c.fk.is_empty() { "（宣言なし）" } else { "" })
        };
        let mut line: Vec<(&str, FontId, Color32)> = vec![];
        if c.pk {
            line.push(("鍵 ", bold(9.0), BONE));
        }
        line.push((&c.name, if c.pk { bold(10.0) } else { plain(10.0) }, TEXT));
        line.push((&ty, plain(9.0), FAINT));
        if !to.is_empty() {
            line.push((&to, plain(9.0), if c.fk.is_empty() { DIM } else { BONE }));
        }
        para(ui, &line);
        if !c.note.is_empty() {
            para(ui, &[(&format!("　{}", c.note), plain(9.0), FAINT)]);
        }
    }
    let rs = rels(db);
    let mut kids: Vec<&str> = rs.iter().filter(|r| r.parent == t.name).map(|r| r.child.as_str()).collect();
    kids.dedup();
    if !kids.is_empty() {
        heading(ui, &format!("この表を指す表（{}）", kids.len()));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for k in kids {
                link(ui, names.get(k).unwrap_or(&k), Action::Table(k.to_string()), &mut out);
            }
        });
    }
    if !t.schema.is_empty() {
        heading(ui, "作った・変えた SQL");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for f in &t.schema {
                let short = f.rsplit('/').next().unwrap_or(f);
                if github.is_empty() {
                    para(ui, &[(short, plain(10.0), TEXT)]);
                } else {
                    link(ui, short, Action::Open(format!("https://github.com/{github}/blob/main/{f}")), &mut out);
                }
            }
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        serde_json::from_value(serde_json::json!({ "node": "p:db", "tables": [
            { "name": "parents", "label": "親", "part": "p:a", "columns": [{ "name": "id", "type": "text", "pk": true }, { "name": "created_at" }] },
            { "name": "kids", "label": "子", "part": "p:a", "columns": [
                { "name": "id", "type": "text", "pk": true },
                { "name": "parent_id", "type": "text", "fk": "parents" },
                { "name": "self_id", "type": "text", "fk": "kids" },
                { "name": "gone_id", "type": "text", "fk": "nowhere" },
                { "name": "created_at" }, { "name": "updated_at" }
            ] },
            { "name": "solo", "part": "p:b", "columns": [{ "name": "kid_id", "pk": true, "ref": "kids" }] },
            { "name": "logs", "columns": [{ "name": "at", "type": "text" }] }
        ] }))
        .unwrap()
    }

    #[test]
    fn 関係は宣言と宣言の無いもの_自分や図に無い表を指すものは除く() {
        let rs = rels(&db());
        assert_eq!(rs.len(), 2);
        assert_eq!((rs[0].child.as_str(), rs[0].parent.as_str(), rs[0].implied, rs[0].one), ("kids", "parents", false, false));
        assert_eq!((rs[1].child.as_str(), rs[1].parent.as_str(), rs[1].implied, rs[1].one), ("solo", "kids", true, true));
        assert_eq!(foreign_keys(&db()), 1);
        assert!(sentence(&db(), &rs[0]).starts_with("1 件の親に、子が何件も"));
        assert!(sentence(&db(), &rs[1]).starts_with("1 件の子に、soloが1 件（宣言は無く"));
    }

    #[test]
    fn よくある列は畳み_押すと開く() {
        let d = db();
        let h = hubs(&d);
        let kids = &d.tables[1];
        assert_eq!(rows(kids, &h, false), vec![Row::Col(0), Row::Col(1), Row::Col(2), Row::Col(3), Row::More(2)]);
        assert_eq!(rows(kids, &h, true).len(), 6);
        let opened = HashSet::new();
        let lay = er_layout(&d, &opened, 1.0).unwrap();
        let cam = fit(Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 1200.0)), &lay);
        let b = lay.boxes.iter().find(|b| b.name == "kids").unwrap();
        let r = rect_of(&cam, b);
        let more = pos2(r.center().x, r.min.y + (HEAD + ROW * 4.5) as f32 * cam.scale);
        assert_eq!(hit(&d, &lay, &cam, &opened, more), Some(Hit::Fold("kids".into())));
        assert_eq!(hit(&d, &lay, &cam, &opened, r.min + vec2(4.0, 4.0)), Some(Hit::Table("kids".into())));
        let opened: HashSet<String> = ["kids".to_string()].into();
        let tall = er_layout(&d, &opened, 1.0).unwrap();
        let b2 = tall.boxes.iter().find(|b| b.name == "kids").unwrap();
        assert!(b2.size.1 > b.size.1);
    }

    #[test]
    fn 表は部品ごとの枠に入り_箱は重ならない() {
        let d = db();
        let lay = er_layout(&d, &HashSet::new(), 1.0).unwrap();
        let parts: Vec<&str> = lay.frames.iter().map(|f| f.part.as_str()).collect();
        assert_eq!(parts, ["p:a", "p:b"]);
        let inside = |name: &str, part: &str| {
            let b = lay.boxes.iter().find(|b| b.name == name).unwrap();
            let f = lay.frames.iter().find(|f| f.part == part).unwrap();
            f.bx[0] <= b.at.0 && b.at.0 + b.size.0 <= f.bx[2] && f.bx[1] <= b.at.1 && b.at.1 + b.size.1 <= f.bx[3]
        };
        assert!(inside("parents", "p:a") && inside("kids", "p:a") && inside("solo", "p:b"));
        for (i, a) in lay.boxes.iter().enumerate() {
            for b in &lay.boxes[i + 1..] {
                let apart = a.at.0 + a.size.0 <= b.at.0 || b.at.0 + b.size.0 <= a.at.0 || a.at.1 + a.size.1 <= b.at.1 || b.at.1 + b.size.1 <= a.at.1;
                assert!(apart, "{} と {} が重なる", a.name, b.name);
            }
        }
        assert_eq!(lay.edges.len(), 2);
        assert!(er_layout(&Db::default(), &HashSet::new(), 1.0).unwrap().boxes.is_empty());
    }

    #[test]
    fn 多くの表が指す表への関係は線にしない_小さな図は真ん中_大きな図は読める倍率で左上() {
        let mut tables = vec![
            serde_json::json!({ "name": "hub", "columns": [{ "name": "id", "pk": true }] }),
            serde_json::json!({ "name": "b", "columns": [{ "name": "id", "pk": true }] }),
        ];
        for i in 0..7 {
            let mut cols = vec![serde_json::json!({ "name": "hub_id", "fk": "hub" })];
            if i == 0 {
                cols.push(serde_json::json!({ "name": "b_id", "ref": "b" }));
            }
            tables.push(serde_json::json!({ "name": format!("t{i}"), "columns": cols }));
        }
        let d: Db = serde_json::from_value(serde_json::json!({ "node": "p:db", "tables": tables })).unwrap();
        assert_eq!(hubs(&d), BTreeMap::from([("hub".to_string(), 7)]));
        assert_eq!(drawn(&d).len(), 1);
        assert_eq!(rows(&d.tables[0], &hubs(&d), false)[0], Row::Hub(7));
        let big = ErLayout { size: (3000.0, 3000.0), ..Default::default() };
        let area = Rect::from_min_size(pos2(10.0, 20.0), vec2(800.0, 600.0));
        let cam = fit(area, &big);
        assert_eq!((cam.scale, cam.origin), (READABLE, pos2(26.0, 36.0)));
        assert!(fit(area, &er_layout(&d, &HashSet::new(), 1.0).unwrap()).scale >= READABLE);
    }
}
