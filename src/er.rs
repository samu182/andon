//! DB の図（ER 図）。DB の部品を押すと、窓の左半分に出す。表ごとに箱を描いて列を並べ、外部キーを線でつなぐ。
//! 表と列・外部キー・どの部品が書く・読むかは、カンバンの arch.py の db が決める（state の arch.db）。ここは並べて描くだけ。
//! 並べ方は Graphviz（dot）。参照される表を上に、参照する表を下に置く（横に流す向きと比べて、窓の形に近い方）。

use crate::draw::{self, BONE, Camera, DIM, FAINT, LINE, PANEL, TEXT, font, measure};
use crate::layout::{self, Pt};
use crate::model::{Db, Table};
use egui::{Align2, Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};
use std::collections::{HashMap, HashSet};

/// 表の箱の幅・見出しの高さ・1 行の高さ（点）
pub const BOX_W: f64 = 230.0;
pub const HEAD: f64 = 26.0;
pub const ROW: f64 = 17.0;
const BOX_PAD: f64 = 6.0;
/// 線の色。普段は薄く、選んだ表の線だけ濃く（参照される表に線が集まるので、全部を濃くすると読めない）
const QUIET: Color32 = Color32::from_rgb(0x9a, 0xa3, 0xad);
const LOUD: Color32 = BONE;
/// 列が読める倍率（行の高さが文字の下限より高い）。全体を収めるとこれより小さくなるときは、この倍率で左上から出す
pub const READABLE: f32 = 0.8;
/// 図の背景（あんどんの図と分ける）
pub const BACK: Color32 = Color32::from_rgb(0xf6, 0xf7, 0xf9);
const HEAD_FILL: Color32 = Color32::from_rgb(0xf3, 0xf4, 0xf6);

#[derive(Clone, Debug)]
pub struct ErBox {
    pub name: String,
    /// 左上と大きさ（点）
    pub at: Pt,
    pub size: Pt,
}

/// 外部キー。from が参照する表（子）、to が参照される表（親）。points は親から子へ
#[derive(Clone, Debug)]
pub struct ErEdge {
    pub from: String,
    pub to: String,
    pub points: Vec<Pt>,
}

#[derive(Clone, Debug, Default)]
pub struct ErLayout {
    pub boxes: Vec<ErBox>,
    pub edges: Vec<ErEdge>,
    pub size: Pt,
}

fn box_h(t: &Table) -> f64 {
    HEAD + ROW * t.columns.len().max(1) as f64 + BOX_PAD
}

/// 外部キーの組（子の表・列・親の表）。親が図に無い・自分を指すものは除く
pub fn foreign_keys(db: &Db) -> Vec<(String, String, String)> {
    let names: HashSet<&str> = db.tables.iter().map(|t| t.name.as_str()).collect();
    let mut out = vec![];
    for t in &db.tables {
        for c in &t.columns {
            if !c.fk.is_empty() && c.fk != t.name && names.contains(c.fk.as_str()) {
                out.push((t.name.clone(), c.name.clone(), c.fk.clone()));
            }
        }
    }
    out
}

/// 多くの表が指す表（指す表が 6 つ以上かつ全体の 4 割以上）。ここへの外部キーは線にせず、列に「→ 表」と書く。
/// どの表もその表を指していると、線が 1 か所に集まって図が横に広がり、ほかの線が読めなくなった（2026-10-23）
pub fn hubs(db: &Db) -> HashSet<String> {
    let mut by: HashMap<&str, HashSet<&str>> = HashMap::new();
    let all = foreign_keys(db);
    for (child, _, parent) in &all {
        by.entry(parent.as_str()).or_default().insert(child.as_str());
    }
    let need = 6usize.max((db.tables.len() as f64 * 0.4).ceil() as usize);
    by.into_iter().filter(|(_, c)| c.len() >= need).map(|(p, _)| p.to_string()).collect()
}

/// 線にする外部キー（hubs への外部キーは除く）
pub fn drawn_keys(db: &Db) -> Vec<(String, String, String)> {
    let h = hubs(db);
    foreign_keys(db).into_iter().filter(|(_, _, p)| !h.contains(p)).collect()
}

fn one(db: &Db, rankdir: &str) -> Result<ErLayout, String> {
    let names: HashMap<&str, String> = db.tables.iter().enumerate().map(|(i, t)| (t.name.as_str(), format!("t{i}"))).collect();
    let mut lines = vec![
        "digraph G {".to_string(),
        format!("rankdir={rankdir}; nodesep=0.45; ranksep=0.8; splines=spline;"),
        r#"node [shape=box, fixedsize=true, label=""];"#.to_string(),
    ];
    for t in &db.tables {
        lines.push(format!("{} [width={:.3}, height={:.3}];", names[t.name.as_str()], BOX_W / 72.0, box_h(t) / 72.0));
    }
    let fks = drawn_keys(db);
    for (child, _, parent) in &fks {
        lines.push(format!("{} -> {};", names[parent.as_str()], names[child.as_str()]));
    }
    // 外部キーでつながらない表は、1 列に長く並ばないよう、見えない線で数列の格子にする（今の図の枠の中と同じ）
    let linked: HashSet<&str> = fks.iter().flat_map(|(c, _, p)| [c.as_str(), p.as_str()]).collect();
    let free: Vec<&Table> = db.tables.iter().filter(|t| !linked.contains(t.name.as_str())).collect();
    if free.len() > 3 {
        let f = free.len() as f64;
        let rows = (f / (f / 2.0).sqrt().ceil()).ceil() as usize;
        for i in 0..free.len().saturating_sub(rows) {
            lines.push(format!("{} -> {} [style=invis];", names[free[i].name.as_str()], names[free[i + rows].name.as_str()]));
        }
    }
    lines.push("}".to_string());
    let g = layout::run_dot(&lines.join("\n"))?;
    let bb = layout::floats(&g["bb"]);
    let (x0, y1) = (bb[0], bb[3]);
    let flip = move |x: f64, y: f64| (x - x0, y1 - y);
    let by_name: HashMap<&str, &Table> = db.tables.iter().map(|t| (names[t.name.as_str()].as_str(), t)).collect();
    let mut boxes = vec![];
    for o in g["objects"].as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
        if let Some(t) = by_name.get(o["name"].as_str().unwrap_or("")) {
            let p = layout::floats(&o["pos"]);
            let (cx, cy) = flip(p[0], p[1]);
            let h = box_h(t);
            boxes.push(ErBox { name: t.name.clone(), at: (cx - BOX_W / 2.0, cy - h / 2.0), size: (BOX_W, h) });
        }
    }
    boxes.sort_by_key(|b| db.tables.iter().position(|t| t.name == b.name));
    let edges = g["edges"]
        .as_array()
        .map(|v| v.as_slice())
        .unwrap_or(&[])
        .iter()
        .take(fks.len())
        .zip(&fks)
        .map(|(e, (child, _, parent))| ErEdge {
            from: child.clone(),
            to: parent.clone(),
            points: layout::spline(e["pos"].as_str().unwrap_or(""), &flip),
        })
        .collect();
    Ok(ErLayout { boxes, edges, size: (bb[2] - bb[0], bb[3] - bb[1]) })
}

/// 上下に流す向きと横に流す向きを並べ、窓の形（aspect = 幅 / 高さ）に近い方を選ぶ
pub fn er_layout(db: &Db, aspect: f64) -> Result<ErLayout, String> {
    if db.tables.is_empty() {
        return Ok(ErLayout::default());
    }
    let tb = one(db, "TB")?;
    let lr = one(db, "LR")?;
    let off = |l: &ErLayout| ((l.size.0 / l.size.1.max(1.0)) / aspect).ln().abs();
    Ok(if off(&lr) < off(&tb) { lr } else { tb })
}

/// 全体を収める
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

/// その場所にある表
pub fn table_at(cam: &Camera, lay: &ErLayout, at: Pos2) -> Option<String> {
    lay.boxes.iter().rev().find(|b| rect_of(cam, b).contains(at)).map(|b| b.name.clone())
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

/// 図を描く。focus は選んでいる表か、乗せている表（その表の線を濃く、つながらない表を薄く）
pub fn paint(p: &Painter, db: &Db, lay: &ErLayout, cam: &Camera, focus: Option<&str>) {
    let s = cam.scale;
    let near: HashSet<&str> = match focus {
        Some(f) => lay.edges.iter().filter(|e| e.from == f || e.to == f).flat_map(|e| [e.from.as_str(), e.to.as_str()]).chain([f]).collect(),
        None => HashSet::new(),
    };
    // 線（薄いものを先に、濃いものを上に）
    for loud in [false, true] {
        for e in &lay.edges {
            let on = focus.is_some_and(|f| e.from == f || e.to == f);
            if on != loud || e.points.len() < 2 {
                continue;
            }
            let pts: Vec<Pos2> = e.points.iter().map(|q| cam.xy(*q)).collect();
            let color = if on { LOUD } else { QUIET };
            p.add(Shape::line(pts.clone(), Stroke::new(if on { 1.6_f32 } else { 1.0 }, color)));
            let n = pts.len();
            crow(p, pts[n - 1], pts[n - 1] - pts[n - 2], s.max(0.6), color);
            // 親の側は「1」の短い横棒
            let d = (pts[1] - pts[0]).normalized();
            let m = pts[0] + d * 7.0 * s.max(0.6);
            let nn = vec2(-d.y, d.x) * 5.0 * s.max(0.6);
            p.line_segment([m - nn, m + nn], Stroke::new(1.2_f32, color));
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
        let alpha = |c: Color32| if dim { c.gamma_multiply(0.35) } else { c };
        p.rect(r, 4.0 * s, PANEL, Stroke::new(if on { 2.0_f32 } else { 1.0 }, if on { BONE } else { alpha(LINE) }), StrokeKind::Inside);
        let head = Rect::from_min_size(r.min, vec2(r.width(), HEAD as f32 * s));
        p.rect_filled(head.shrink(1.0), 3.0 * s, alpha(HEAD_FILL));
        p.line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0_f32, alpha(LINE)));
        let hf = font(10.0 * s, true);
        let pad = 8.0 * s;
        p.text(pos2(head.min.x + pad, head.center().y), Align2::LEFT_CENTER, draw::clip(p, &t.name, &hf, head.width() - pad * 2.0), hf, alpha(TEXT));
        if !rows_readable {
            continue; // 縮めているときは表の名前だけ（文字の下限より行が低いと重なる）
        }
        let f = font(8.5 * s, false);
        let tag = font(7.0 * s, true);
        for (i, c) in t.columns.iter().enumerate() {
            let y = head.max.y + row_px * (i as f32 + 0.5) + 2.0 * s;
            let mut x = r.min.x + pad;
            let mark = if c.pk { "PK" } else if !c.fk.is_empty() { "FK" } else { "" };
            if !mark.is_empty() {
                p.text(pos2(x, y), Align2::LEFT_CENTER, mark, tag.clone(), alpha(if c.pk { BONE } else { DIM }));
            }
            x += 22.0 * s;
            let (right, ink) = if c.fk.is_empty() { (c.r#type.clone(), FAINT) } else { (format!("→ {}", c.fk), BONE) };
            let type_w = measure(p, &right, &f).min(r.width() * 0.45);
            p.text(pos2(r.max.x - pad, y), Align2::RIGHT_CENTER, draw::clip(p, &right, &f, type_w), f.clone(), alpha(ink));
            let room = r.max.x - pad - type_w - 8.0 * s - x;
            p.text(pos2(x, y), Align2::LEFT_CENTER, draw::clip(p, &c.name, &f, room), f.clone(), alpha(if c.pk { TEXT } else { DIM }));
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        serde_json::from_value(serde_json::json!({ "node": "p:db", "tables": [
            { "name": "parents", "columns": [{ "name": "id", "type": "text", "pk": true }] },
            { "name": "kids", "columns": [
                { "name": "id", "type": "text", "pk": true },
                { "name": "parent_id", "type": "text", "fk": "parents" },
                { "name": "self_id", "type": "text", "fk": "kids" },
                { "name": "gone_id", "type": "text", "fk": "nowhere" }
            ] },
            { "name": "logs", "columns": [{ "name": "at", "type": "text" }] }
        ] }))
        .unwrap()
    }

    #[test]
    fn 外部キーは図にある親だけ_自分を指すものは線にしない() {
        assert_eq!(foreign_keys(&db()), vec![("kids".to_string(), "parent_id".to_string(), "parents".to_string())]);
    }

    #[test]
    fn 表ごとに箱を並べ_親から子へ線を引き_箱は列の数だけ高い() {
        let lay = er_layout(&db(), 1.0).unwrap();
        let names: Vec<&str> = lay.boxes.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["parents", "kids", "logs"]);
        assert_eq!(lay.boxes[1].size, (BOX_W, HEAD + ROW * 4.0 + BOX_PAD));
        assert_eq!(lay.edges.len(), 1);
        assert_eq!((lay.edges[0].from.as_str(), lay.edges[0].to.as_str()), ("kids", "parents"));
        assert!(lay.edges[0].points.len() >= 2);
        // 箱どうしは重ならない
        for (i, a) in lay.boxes.iter().enumerate() {
            for b in &lay.boxes[i + 1..] {
                let apart = a.at.0 + a.size.0 <= b.at.0 || b.at.0 + b.size.0 <= a.at.0 || a.at.1 + a.size.1 <= b.at.1 || b.at.1 + b.size.1 <= a.at.1;
                assert!(apart, "{} と {} が重なる", a.name, b.name);
            }
        }
        // 箱を押すとその表
        let cam = fit(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)), &lay);
        let c = Rect::from_min_size(cam.xy(lay.boxes[0].at), vec2(10.0, 10.0)).center();
        assert_eq!(table_at(&cam, &lay, c).as_deref(), Some("parents"));
        assert!(er_layout(&Db::default(), 1.0).unwrap().boxes.is_empty());
    }

    #[test]
    fn 多くの表が指す表への外部キーは線にしない() {
        // 7 表が hub を指し、a だけが b を指す（hub は 6 表以上かつ 4 割以上に指される）
        let mut tables = vec![serde_json::json!({ "name": "hub", "columns": [{ "name": "id", "pk": true }] }),
                              serde_json::json!({ "name": "b", "columns": [{ "name": "id", "pk": true }] })];
        for i in 0..7 {
            let mut cols = vec![serde_json::json!({ "name": "hub_id", "fk": "hub" })];
            if i == 0 {
                cols.push(serde_json::json!({ "name": "b_id", "fk": "b" }));
            }
            tables.push(serde_json::json!({ "name": format!("t{i}"), "columns": cols }));
        }
        let db: Db = serde_json::from_value(serde_json::json!({ "node": "p:db", "tables": tables })).unwrap();
        assert_eq!(hubs(&db), HashSet::from(["hub".to_string()]));
        assert_eq!(drawn_keys(&db), vec![("t0".to_string(), "b_id".to_string(), "b".to_string())]);
        assert_eq!(foreign_keys(&db).len(), 8); // 数えるときは全部
        let lay = er_layout(&db, 1.0).unwrap();
        assert_eq!(lay.edges.len(), 1);
        // 小さな図は読める倍率のまま真ん中に、大きな図は読める倍率で左上から
        let big = ErLayout { size: (3000.0, 3000.0), ..Default::default() };
        let area = Rect::from_min_size(pos2(10.0, 20.0), vec2(800.0, 600.0));
        let cam = fit(area, &big);
        assert_eq!((cam.scale, cam.origin), (READABLE, pos2(26.0, 36.0)));
        assert!(fit(area, &lay).scale >= READABLE);
    }
}
