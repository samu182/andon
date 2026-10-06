//! 描く。arch_view.py の redraw の写し（見た目の決まりは向こうのコメントが正本）。

use crate::layout::{Layout, NODE_H, NODE_W, Pt, ZONE_HEAD};
use crate::model::{Changes, View, Work, bones, through};
use egui::{
    Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2,
};
use std::collections::HashMap;
use std::sync::Arc;

// 色。無彩色で組み、色は意味のある所だけ（2026-10-06: 色を均等にばらまくと「AI で作った感」が出た）
pub const BG: Color32 = hex(0xeceef1);
const DOT: Color32 = hex(0xd5d9de);
pub const PANEL: Color32 = hex(0xffffff);
pub const LINE: Color32 = hex(0xd5d9de);
const CARD: Color32 = hex(0xffffff);
const SHADOW: Color32 = hex(0xe4e7eb);
pub const TEXT: Color32 = hex(0x1f2328);
pub const DIM: Color32 = hex(0x4b535d);
pub const FAINT: Color32 = hex(0x636c76);
const EDGE: Color32 = hex(0x5f6772);
const EDGE_QUIET: Color32 = hex(0xd9dde2);
const CARD_LINE: Color32 = hex(0xbfc5cd);
const TINT_WIP: Color32 = hex(0xe6effc);
const TINT_STOP: Color32 = hex(0xfbf1d6);
const TINT_NONE: Color32 = hex(0xeef0f2);
pub const BONE: Color32 = hex(0x1e3a8a); // アクセントは紺 1 色だけ（選んだもの・リンク・骨格の番号）
const WHITE: Color32 = hex(0xffffff);
pub const ERROR: Color32 = hex(0xcf222e); // 赤はエラーだけ（待ちはエラーではない）
const KIND_MARK: Color32 = hex(0xe2e5e9); // 外部・社内の仕組み・外部サービスは無彩色の淡い灰色
const PROJECT_MARKS: [Color32; 2] = [hex(0xdce6f4), hex(0xe7dcf4)]; // 青と紫（明るさ・鮮やかさは同じで、色相だけ変える）
const R_NODE: f32 = 6.0;
const R_ZONE: f32 = 10.0;
const DOT_STEP: f32 = 22.0;
pub const HEAD_H: f32 = 44.0;
pub const STRIP_H: f32 = 64.0;
pub const TAB_H: f32 = 36.0;

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub fn status_color(kind: &str) -> Color32 {
    match kind {
        "live" => hex(0x1a7f37),
        "wip" => hex(0x2f6fd0),
        "stop" => hex(0xbf8700),
        _ => hex(0x8c959f),
    }
}

fn status_short(kind: &str) -> &'static str {
    match kind {
        "live" => "本番",
        "wip" => "作りかけ",
        "stop" => "待ち",
        _ => "まだ無い",
    }
}

/// 線の種類は 4 つだけ。形で分ける
fn dash(kind: &str) -> &'static [f32] {
    match kind {
        "live" => &[],
        "wip" => &[7.0, 5.0],
        "stop" => &[10.0, 4.0, 2.0, 4.0],
        _ => &[2.0, 5.0],
    }
}

/// Windows に入っている BIZ UDPゴシック（ttc の 2 番目の面）を使う。egui の標準のフォントには日本語が無い。
/// Yu Gothic UI は細く薄く出る。BIZ UDP は太めで小さくても読める（2026-10-06）
pub fn setup_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let mut add = |name: &str, file: &str, index: u32| {
        let Ok(bytes) = std::fs::read(format!(r"C:\Windows\Fonts\{file}")) else { return false };
        let mut data = FontData::from_owned(bytes);
        data.index = index;
        defs.font_data.insert(name.to_string(), Arc::new(data));
        true
    };
    let regular = add("biz", "BIZ-UDGothicR.ttc", 1);
    let bold = add("biz-bold", "BIZ-UDGothicB.ttc", 1);
    let num = add("consolas-bold", "consolab.ttf", 0);
    let base = defs.families[&FontFamily::Proportional].clone();
    if regular {
        defs.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "biz".into());
    }
    let mut bold_family = base.clone();
    if bold {
        bold_family.insert(0, "biz-bold".into());
    } else if regular {
        bold_family.insert(0, "biz".into());
    }
    let mut num_family = bold_family.clone();
    if num {
        num_family.insert(0, "consolas-bold".into());
    }
    defs.families.insert(FontFamily::Name("bold".into()), bold_family);
    defs.families.insert(FontFamily::Name("num".into()), num_family);
    ctx.set_fonts(defs);
}

/// 文字の下限（画素）。8 だと拡大率 150% の画面で潰れて読めなかった（2026-10-06）。
/// 図を縮めても文字はこれより小さくせず、入りきらない分は「…」で切る
pub const MIN_FONT: f32 = 11.0;

/// 大きさは arch_view.py の font と同じ（px * 1.33 を整数に丸める）。下限だけ MIN_FONT に上げた
pub fn font(px: f32, bold: bool) -> FontId {
    let size = (px * 1.33).round().max(MIN_FONT);
    FontId::new(size, if bold { FontFamily::Name("bold".into()) } else { FontFamily::Proportional })
}

fn num_font(px: f32) -> FontId {
    FontId::new((px * 1.33).round().max(MIN_FONT), FontFamily::Name("num".into()))
}

pub fn measure(p: &Painter, text: &str, f: &FontId) -> f32 {
    p.layout_no_wrap(text.to_string(), f.clone(), TEXT).size().x
}

/// 四角の幅に収まらない文字は、末尾を「…」にして切る（はみ出すと隣の四角に重なる）。
pub fn clip(p: &Painter, text: &str, f: &FontId, room: f32) -> String {
    if measure(p, text, f) <= room {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() && measure(p, &(chars.iter().collect::<String>() + "…"), f) > room {
        chars.pop();
    }
    chars.into_iter().collect::<String>() + "…"
}

/// 折れ線を、決まった並び（線・すき間・線・すき間…）で切る。並びが空なら切らない。
fn dashed(points: &[Pos2], pattern: &[f32]) -> Vec<Vec<Pos2>> {
    if pattern.is_empty() || points.len() < 2 {
        return vec![points.to_vec()];
    }
    let (mut out, mut cur) = (vec![], vec![points[0]]);
    let (mut k, mut left, mut on) = (0, pattern[0], true);
    for w in points.windows(2) {
        let (mut a, b) = (w[0], w[1]);
        let mut seg = a.distance(b);
        while seg > 0.0 {
            let step = left.min(seg);
            let q = a + (b - a) * (step / seg);
            if on {
                cur.push(q);
            }
            seg -= step;
            left -= step;
            a = q;
            if left <= 1e-4 {
                if on && cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                k = (k + 1) % pattern.len();
                left = pattern[k];
                on = !on;
                cur = if on { vec![a] } else { vec![] };
            }
        }
    }
    if on && cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// 矢じり（tk の arrowshape と同じ 3 つの長さ: 首までの長さ・翼までの長さ・線からの翼の幅）。線の端は首まで縮める。
fn arrow_head(p: &Painter, pts: &mut [Pos2], shape: (f32, f32, f32), width: f32, color: Color32) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let tip = pts[n - 1];
    let Some(prev) = pts[..n - 1].iter().rev().find(|q| q.distance(tip) > 0.5) else { return };
    let dir = (tip - *prev).normalized();
    let perp = vec2(-dir.y, dir.x);
    let (d1, d2, d3) = shape;
    let neck = tip - dir * d1;
    let back = tip - dir * d2;
    let wing = d3 + width / 2.0;
    p.add(Shape::convex_polygon(vec![tip, back + perp * wing, neck], color, Stroke::NONE));
    p.add(Shape::convex_polygon(vec![tip, neck, back - perp * wing], color, Stroke::NONE));
    pts[n - 1] = neck;
}

fn polyline(p: &Painter, mut pts: Vec<Pos2>, width: f32, color: Color32, pattern: &[f32], arrow: &str, shape: (f32, f32, f32)) {
    if arrow == "last" || arrow == "both" {
        arrow_head(p, &mut pts, shape, width, color);
    }
    if arrow == "first" || arrow == "both" {
        pts.reverse();
        arrow_head(p, &mut pts, shape, width, color);
        pts.reverse();
    }
    for part in dashed(&pts, pattern) {
        p.add(Shape::line(part, Stroke::new(width, color)));
    }
}

/// 角丸の四角の輪郭の点（閉じた形）
fn rounded_points(r: Rect, radius: f32) -> Vec<Pos2> {
    let radius = radius.min(r.width() / 2.0).min(r.height() / 2.0);
    let corners = [
        (pos2(r.max.x - radius, r.min.y + radius), -90.0f32),
        (pos2(r.max.x - radius, r.max.y - radius), 0.0),
        (pos2(r.min.x + radius, r.max.y - radius), 90.0),
        (pos2(r.min.x + radius, r.min.y + radius), 180.0),
    ];
    let mut out = vec![];
    for (c, start) in corners {
        for i in 0..=6 {
            let a = (start + 90.0 * i as f32 / 6.0).to_radians();
            out.push(c + vec2(a.cos(), a.sin()) * radius);
        }
    }
    out.push(out[0]);
    out
}

fn rounded(p: &Painter, r: Rect, radius: f32, fill: Option<Color32>, stroke: Option<(f32, Color32)>, pattern: &[f32]) {
    let cr = CornerRadius::from(radius.min(r.width() / 2.0).min(r.height() / 2.0).min(255.0));
    if let Some(fill) = fill {
        p.rect_filled(r, cr, fill);
    }
    if let Some((w, c)) = stroke {
        if pattern.is_empty() {
            p.rect_stroke(r, cr, Stroke::new(w, c), StrokeKind::Middle);
        } else {
            for part in dashed(&rounded_points(r, radius), pattern) {
                p.add(Shape::line(part, Stroke::new(w, c)));
            }
        }
    }
}

/// 丸端のチップ（両端が半円）
pub fn pill(p: &Painter, r: Rect, fill: Color32, outline: Color32) {
    rounded(p, r, r.height() / 2.0, Some(fill), Some((1.0, outline)), &[]);
}

/// 点の方眼の背景（図を動かしても紙は動かさない）
pub fn paper(p: &Painter, area: Rect) {
    p.rect_filled(area, 0.0, BG);
    let mut y = area.min.y + DOT_STEP / 2.0;
    while y < area.max.y {
        let mut x = area.min.x + DOT_STEP / 2.0;
        while x < area.max.x {
            p.rect_filled(Rect::from_min_size(pos2(x, y), vec2(2.0, 2.0)), 0.0, DOT);
            x += DOT_STEP;
        }
        y += DOT_STEP;
    }
}

/// 図を描く位置と倍率
#[derive(Clone, Copy)]
pub struct Camera {
    pub scale: f32,
    pub origin: Pos2,
}

impl Camera {
    /// 窓に全体を収める（拡大は 1.4 倍まで）。枠の外を回る線（全体の上・下・右のすき間）も含めて収める
    pub fn fit(area: Rect, lay: &Layout) -> Self {
        let (mut x0, mut y0, mut x1, mut y1) = (0.0f64, 0.0f64, lay.size.0, lay.size.1);
        let lines = lay.paths.iter().map(|p| &p.points).chain(lay.bundles.iter().map(|b| &b.points));
        for q in lines.flatten() {
            (x0, y0, x1, y1) = (x0.min(q.0), y0.min(q.1), x1.max(q.0), y1.max(q.1));
        }
        let (x0, y0) = (x0 as f32 - 10.0, y0 as f32 - 10.0); // 矢じりと名札のぶん
        let (sw, sh) = (x1 as f32 + 10.0 - x0, y1 as f32 + 10.0 - y0);
        let scale = ((area.width() - 24.0) / sw).min((area.height() - 24.0) / sh).min(1.4);
        let origin = area.min + vec2((area.width() - sw * scale) / 2.0 - x0 * scale, (area.height() - sh * scale) / 2.0 - y0 * scale);
        Camera { scale, origin }
    }

    pub fn xy(&self, p: Pt) -> Pos2 {
        self.origin + vec2(p.0 as f32 * self.scale, p.1 as f32 * self.scale)
    }
}

/// 名札を置くときに避けるもの（部品・線・ほかの名札）
#[derive(Default)]
struct Crowd {
    boxes: Vec<(Rect, usize)>,
    lines: Vec<(Vec<Pos2>, usize)>,
}

impl Crowd {
    fn count(&self, r: Rect, own: usize) -> usize {
        let b = self.boxes.iter().filter(|(x, id)| *id != own && x.intersects(r)).count();
        let l = self
            .lines
            .iter()
            .filter(|(pts, id)| *id != own && pts.windows(2).any(|s| Rect::from_two_pos(s[0], s[1]).expand(1.0).intersects(r)))
            .count();
        b + l
    }
}

/// 線の上で、ほかの線・部品・名札にいちばんかぶらない場所に名札を置く（真ん中から外へ順に試す）。
fn place_label(p: &Painter, crowd: &mut Crowd, text: &str, f: &FontId, pts: &[Pos2], own: usize) {
    if pts.len() < 2 {
        return;
    }
    let seg: Vec<f32> = pts.windows(2).map(|s| s[0].distance(s[1])).collect();
    let total = seg.iter().sum::<f32>().max(1.0);
    let (w, h) = (measure(p, text, f) + 10.0, f.size + 4.0);
    let mut best: Option<(usize, Pos2)> = None;
    for frac in [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8, 0.12, 0.88] {
        let (mut d, mut at) = (frac * total, pts[0]);
        for (s, l) in pts.windows(2).zip(&seg) {
            if d <= *l && *l > 0.0 {
                at = s[0] + (s[1] - s[0]) * (d / l);
                break;
            }
            d -= l;
        }
        let n = crowd.count(Rect::from_center_size(at, vec2(w, h)), own);
        if best.is_none_or(|b| n < b.0) {
            best = Some((n, at));
        }
        if n == 0 {
            break;
        }
    }
    let at = best.unwrap().1;
    let r = Rect::from_center_size(at, vec2(w, h));
    pill(p, r, PANEL, LINE);
    p.text(at, Align2::CENTER_CENTER, text, f.clone(), DIM);
    crowd.boxes.push((r, own));
}

/// 何を見ているか（選んだ部品・マウスを乗せた部品と線）
#[derive(Default)]
pub struct Focus<'a> {
    pub selected: Option<&'a str>,
    pub hover_node: Option<&'a str>,
    pub hover_edge: Option<usize>,
    /// 骨格（ウォーキングスケルトン）以外を薄くする
    pub bones_only: bool,
    /// 作業中の部品（herdr のペインのブランチ → PR → カード → 部品）
    pub working: Option<&'a HashMap<String, Work>>,
}

/// 部品の四角（画面の座標）
pub fn node_rect(cam: &Camera, c: Pt) -> Rect {
    let s = cam.scale;
    Rect::from_center_size(cam.xy(c), vec2(NODE_W as f32 * 72.0 * s, NODE_H as f32 * 72.0 * s))
}

/// 折れ線と点のいちばん近い距離
pub fn distance_to(pts: &[Pos2], at: Pos2) -> f32 {
    pts.windows(2)
        .map(|s| {
            let (a, b) = (s[0], s[1]);
            let ab = b - a;
            let t = if ab.length_sq() > 0.0 { ((at - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
            at.distance(a + ab * t)
        })
        .fold(f32::INFINITY, f32::min)
}

pub fn diagram(p: &Painter, view: &View, lay: &Layout, changes: &Changes, cam: &Camera, focus: &Focus) {
    let s = cam.scale;
    let fnode = focus.hover_node.or(focus.selected);
    let mut marks = PROJECT_MARKS.iter().cycle();
    let zone_mark: HashMap<&str, Color32> = view
        .zones
        .iter()
        .map(|z| (z.id.as_str(), if matches!(z.kind.as_str(), "external" | "company" | "cloud") { KIND_MARK } else { *marks.next().unwrap() }))
        .collect();
    // 同じ理由で止まっている部品を囲んだ枠の、理由
    let mut reasons: HashMap<String, &str> = HashMap::new();
    for (zi, z) in view.zones.iter().enumerate() {
        let mut groups: Vec<&str> = vec![];
        for n in &view.nodes {
            if n.zone == z.id && !n.blocked.is_empty() && !groups.contains(&n.blocked.as_str()) {
                groups.push(&n.blocked);
            }
        }
        for (gi, reason) in groups.iter().enumerate() {
            reasons.insert(format!("z{zi}_b{gi}"), reason);
        }
    }
    // 枠（システム）: 系の区別は淡い背景色だけ
    let mut tags_later = vec![];
    for f in &lay.frames {
        let r = Rect::from_min_max(cam.xy((f.bx[0], f.bx[1])), cam.xy((f.bx[2], f.bx[3])));
        if let Some(reason) = reasons.get(&f.key) {
            // 止まっている所: 灰色の破線で囲み、札は部品のあとに描く
            rounded(p, r, 0.0, None, Some((1.0, FAINT)), &[4.0, 3.0]);
            tags_later.push((r, *reason));
            continue;
        }
        let Some(z) = f.key[1..].parse::<usize>().ok().and_then(|i| view.zones.get(i)) else { continue };
        rounded(p, r, R_ZONE * s, Some(zone_mark[z.id.as_str()]), Some((1.0, CARD_LINE)), &[]);
        // 名前と説明は、枠の上の空き（ZONE_HEAD）に収める。文字の下限で大きくなった分は詰め、入らなければ説明を出さない
        let (tf, sf) = (font(14.0 * s, true), font(9.0 * s, false));
        let head = ZONE_HEAD as f32 * s;
        let top = (10.0 * s).min((head - tf.size - sf.size - 4.0) / 2.0).max(3.0);
        let room = r.width() - 32.0 * s;
        let t = p.text(r.min + vec2(16.0 * s, top), Align2::LEFT_TOP, clip(p, &z.title, &tf, room), tf, TEXT);
        if !z.sub.is_empty() && t.max.y + 2.0 + sf.size <= r.min.y + head - 2.0 {
            let y = (r.min.y + 33.0 * s).min(t.max.y + 2.0);
            p.text(pos2(r.min.x + 16.0 * s, y), Align2::LEFT_TOP, clip(p, &z.sub, &sf, room), sf, DIM);
        }
    }
    // 線。普段は枠の中の線と、枠どうしをまとめた線だけ。部品を押す・乗せると、その部品の線を行き先まで出す
    let zone_of: HashMap<&str, &str> = view.nodes.iter().map(|n| (n.id.as_str(), n.zone.as_str())).collect();
    let mine: Vec<usize> = (0..view.edges.len()).filter(|&i| fnode.is_some_and(|f| view.edges[i].from == f || view.edges[i].to == f)).collect();
    let mut near: Vec<&str> = fnode.into_iter().collect();
    near.extend(mine.iter().flat_map(|&i| [view.edges[i].from.as_str(), view.edges[i].to.as_str()]));
    let mut crowd = Crowd::default();
    let mut labels: Vec<(String, FontId, Vec<Pos2>, usize)> = vec![];
    for (bi, b) in lay.bundles.iter().enumerate() {
        let pts: Vec<Pos2> = b.points.iter().map(|q| cam.xy(*q)).collect();
        let quiet = fnode.is_some() || focus.bones_only;
        crowd.lines.push((pts.clone(), 10_000 + bi));
        let (w, c) = if quiet { (1.0, EDGE_QUIET) } else { (2.0, EDGE) };
        polyline(p, pts.clone(), w, c, dash(&b.kind), &b.arrow, (9.0, 10.0, 4.0));
        if !quiet {
            labels.push((format!("{} 本", b.edges.len()), font(9.0 * s, false), pts, 10_000 + bi));
        }
    }
    for (i, (e, path)) in view.edges.iter().zip(&lay.paths).enumerate() {
        let strong = mine.contains(&i);
        if zone_of[e.from.as_str()] != zone_of[e.to.as_str()] && !strong {
            continue; // 枠をまたぐ線は、まとめた線で出している
        }
        let pts: Vec<Pos2> = path.points.iter().map(|q| cam.xy(*q)).collect();
        let kind = if e.blocked.is_empty() { e.status.as_str() } else { "stop" };
        let faded = (focus.bones_only && !e.skeleton) || (fnode.is_some() && !strong);
        let color = if faded { EDGE_QUIET } else if strong { BONE } else { EDGE };
        crowd.lines.push((pts.clone(), 20_000 + i));
        polyline(p, pts.clone(), if strong { 2.0 } else { 1.0 }, color, dash(kind), "last", (8.0, 9.0, 3.0));
        if !e.label.is_empty() && (strong || focus.hover_edge == Some(i)) {
            labels.push((e.label.clone(), font(9.0 * s, false), pts, 20_000 + i)); // 名札は、その線を見ているときだけ
        }
    }
    // 部品: 白いカードに 1px の灰色。状態は「丸 + 文字」（色だけに頼らない）
    let sk = through(&view.nodes);
    let w = NODE_W as f32 * 72.0 * s;
    for (ni, n) in view.nodes.iter().enumerate() {
        let Some(&c) = lay.pos.get(&n.id) else { continue };
        let r = node_rect(cam, c);
        crowd.boxes.push((r, ni));
        let kind = if n.blocked.is_empty() { n.status.as_str() } else { "stop" };
        let sel = focus.selected == Some(n.id.as_str());
        // 部品を選んでいるときは、つながっていない部品も薄くする（Ilograph の「関係ないものを隠す」に倣う）
        let faded = (focus.bones_only && !n.skeleton) || (fnode.is_some() && !near.contains(&n.id.as_str()));
        let (pad, sf, tf) = ((12.0 * s).max(6.0), font(9.0 * s, false), font(10.0 * s, true));
        // 四角が低くて 2 行（名前と説明）が入らないときは、名前だけを真ん中に出す（説明と状態の文字は右の欄で読む）
        let two = r.height() >= (tf.size + sf.size) * 1.3 + 4.0;
        let title_y = if two { r.min.y + (15.0 * s).max(tf.size * 0.5 + 3.0) } else { r.center().y };
        let sub_y = r.max.y - (14.0 * s).max(sf.size * 0.5 + 3.0);
        if faded {
            rounded(p, r, R_NODE * s, Some(BG), Some((1.0, EDGE_QUIET)), &[]);
            p.text(pos2(r.min.x + pad, title_y), Align2::LEFT_CENTER, clip(p, &n.title, &tf, w - pad * 2.0 - 16.0 * s), tf, FAINT);
            if two {
                p.text(pos2(r.min.x + pad, sub_y), Align2::LEFT_CENTER, clip(p, &n.sub, &sf, w - pad * 2.0 - 30.0 * s), sf, FAINT);
            }
            continue;
        }
        if sk.next.as_deref() == Some(n.id.as_str()) {
            // 見どころは 1 か所だけ: 次に通す部品を紺の太い輪で囲む
            let g = 5.0 * s;
            rounded(p, r.expand(g), (R_NODE + 4.0) * s, None, Some((3.0, BONE)), &[]);
            p.text(pos2(r.max.x, r.min.y - g - 3.0 * s), Align2::RIGHT_BOTTOM, "次はここ", font(9.0 * s, true), BONE);
        }
        rounded(p, r.translate(vec2(0.0, 1.0)), R_NODE * s, Some(SHADOW), None, &[]); // 1px だけ下にずらした影
        let fill = match kind {
            "wip" => TINT_WIP,
            "stop" => TINT_STOP,
            "none" => TINT_NONE,
            _ => CARD,
        };
        let outline = if sel { (2.0, BONE) } else { (1.0, CARD_LINE) };
        rounded(p, r, R_NODE * s, Some(fill), Some(outline), if kind == "none" && !sel { &[4.0, 3.0] } else { &[] });
        // 本番は丸だけ（多数派なので文字を出すと題を削る）。1 行しか入らないときも丸だけ（色の地と丸で分かる）
        let word = if kind == "live" || !two { "" } else { status_short(kind) };
        let ww = if word.is_empty() { (10.0 * s).max(8.0) } else { measure(p, word, &sf) + 12.0 * s };
        let rad = (3.5 * s).max(2.5);
        let dy = title_y;
        p.circle_filled(pos2(r.max.x - pad - ww + rad, dy), rad, status_color(kind));
        p.text(pos2(r.max.x - pad, dy), Align2::RIGHT_CENTER, word, sf.clone(), DIM);
        let title = clip(p, &n.title, &tf, w - pad * 2.0 - ww - 6.0 * s);
        p.text(pos2(r.min.x + pad, dy), Align2::LEFT_CENTER, title, tf, TEXT);
        if let Some(work) = focus.working.and_then(|w| w.get(&n.id)) {
            // 今どこを作業しているか
            let text = if work.focused { "作業中（今のペイン）" } else { "作業中" };
            p.text(pos2(r.min.x + 14.0 * s, r.min.y - 4.0 * s), Align2::LEFT_BOTTOM, text, font(9.0 * s, true), BONE);
        }
        if two {
            let sub = clip(p, &n.sub, &sf, w - pad * 2.0 - 30.0 * s);
            p.text(pos2(r.min.x + pad, sub_y), Align2::LEFT_CENTER, sub, sf, FAINT);
            if changes.nodes.contains_key(&n.id) {
                // 前回から状態か待ちが変わった
                p.text(pos2(r.max.x - pad, sub_y), Align2::RIGHT_CENTER, "変化", font(8.5 * s, true), BONE);
            }
        }
        if let Some(step) = n.step {
            // 骨格の番号。切れ目なく本番の所までは塗り、その先は輪だけ
            let done = step <= sk.through;
            let nf = num_font(8.5 * s);
            let rad = (9.0 * s).max(nf.size * 0.7);
            p.circle(r.min, rad, if done { BONE } else { CARD }, Stroke::new(1.0_f32, BONE));
            p.text(r.min, Align2::CENTER_CENTER, step.to_string(), nf, if done { WHITE } else { BONE });
        }
    }
    // 止まっている所の札（黄色の丸 = 待ち。文字は理由だけ、枠の幅に収める）
    for (r, reason) in tags_later {
        let f = font(9.0 * s, false);
        let text = clip(p, reason, &f, r.width() - 34.0 * s);
        let tw = measure(p, &text, &f);
        let b = Rect::from_min_size(r.min + vec2(6.0 * s, 3.0 * s), vec2(tw + 22.0 * s, 18.0 * s));
        p.rect(b, 0.0, PANEL, Stroke::new(1.0_f32, LINE), StrokeKind::Middle);
        p.circle_filled(b.min + vec2(9.0 * s, 9.0 * s), 3.0 * s, status_color("stop"));
        p.text(b.min + vec2(16.0 * s, 9.0 * s), Align2::LEFT_CENTER, text, f, DIM);
    }
    for (text, f, pts, own) in labels {
        place_label(p, &mut crowd, &text, &f, &pts, own);
    }
}

/// 上の帯（題・操作の案内・何時の state か）
pub fn header(p: &Painter, area: Rect, title: &str, hint: &str, meta: &str) {
    let r = Rect::from_min_size(area.min, vec2(area.width(), HEAD_H));
    p.rect_filled(r, 0.0, PANEL);
    p.line_segment([pos2(r.min.x, r.max.y), pos2(r.max.x, r.max.y)], Stroke::new(1.0_f32, LINE));
    p.text(pos2(r.min.x + 14.0, r.center().y), Align2::LEFT_CENTER, title, font(12.0, true), TEXT);
    let m = p.text(pos2(r.max.x - 14.0, r.center().y), Align2::RIGHT_CENTER, meta, font(9.0, false), DIM);
    p.text(pos2(m.min.x - 24.0, r.center().y), Align2::RIGHT_CENTER, hint, font(9.0, false), FAINT);
}

/// 下の帯で押せるもの
pub enum StripHit {
    /// 出す骨格を次のプロジェクトへ（全体で、骨格が 2 つ以上あるとき）
    Owner,
    Node(String),
}

/// 下の帯。骨格の部品を番号順に並べる。切れ目なく本番の所までは濃く、その先は薄く。押すとその部品へ。
/// 返すのは押せる所
pub fn strip(p: &Painter, r: Rect, view: &View, selected: Option<&str>) -> Vec<(Rect, StripHit)> {
    p.rect_filled(r, 0.0, PANEL);
    p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0_f32, LINE));
    let mut hits = vec![];
    let bones = bones(&view.nodes);
    if bones.is_empty() {
        p.text(pos2(r.min.x + 16.0, r.min.y + 32.0), Align2::LEFT_CENTER,
               "この範囲にはウォーキングスケルトンが無い（計画書の ◆ の項目か、宣言の step で決まる）", font(9.0, false), FAINT);
        return hits;
    }
    let sk = through(&view.nodes); // 個数ではなく、端から切れ目なく本番の所まで
    let many = view.owners.len() > 1;
    let owner = view.owner.as_deref().unwrap_or("");
    let o = p.text(pos2(r.min.x + 16.0, r.min.y + 22.0), Align2::LEFT_CENTER, format!("{owner} の骨格{}", if many { " ›" } else { "" }),
                   font(9.0, true), if many { BONE } else { FAINT });
    if many {
        hits.push((o.expand(4.0), StripHit::Owner));
    }
    let value = if sk.next.is_none() { "全部本番".to_string() } else { format!("{} 番まで本番", sk.through) };
    let head = p.text(pos2(r.min.x + 16.0, r.min.y + 41.0), Align2::LEFT_CENTER, format!("{value} / 全 {}", bones.len()), font(10.0, true), TEXT);
    let x0 = head.max.x.max(r.min.x + 150.0) + 24.0;
    let f = font(9.0, false);
    let n = bones.len() as f32;
    let each = (r.max.x - 16.0 - x0 - 12.0 * (n - 1.0)) / n; // 1 つあたりに使える幅
    let mut x = x0;
    for (i, b) in bones.iter().enumerate() {
        let cw = (measure(p, &b.title, &f) + 42.0).min(each).max(36.0);
        let sel = selected == Some(b.id.as_str());
        let done = b.step.unwrap() <= sk.through;
        let chip = Rect::from_min_max(pos2(x, r.min.y + 18.0), pos2(x + cw, r.min.y + 46.0));
        pill(p, chip, if sel { BONE } else { PANEL }, if sel { BONE } else { LINE });
        let c = pos2(x + 15.0, r.min.y + 32.0);
        let (fill, ring) = match (sel, done) {
            (true, true) => (WHITE, WHITE),
            (true, false) => (PANEL, WHITE),
            (false, true) => (BONE, BONE),
            (false, false) => (PANEL, BONE),
        };
        p.circle(c, 8.0, fill, Stroke::new(1.0_f32, ring));
        let num = if done { if sel { BONE } else { WHITE } } else { BONE };
        p.text(c, Align2::CENTER_CENTER, b.step.unwrap().to_string(), num_font(8.0), num);
        let ink = if sel { WHITE } else if done { TEXT } else { FAINT };
        p.text(pos2(x + 29.0, c.y), Align2::LEFT_CENTER, clip(p, &b.title, &f, cw - 36.0), f.clone(), ink);
        if sk.next.as_deref() == Some(b.id.as_str()) {
            // 次に通す部品
            p.text(pos2(x + cw / 2.0, r.min.y + 9.0), Align2::CENTER_CENTER, "次はここ", font(8.0, true), BONE);
        }
        if i + 1 < bones.len() {
            p.text(pos2(x + cw + 6.0, c.y), Align2::CENTER_CENTER, "›", font(10.0, false), FAINT);
        }
        hits.push((chip, StripHit::Node(b.id.clone())));
        x += cw + 12.0;
    }
    hits
}

/// タブの帯で押せるもの
#[derive(Clone, Copy)]
pub enum TabHit {
    Switch(usize),
    Close(usize),
}

/// タブの帯。今のタブは白く浮かせ、ほかは紙の色に沈める。2 枚以上あれば × で閉じられる
pub fn tabbar(p: &Painter, r: Rect, scopes: &[String], cur: usize) -> Vec<(Rect, TabHit)> {
    p.rect_filled(r, 0.0, BG);
    let (f, fb) = (font(9.0, false), font(9.0, true));
    let closable = scopes.len() > 1;
    let mut hits = vec![];
    let mut x = r.min.x + 12.0;
    for (i, scope) in scopes.iter().enumerate() {
        let label = if scope.is_empty() { "全体" } else { scope.as_str() };
        let on = i == cur;
        let w = measure(p, label, if on { &fb } else { &f }) + if closable { 40.0 } else { 24.0 };
        let tab = Rect::from_min_max(pos2(x, r.min.y + 6.0), pos2(x + w, r.max.y));
        if on {
            p.rect(tab, 0.0, PANEL, Stroke::new(1.0_f32, LINE), StrokeKind::Inside);
            p.line_segment([pos2(tab.min.x + 1.0, tab.max.y - 0.5), pos2(tab.max.x - 1.0, tab.max.y - 0.5)], Stroke::new(1.0_f32, PANEL)); // 下の辺は消して、下の図とつながって見えるように
        }
        p.text(pos2(x + 12.0, tab.center().y), Align2::LEFT_CENTER, label, if on { fb.clone() } else { f.clone() }, if on { TEXT } else { DIM });
        hits.push((tab, TabHit::Switch(i)));
        if closable {
            let c = pos2(x + w - 14.0, tab.center().y);
            p.text(c, Align2::CENTER_CENTER, "×", font(10.0, false), FAINT);
            hits.push((Rect::from_center_size(c, vec2(18.0, 18.0)), TabHit::Close(i)));
        }
        x += w + 4.0;
    }
    p.text(pos2(x + 10.0, r.center().y + 3.0), Align2::LEFT_CENTER, "ほかのプロジェクトで prefix+o を押すと、タブが増える", f, FAINT);
    hits
}
