//! 並べ方。arch_view.py の layout をそのまま写したもの（座標を Python と突き合わせて確かめる。`--dump-layout`）。
//! 枠ごとに中を Graphviz（dot）で並べ、枠どうしは左から決まった狭い間隔で並べる。座標は点（左上原点）。

use crate::model::{Edge, Node, View};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub type Pt = (f64, f64);

pub const NODE_W: f64 = 2.5; // インチ（Graphviz の単位）。1 インチ = 72 点
pub const NODE_H: f64 = 0.78;
const ZONE_GAP: f64 = 64.0; // 枠どうしの間（点）。枠をまたぐ線はこのすき間を通る
const LANE: f64 = 10.0; // 同じすき間を通る線どうしのずらし幅（点）
pub const ZONE_HEAD: f64 = 56.0; // 枠の上の、名前を書く場所（点）
const ZONE_PAD: f64 = 18.0; // 枠の内側の余白（点）
const WORST: [&str; 4] = ["stop", "none", "wip", "live"]; // まとめた線の色は、中でいちばん悪い状態に合わせる

#[derive(Serialize, Clone, Debug)]
pub struct Path {
    pub points: Vec<Pt>,
    pub label_at: Option<Pt>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Frame {
    pub key: String,
    #[serde(rename = "box")]
    pub bx: [f64; 4],
}

#[derive(Serialize, Clone, Debug)]
pub struct Bundle {
    pub zones: (String, String),
    pub edges: Vec<usize>,
    pub kind: String,
    pub arrow: String,
    pub points: Vec<Pt>,
    pub label_at: Option<Pt>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Layout {
    pub pos: HashMap<String, Pt>,
    pub frames: Vec<Frame>,
    pub paths: Vec<Path>,
    pub size: Pt,
    pub bundles: Vec<Bundle>,
}

struct Inner {
    pos: HashMap<String, Pt>,
    frames: Vec<Frame>,
    paths: Vec<Path>,
    size: Pt,
}

fn dot_exe() -> PathBuf {
    if let Some(p) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&p) {
            let c = d.join("dot.exe");
            if c.is_file() {
                return c;
            }
        }
    }
    PathBuf::from(r"C:\Program Files\Graphviz\bin\dot.exe")
}

/// dot はコンソールのプログラムなので、CREATE_NO_WINDOW で呼ばないと呼ぶたびに窓が開いて入力を奪う（2026-10-06）
pub fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn run_dot(src: &str) -> Result<serde_json::Value, String> {
    let mut cmd = Command::new(dot_exe());
    hidden(cmd.arg("-Tjson").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()));
    let mut child = cmd.spawn().map_err(|e| format!("dot を起動できない: {e}"))?;
    child.stdin.take().unwrap().write_all(src.as_bytes()).map_err(|e| format!("dot に渡せない: {e}"))?;
    let out = child.wait_with_output().map_err(|e| format!("dot が終わらない: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().chars().take(300).collect());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("dot の出力が読めない: {e}"))
}

fn floats(v: &serde_json::Value) -> Vec<f64> {
    v.as_str().unwrap_or("").split(',').filter_map(|s| s.trim().parse().ok()).collect()
}

/// Graphviz の 3 次ベジェの制御点を、なめらかな折れ線にする。
fn bezier(pts: &[Pt], steps: usize) -> Vec<Pt> {
    let mut out: Vec<Pt> = pts.first().copied().into_iter().collect();
    let mut i = 0;
    while i + 3 < pts.len() {
        let (p0, p1, p2, p3) = (pts[i], pts[i + 1], pts[i + 2], pts[i + 3]);
        for s in 1..=steps {
            let t = s as f64 / steps as f64;
            let (a, b, c, d) = ((1.0 - t).powi(3), 3.0 * t * (1.0 - t).powi(2), 3.0 * t * t * (1.0 - t), t.powi(3));
            out.push((a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1));
        }
        i += 3;
    }
    out
}

/// Graphviz の線の形（pos）を、なめらかな折れ線にする。
fn spline(pos: &str, flip: &dyn Fn(f64, f64) -> Pt) -> Vec<Pt> {
    let (mut pts, mut end) = (vec![], None);
    for tok in pos.split_whitespace() {
        let parts: Vec<&str> = tok.split(',').collect();
        let num = |i: usize| parts.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        if parts[0] == "e" {
            end = Some(flip(num(1), num(2)));
        } else if parts[0] != "s" {
            pts.push(flip(num(0), num(1)));
        }
    }
    let mut out = bezier(&pts, 12);
    out.extend(end);
    out
}

/// 1 つの枠の中だけを Graphviz に並べさせる。座標は枠の中の左上を原点にした点。
fn zone_layout(zi: usize, members: &[&Node], inner_edges: &[&Edge], names: &HashMap<String, String>, rankdir: &str) -> Result<Inner, String> {
    let mut lines = vec![
        "digraph G {".to_string(),
        format!("rankdir={rankdir}; nodesep=0.35; ranksep=0.9; splines=spline;"),
        r#"graph [fontname="Yu Gothic UI", fontsize=13, margin=0];"#.to_string(),
        format!(r#"node [shape=box, fixedsize=true, width={NODE_W}, height={NODE_H}, label=""];"#),
        r#"edge [fontname="Yu Gothic UI", fontsize=10];"#.to_string(),
    ];
    // 同じ理由で止まっている部品は、点線の枠にまとめる（Graphviz が寄せて並べる）
    let mut groups: Vec<(&str, Vec<&Node>)> = vec![];
    for n in members.iter().copied().filter(|n| !n.blocked.is_empty()) {
        match groups.iter_mut().find(|g| g.0 == n.blocked) {
            Some(g) => g.1.push(n),
            None => groups.push((n.blocked.as_str(), vec![n])),
        }
    }
    let name = |n: &Node| format!("{};", names[&n.id]);
    for (gi, (_, ns)) in groups.iter().enumerate() {
        let body: Vec<String> = ns.iter().map(|n| name(n)).collect();
        lines.push(format!(r#"subgraph cluster_b{gi} {{ label=" "; fontsize=18; margin=14; {}}}"#, body.join(" ")));
    }
    lines.push(members.iter().filter(|n| n.blocked.is_empty()).map(|n| name(n)).collect::<Vec<_>>().join(" "));
    for e in inner_edges {
        // 本物の線を先に書く（結果の edges の先頭から本物として読む）
        let label = if e.label.is_empty() { String::new() } else { format!(r#" [label="{}"]"#, e.label) };
        lines.push(format!("{} -> {}{};", names[&e.from], names[&e.to], label));
    }
    // 枠の中の線でつながっていない部品は、1 列に縦長に並ばないよう、見えない線で数列の格子にする
    let linked: HashSet<&str> = inner_edges.iter().flat_map(|e| [e.from.as_str(), e.to.as_str()]).collect();
    let free: Vec<&&Node> = members.iter().filter(|n| !linked.contains(n.id.as_str())).collect();
    if free.len() > 3 {
        let f = free.len() as f64;
        let rows = (f / (f / 2.0).sqrt().ceil()).ceil() as usize;
        for i in 0..free.len().saturating_sub(rows) {
            lines.push(format!("{} -> {} [style=invis];", names[&free[i].id], names[&free[i + rows].id]));
        }
    }
    lines.push("}".to_string());

    let g = run_dot(&lines.join("\n"))?;
    let bb = floats(&g["bb"]);
    let (x0, y0, x1, y1) = (bb[0], bb[1], bb[2], bb[3]);
    let flip = move |x: f64, y: f64| (x - x0, y1 - y);
    let by_name: HashMap<&str, &str> = names.iter().map(|(k, v)| (v.as_str(), k.as_str())).collect();
    let (mut pos, mut frames) = (HashMap::new(), vec![]);
    for o in g["objects"].as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
        let oname = o["name"].as_str().unwrap_or("");
        if oname.starts_with("cluster_b") && o.get("bb").is_some() {
            let b = floats(&o["bb"]);
            frames.push(Frame {
                key: format!("z{zi}_{}", &oname["cluster_".len()..]),
                bx: [b[0] - x0, y1 - b[3], b[2] - x0, y1 - b[1]],
            });
        } else if let Some(id) = by_name.get(oname) {
            let p = floats(&o["pos"]);
            pos.insert(id.to_string(), flip(p[0], p[1]));
        }
    }
    let paths = g["edges"]
        .as_array()
        .map(|v| v.as_slice())
        .unwrap_or(&[])
        .iter()
        .take(inner_edges.len())
        .map(|e| Path {
            points: spline(e["pos"].as_str().unwrap_or(""), &flip),
            label_at: e.get("lp").map(|lp| {
                let p = floats(lp);
                flip(p[0], p[1])
            }),
        })
        .collect();
    Ok(Inner { pos, frames, paths, size: (x1 - x0, y1 - y0) })
}

fn dist(a: Pt, b: Pt) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// 線分 p-q が、skip 以外の枠のどれかを横切るか。
fn hits(p: Pt, q: Pt, rects: &[[f64; 4]], skip: &[usize]) -> bool {
    for (i, r) in rects.iter().enumerate() {
        if skip.contains(&i) {
            continue;
        }
        let [x1, y1, x2, y2] = *r;
        if p.0 == q.0 {
            // 縦の線分
            if x1 < p.0 && p.0 < x2 && p.1.min(q.1) < y2 && p.1.max(q.1) > y1 {
                return true;
            }
        } else if p.1 == q.1 {
            // 横の線分
            if y1 < p.1 && p.1 < y2 && p.0.min(q.0) < x2 && p.0.max(q.0) > x1 {
                return true;
            }
        }
    }
    false
}

/// 折れ線の角を丸める（角ごとに短い曲線を入れる）。
fn rounded_path(pts: &[Pt], r: f64) -> Vec<Pt> {
    let mut out = vec![pts[0]];
    for w in pts.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        let (da, dc) = (dist(a, b), dist(b, c));
        let k = r.min(da / 2.0).min(dc / 2.0);
        if k < 1.0 {
            out.push(b);
            continue;
        }
        let p = (b.0 + (a.0 - b.0) * k / da, b.1 + (a.1 - b.1) * k / da);
        let q = (b.0 + (c.0 - b.0) * k / dc, b.1 + (c.1 - b.1) * k / dc);
        out.push(p);
        for t in [0.25, 0.5, 0.75] {
            out.push((
                (1.0 - t) * (1.0 - t) * p.0 + 2.0 * t * (1.0 - t) * b.0 + t * t * q.0,
                (1.0 - t) * (1.0 - t) * p.1 + 2.0 * t * (1.0 - t) * b.1 + t * t * q.1,
            ));
        }
        out.push(q);
    }
    out.push(*pts.last().unwrap());
    out
}

#[derive(Hash, PartialEq, Eq)]
enum LaneKey {
    V(i64),
    H(u64),
}

/// 枠をまたぐ線を、枠と枠のあいだの通り道だけを通して引く（別の枠の上を通らない）。
/// 通り道: 枠の左右のすき間（縦）と、全体の上・下のすき間（横）。同じ通り道の線は少しずつずらす（lanes）。
/// w は端の点から辺までの幅（部品なら部品の半分、枠の辺から出すなら 0）。
#[allow(clippy::too_many_arguments)]
fn route_cross(a: Pt, b: Pt, za: usize, zb: usize, rects: &[[f64; 4]], total: Pt, lanes: &mut HashMap<LaneKey, i64>, w: f64) -> Path {
    let (ra, rb) = (rects[za], rects[zb]);
    let half = ZONE_GAP / 2.0;
    let right = b.0 >= a.0;
    let same_column = ra[0] < rb[2] && rb[0] < ra[2]; // 上下に積んだ枠どうし: 右のすき間を通る
    let (ex, nx) = if same_column {
        let x = ra[2].max(rb[2]) + half;
        (x, x)
    } else {
        (if right { ra[2] + half } else { ra[0] - half }, if right { rb[0] - half } else { rb[2] + half })
    };
    let start = (if ex > a.0 { a.0 + w } else { a.0 - w }, a.1);
    let end = (if nx < b.0 { b.0 - w } else { b.0 + w }, b.1);
    let (top, bottom) = (-half, total.1 + half);
    let mut candidates: Vec<Vec<Pt>> = vec![];
    if ex == nx {
        candidates.push(vec![start, (ex, a.1), (ex, b.1), end]);
    }
    candidates.push(vec![start, (ex, a.1), (ex, b.1), (nx, b.1), end]); // 縦に動いてから、相手の高さで横へ
    candidates.push(vec![start, (ex, a.1), (nx, a.1), (nx, b.1), end]); // 横に動いてから縦へ
    for y in [top, bottom] {
        candidates.push(vec![start, (ex, a.1), (ex, y), (nx, y), (nx, b.1), end]); // 全体の上か下を回る
    }
    let skip = [za, zb];
    let ok = |path: &Vec<Pt>| (1..path.len() - 2).all(|i| !hits(path[i], path[i + 1], rects, &skip));
    let length = |path: &Vec<Pt>| path.windows(2).map(|s| dist(s[0], s[1])).sum::<f64>();
    let good: Vec<&Vec<Pt>> = candidates.iter().filter(|c| ok(c)).collect();
    let pool: Vec<&Vec<Pt>> = if good.is_empty() { candidates[candidates.len() - 2..].iter().collect() } else { good };
    let mut best = pool[0];
    for c in &pool[1..] {
        if length(c) < length(best) {
            best = c;
        }
    }
    let mut pts: Vec<Pt> = vec![];
    for (i, p) in best.iter().enumerate() {
        if i == 0 || *p != best[i - 1] {
            pts.push(*p);
        }
    }
    // 選んだ通り道にだけ、ずらし幅を割り当てる（同じすき間を通る線が重ならないように）
    let mut lane = |key: LaneKey| {
        let n = lanes.entry(key).and_modify(|n| *n += 1).or_insert(0);
        let n = *n;
        ((n + 1) / 2) as f64 * if n % 2 == 1 { LANE } else { -LANE }
    };
    let dex = lane(LaneKey::V(ex.round_ties_even() as i64));
    let dnx = if nx == ex { dex } else { lane(LaneKey::V(nx.round_ties_even() as i64)) };
    let inner = &pts[1..pts.len() - 1];
    let dtop = if inner.iter().any(|p| p.1 == top) { lane(LaneKey::H(top.to_bits())) } else { 0.0 };
    let dbottom = if inner.iter().any(|p| p.1 == bottom) { lane(LaneKey::H(bottom.to_bits())) } else { 0.0 };
    let dx = |x: f64| if x == ex { dex } else if x == nx { dnx } else { 0.0 };
    let dy = |y: f64| if y == top { dtop } else if y == bottom { dbottom } else { 0.0 };
    let n = pts.len();
    let pts: Vec<Pt> = pts
        .iter()
        .enumerate()
        .map(|(i, p)| if i == 0 || i == n - 1 { *p } else { (p.0 + dx(p.0), p.1 + dy(p.1)) })
        .collect();
    // いちばん長い区間の真ん中に名札
    let mut mid = (pts[0], pts[1]);
    for s in pts.windows(2) {
        if dist(s[0], s[1]) > dist(mid.0, mid.1) {
            mid = (s[0], s[1]);
        }
    }
    Path { points: rounded_path(&pts, 14.0), label_at: Some(((mid.0.0 + mid.1.0) / 2.0, (mid.0.1 + mid.1.1) / 2.0 - 7.0)) }
}

/// 枠を左から順に置く。小さい枠は、いちばん高い枠の高さに収まるあいだ同じ列に縦に積む。
fn arrange(sizes: &[Pt]) -> (Vec<Pt>, Pt) {
    let tallest = sizes.iter().map(|s| s.1).fold(f64::MIN, f64::max);
    let (mut spots, mut x, mut y, mut col_w) = (vec![], 0.0, 0.0, 0.0f64);
    for &(w, h) in sizes {
        if y > 0.0 && y + h > tallest + 1.0 {
            (x, y, col_w) = (x + col_w + ZONE_GAP, 0.0, 0.0);
        }
        spots.push((x, y));
        (y, col_w) = (y + h + ZONE_GAP, col_w.max(w));
    }
    (spots, (x + col_w, tallest))
}

/// 枠を左から並べる順。外部 → 社内の仕組み → プロジェクト → 外部サービス。
/// プロジェクトどうしは、呼ぶ側を左・呼ばれる側（土台）を右に置く（線が左から右へ流れる）。
fn zone_order(view: &View) -> Vec<String> {
    let zone_of: HashMap<&str, &str> = view.nodes.iter().map(|n| (n.id.as_str(), n.zone.as_str())).collect();
    let mut used: Vec<_> = view.zones.iter().filter(|z| view.nodes.iter().any(|n| n.zone == z.id)).collect();
    let called = |z: &str| view.edges.iter().filter(|e| zone_of[e.to.as_str()] == z && zone_of[e.from.as_str()] != z).count();
    let rank = |k: &str| match k {
        "external" => 0,
        "company" => 1,
        "project" => 2,
        "cloud" => 3,
        _ => 2,
    };
    used.sort_by_key(|z| (rank(&z.kind), called(&z.id)));
    used.iter().map(|z| z.id.clone()).collect()
}

/// 全体が窓の形（aspect = 幅 / 高さ）にいちばん近くなるよう、枠の中を横に流すか縦に流すかを選ぶ。
pub fn layout(view: &View, aspect: f64) -> Result<Layout, String> {
    let names: HashMap<String, String> = view.nodes.iter().enumerate().map(|(i, n)| (n.id.clone(), format!("n{i}"))).collect();
    let zone_of: HashMap<&str, &str> = view.nodes.iter().map(|n| (n.id.as_str(), n.zone.as_str())).collect();
    let zi_of: HashMap<&str, usize> = view.zones.iter().enumerate().map(|(i, z)| (z.id.as_str(), i)).collect();
    let order = zone_order(view);
    let idx: HashMap<&str, Vec<usize>> = order
        .iter()
        .map(|zid| {
            let ids = (0..view.edges.len())
                .filter(|&i| zone_of[view.edges[i].from.as_str()] == zid && zone_of[view.edges[i].to.as_str()] == zid)
                .collect();
            (zid.as_str(), ids)
        })
        .collect();
    // 枠ごと・向きごとの Graphviz を同時に走らせる
    let options: Vec<[Inner; 2]> = std::thread::scope(|s| {
        let jobs: Vec<_> = order
            .iter()
            .map(|zid| {
                let members: Vec<&Node> = view.nodes.iter().filter(|n| &n.zone == zid).collect();
                let inner_edges: Vec<&Edge> = idx[zid.as_str()].iter().map(|&i| &view.edges[i]).collect();
                let zi = zi_of[zid.as_str()];
                ["LR", "TB"].map(|d| {
                    let (members, inner_edges, names) = (members.clone(), inner_edges.clone(), &names);
                    s.spawn(move || zone_layout(zi, &members, &inner_edges, names, d))
                })
            })
            .collect();
        jobs.into_iter()
            .map(|[a, b]| Ok([a.join().map_err(|_| "並べる糸が落ちた".to_string())??, b.join().map_err(|_| "並べる糸が落ちた".to_string())??]))
            .collect::<Result<Vec<_>, String>>()
    })?;
    let bx = |inner: &Inner| (inner.size.0 + ZONE_PAD * 2.0, inner.size.1 + ZONE_HEAD + ZONE_PAD);
    let n = order.len();
    let mut best: Option<(f64, Vec<usize>, Vec<Pt>, Pt)> = None;
    for m in 0..(1usize << n) {
        // itertools.product((0, 1), repeat=n) と同じ順（先頭の枠がいちばん上の桁）
        let pick: Vec<usize> = (0..n).map(|i| (m >> (n - 1 - i)) & 1).collect();
        let sizes: Vec<Pt> = pick.iter().enumerate().map(|(i, &p)| bx(&options[i][p])).collect();
        let (spots, (tw, th)) = arrange(&sizes);
        let score = ((tw / th) / aspect).ln().abs() + 0.15 * (tw * th).ln(); // 窓の形に近く、なるべく小さく
        if best.as_ref().is_none_or(|b| score < b.0) {
            best = Some((score, pick, spots, (tw, th)));
        }
    }
    let (_, pick, spots, total) = best.ok_or("並べる枠が無い")?;
    let mut pos = HashMap::new();
    let mut frames: Vec<Frame> = vec![];
    let mut paths: Vec<Option<Path>> = vec![None; view.edges.len()];
    for (i, zid) in order.iter().enumerate() {
        let inner = &options[i][pick[i]];
        let (x, y) = spots[i];
        let zi = zi_of[zid.as_str()];
        let (ox, oy) = (x + ZONE_PAD, y + ZONE_HEAD);
        let mv = |p: Pt| (p.0 + ox, p.1 + oy);
        for (nid, p) in &inner.pos {
            pos.insert(nid.clone(), mv(*p));
        }
        for f in &inner.frames {
            let [a, b, c, d] = f.bx;
            let (p, q) = (mv((a, b)), mv((c, d)));
            frames.push(Frame { key: f.key.clone(), bx: [p.0, p.1, q.0, q.1] });
        }
        for (ei, p) in idx[zid.as_str()].iter().zip(&inner.paths) {
            paths[*ei] = Some(Path { points: p.points.iter().map(|q| mv(*q)).collect(), label_at: p.label_at.map(mv) });
        }
        let (w, h) = bx(inner);
        frames.insert(0, Frame { key: format!("z{zi}"), bx: [x, y, x + w, y + h] });
    }
    let zone_frames: Vec<&Frame> = frames.iter().filter(|f| !f.key.contains('_')).collect();
    let rects: Vec<[f64; 4]> = zone_frames.iter().map(|f| f.bx).collect();
    let zone_rect: HashMap<&str, usize> = order
        .iter()
        .map(|zid| (zid.as_str(), zone_frames.iter().position(|f| f.key == format!("z{}", zi_of[zid.as_str()])).unwrap()))
        .collect();
    let mut lanes = HashMap::new();
    for (i, e) in view.edges.iter().enumerate() {
        if paths[i].is_none() {
            let (za, zb) = (zone_rect[zone_of[e.from.as_str()]], zone_rect[zone_of[e.to.as_str()]]);
            paths[i] = Some(route_cross(pos[&e.from], pos[&e.to], za, zb, &rects, total, &mut lanes, NODE_W * 72.0 / 2.0));
        }
    }
    let bundles = bundles(view, &pos, &zone_of, &zone_rect, &rects, total);
    Ok(Layout { pos, frames, paths: paths.into_iter().map(|p| p.unwrap()).collect(), size: total, bundles })
}

/// 枠をまたぐ線を、枠の組ごとに 1 本にまとめる。端は枠の、相手に向いた辺（高さは関わる部品の平均）。
fn bundles(view: &View, pos: &HashMap<String, Pt>, zone_of: &HashMap<&str, &str>, zone_rect: &HashMap<&str, usize>, rects: &[[f64; 4]], total: Pt) -> Vec<Bundle> {
    let mut groups: Vec<((String, String), Vec<usize>)> = vec![];
    for (i, e) in view.edges.iter().enumerate() {
        let (za, zb) = (zone_of[e.from.as_str()], zone_of[e.to.as_str()]);
        if za != zb {
            let key = if za <= zb { (za.to_string(), zb.to_string()) } else { (zb.to_string(), za.to_string()) };
            match groups.iter_mut().find(|g| g.0 == key) {
                Some(g) => g.1.push(i),
                None => groups.push((key, vec![i])),
            }
        }
    }
    let anchor = |zone: &str, rect: [f64; 4], other: [f64; 4], idx: &[usize]| -> Pt {
        let ys: Vec<f64> = idx
            .iter()
            .flat_map(|&i| [&view.edges[i].from, &view.edges[i].to])
            .filter(|x| zone_of[x.as_str()] == zone)
            .map(|x| pos[x].1)
            .collect();
        let y = (ys.iter().sum::<f64>() / ys.len() as f64).max(rect[1] + 30.0).min(rect[3] - 20.0);
        if rect[0] < other[2] && other[0] < rect[2] {
            return (rect[2], y); // 上下に積んだ枠どうし: 右の辺から
        }
        if other[0] >= rect[2] { (rect[2], y) } else { (rect[0], y) }
    };
    let mut ends: Vec<[Pt; 2]> = groups
        .iter()
        .map(|((za, zb), idx)| {
            let (ra, rb) = (rects[zone_rect[za.as_str()]], rects[zone_rect[zb.as_str()]]);
            [anchor(za, ra, rb, idx), anchor(zb, rb, ra, idx)]
        })
        .collect();
    // 同じ枠の同じ辺から出る端どうしは、高さの順を保ったまま 24 点以上あける（線が重ならないように）
    let mut sides: Vec<((String, f64), Vec<(f64, usize, usize)>)> = vec![];
    for (gi, pair) in ends.iter().enumerate() {
        for k in 0..2 {
            let zone = if k == 0 { &groups[gi].0.0 } else { &groups[gi].0.1 };
            let side = (zone.clone(), pair[k].0);
            let item = (pair[k].1, gi, k);
            match sides.iter_mut().find(|s| s.0 == side) {
                Some(s) => s.1.push(item),
                None => sides.push((side, vec![item])),
            }
        }
    }
    for ((zone, _), items) in &mut sides {
        let rect = rects[zone_rect[zone.as_str()]];
        // Python の items.sort() と同じ順（高さ → 枠の組の名前 → 端の番号）
        items.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| groups[a.1].0.cmp(&groups[b.1].0)).then(a.2.cmp(&b.2)));
        let mut ys: Vec<f64> = items.iter().map(|it| it.0).collect();
        for i in 1..ys.len() {
            ys[i] = ys[i].max(ys[i - 1] + 24.0);
        }
        let shift = (ys.last().unwrap() - (rect[3] - 16.0)).max(0.0); // 下にはみ出たぶん、全体を上へ
        for (&(_, gi, k), y) in items.iter().zip(ys) {
            ends[gi][k].1 = (rect[1] + 24.0).max(y - shift);
        }
    }
    let mut lanes = HashMap::new();
    groups
        .iter()
        .zip(ends)
        .map(|(((za, zb), idx), [a, b])| {
            let path = route_cross(a, b, zone_rect[za.as_str()], zone_rect[zb.as_str()], rects, total, &mut lanes, 0.0);
            let kind = idx
                .iter()
                .map(|&i| if view.edges[i].blocked.is_empty() { view.edges[i].status.as_str() } else { "stop" })
                .min_by_key(|k| WORST.iter().position(|w| w == k).unwrap_or(WORST.len()))
                .unwrap()
                .to_string();
            let forward = idx.iter().any(|&i| zone_of[view.edges[i].from.as_str()] == za);
            let backward = idx.iter().any(|&i| zone_of[view.edges[i].from.as_str()] == zb);
            let arrow = if forward && backward { "both" } else if forward { "last" } else { "first" };
            Bundle { zones: (za.clone(), zb.clone()), edges: idx.clone(), kind, arrow: arrow.to_string(), points: path.points, label_at: path.label_at }
        })
        .collect()
}
