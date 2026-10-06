//! 右の欄。部品を押したときの中身と、まとめた線の中身。arch_view.py の show_side・show_bundle の写し。
//! 中身は arch.py の detail が決める（ここは並べるだけ）。全部を同じ重さで並べない・空欄にしない・書き換えさせない。

use crate::draw::{BONE, DIM, FAINT, TEXT, status_color};
use crate::layout::Layout;
use crate::model::{Node, State, View, through};
use egui::text::LayoutJob;
use egui::{Color32, CursorIcon, FontFamily, FontId, Label, RichText, Sense, Stroke, TextFormat, Ui};
use std::collections::HashMap;

pub const WIDTH: f32 = 340.0;

/// 欄の中で押されたもの
pub enum Action {
    /// 図の中のその部品を選ぶ
    Select(String),
    /// 既定のブラウザで開く
    Open(String),
    /// AI に渡す文をコピーする
    Copy(String),
}

/// 畳んだ見出しの開き具合（部品ごとに覚えておく）
#[derive(Default)]
pub struct Side {
    opened: HashMap<(String, String), bool>,
}

pub fn status_text(kind: &str) -> &'static str {
    match kind {
        "live" => "本番で動いている",
        "wip" => "作りかけ",
        "stop" => "待ち",
        _ => "まだ無い",
    }
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

/// 色と太さの違う文字を 1 つの段落にする（折り返す）
fn para(ui: &mut Ui, parts: &[(&str, FontId, Color32)]) {
    let mut job = LayoutJob::default();
    for (text, font, color) in parts {
        job.append(text, 0.0, TextFormat { font_id: font.clone(), color: *color, ..Default::default() });
    }
    job.wrap.max_width = ui.available_width();
    ui.label(job);
}

fn text(ui: &mut Ui, s: &str, size: f32, color: Color32) {
    para(ui, &[(s, plain(size), color)]);
}

fn label(ui: &mut Ui, s: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(s).font(bold(9.0)).color(FAINT));
}

/// 押せる文字（紺。乗せると下線）
fn link(ui: &mut Ui, s: &str, act: Action, out: &mut Option<Action>) {
    let r = ui.add(Label::new(RichText::new(s).font(plain(10.0)).color(BONE)).sense(Sense::click()));
    let r = r.on_hover_cursor(CursorIcon::PointingHand);
    if r.hovered() {
        let y = r.rect.bottom() - 1.0;
        ui.painter().line_segment([egui::pos2(r.rect.left(), y), egui::pos2(r.rect.right(), y)], Stroke::new(1.0_f32, BONE));
    }
    if r.clicked() {
        *out = Some(act);
    }
}

fn gh_url(github: &str, rest: &str) -> Option<String> {
    (!github.is_empty()).then(|| format!("https://github.com/{github}/{rest}"))
}

/// 外へのリンク。行き先が無ければ文字だけ
fn link_or_text(ui: &mut Ui, s: &str, url: Option<String>, out: &mut Option<Action>) {
    match url {
        Some(u) => link(ui, s, Action::Open(u), out),
        None => text(ui, s, 10.0, TEXT),
    }
}

impl Side {
    /// 畳める見出し。押すと開く・閉じる
    fn section(&mut self, ui: &mut Ui, node: &str, title: &str, count: &str, default: bool, fill: impl FnOnce(&mut Ui)) {
        let key = (node.to_string(), title.to_string());
        let opened = *self.opened.get(&key).unwrap_or(&default);
        // 開いている印は ∨（Python 版の ⌄ は BIZ UDPゴシックに無く、□ に化けた）
        ui.add_space(10.0);
        let mut job = LayoutJob::default();
        let head = TextFormat { font_id: bold(10.0), color: TEXT, ..Default::default() };
        job.append(&format!("{} {title}", if opened { "∨" } else { "›" }), 0.0, head);
        job.append(count, 0.0, TextFormat { font_id: plain(9.0), color: FAINT, ..Default::default() });
        let r = ui.add(Label::new(job).sense(Sense::click())).on_hover_cursor(CursorIcon::PointingHand);
        if r.clicked() {
            self.opened.insert(key, !opened);
        }
        if opened {
            fill(ui);
        }
    }

    /// 部品の詳しい中身。上に要約（なぜ・次の一手・主な PR・AI に渡す文）、その下に畳む材料、最後に外へのリンク
    pub fn node(&mut self, ui: &mut Ui, st: &State, view: &View, n: &Node, work: Option<&crate::model::Work>) -> Option<Action> {
        let mut out = None;
        let d = &n.detail;
        let names: HashMap<&str, &str> = view.nodes.iter().map(|x| (x.id.as_str(), x.title.as_str())).collect();
        let gh = |project: &str| st.projects.get(project).map(|p| p.github.clone()).unwrap_or_default();
        let sk = through(&view.nodes);

        // -- 要約（スクロールせずに見える範囲） --
        ui.label(RichText::new(&n.title).font(bold(12.0)).color(TEXT));
        ui.add_space(6.0);
        let kind = if n.blocked.is_empty() { n.status.as_str() } else { "stop" };
        para(ui, &[("● ", plain(10.0), status_color(kind)), (status_text(kind), plain(10.0), TEXT)]);
        let mut rows: Vec<(&str, String)> = vec![("説明", n.sub.clone())];
        if let Some(w) = work {
            rows.push(("作業中", w.branch.clone()));
        }
        let changes = st.arch.as_ref().map(|a| &a.changes);
        if let Some(c) = changes.and_then(|ch| ch.nodes.get(&n.id)) {
            let old = c.from.as_ref().map_or("新しく宣言", |w| status_text(&w.status));
            let why = if c.to.blocked.is_empty() { String::new() } else { format!("（{}）", c.to.blocked) };
            rows.push(("前回から", format!("{old} → {}{why}", status_text(&c.to.status))));
        }
        if let Some(step) = n.step {
            let next = if sk.next.as_deref() == Some(n.id.as_str()) { "（次はここ）" } else { "" };
            rows.push(("骨格", format!("{step} 番目{next}")));
        }
        if !n.refs.is_empty() {
            let refs: Vec<String> = n
                .refs
                .iter()
                .map(|r| r.id.split_once(':').map_or(r.id.as_str(), |x| x.1).to_string() + if r.critical { " ◆" } else { "" })
                .collect();
            rows.push(("計画書", refs.join("、")));
        }
        egui::Grid::new("rows").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            for (key, value) in rows.iter().filter(|r| !r.1.is_empty()) {
                ui.label(RichText::new(*key).font(plain(9.0)).color(FAINT));
                text(ui, value, 10.0, TEXT);
                ui.end_row();
            }
        });
        label(ui, "なぜ");
        text(ui, &d.reason, 10.0, if n.blocked.is_empty() { TEXT } else { status_color("stop") });
        label(ui, "次の一手");
        para(ui, &[(&d.next, bold(10.0), TEXT)]);
        let planned = !n.refs.is_empty(); // 計画書の項目を持たない部品（外部・他チームなど）には、やること・PR・メモが無い
        if planned {
            label(ui, "PR");
            match &d.main_pr {
                Some(pr) => {
                    link_or_text(ui, &format!("#{} {}", pr.number, pr.title), gh_url(&gh(&pr.project), &format!("pull/{}", pr.number)), &mut out);
                    let why = if pr.why.is_empty() { pr.state.to_lowercase() } else { pr.why.clone() };
                    text(ui, &format!("　{why}"), 9.0, FAINT);
                }
                None => text(ui, "まだ無い（PR を出すと、題の計画書の番号かカードの prs でつながる）", 9.0, FAINT),
            }
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let b = egui::Button::new(RichText::new("AI に渡す文をコピー").font(bold(9.0)).color(BONE))
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1.0_f32, crate::draw::LINE));
            if ui.add(b).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                out = Some(Action::Copy(d.ai.clone()));
            }
            text(ui, &format!("{} 時点", st.generated_at.get(11..16).unwrap_or("")), 9.0, FAINT);
        });

        // -- 畳む材料 --
        if planned {
            let count = if d.items.is_empty() { String::new() } else { format!("（{}）", d.progress) };
            self.section(ui, &n.id, "やること", &count, !n.blocked.is_empty(), |ui| {
                if d.items.is_empty() {
                    text(ui, "カードにやることが無い（作業を始めたらカードの items に足す）", 9.0, FAINT);
                }
                for i in &d.items {
                    let mark = if i.done { "■ " } else { "□ " };
                    text(ui, &format!("{mark}{}", i.text), 10.0, if i.done { FAINT } else { TEXT });
                }
            });
            self.section(ui, &n.id, "PR", &format!("（{} 本）", d.prs.len()), false, |ui| {
                if d.prs.is_empty() {
                    text(ui, "つながっている PR はまだ無い", 9.0, FAINT);
                }
                let mut prs: Vec<_> = d.prs.iter().collect();
                prs.sort_by_key(|p| std::cmp::Reverse(p.number));
                for p in prs {
                    ui.horizontal_wrapped(|ui| {
                        link_or_text(ui, &format!("#{}", p.number), gh_url(&gh(&p.project), &format!("pull/{}", p.number)), &mut out);
                        let why = if p.state == "OPEN" && !p.why.is_empty() { format!("・{}", p.why) } else { String::new() };
                        let color = if p.state == "OPEN" { TEXT } else { FAINT };
                        text(ui, &format!("{}{why}　{}", p.state.to_lowercase(), p.title), 10.0, color);
                    });
                }
            });
        }
        let mine: Vec<_> = view.edges.iter().filter(|e| e.from == n.id || e.to == n.id).collect();
        let checked: Vec<_> = mine.iter().filter(|e| !e.note.is_empty()).collect();
        if planned || !checked.is_empty() || !n.switch_note.is_empty() {
            let count = format!("（{}）", checked.len() + usize::from(!n.switch_note.is_empty()));
            self.section(ui, &n.id, "確かめた結果", &count, false, |ui| {
                if !n.switch_note.is_empty() {
                    let ok = !n.switch_note.contains("が無い");
                    let (mark, color) = if ok { ("✓ ", status_color("live")) } else { ("✗ ", crate::draw::ERROR) };
                    text(ui, &format!("{mark}本番のスイッチ"), 10.0, color);
                    text(ui, &format!("   {}", n.switch_note), 9.0, FAINT);
                }
                if checked.is_empty() && n.switch_note.is_empty() {
                    text(ui, "コードで確かめている線は無い（宣言の線に check を足すと、origin/main で確かめる）", 9.0, FAINT);
                }
                for e in &checked {
                    let ok = e.note.starts_with("コードで確かめた") && !e.note.contains("が無い");
                    let (mark, color) = if ok { ("✓ ", status_color("live")) } else { ("✗ ", crate::draw::ERROR) };
                    let from = names.get(e.from.as_str()).unwrap_or(&"");
                    let to = names.get(e.to.as_str()).unwrap_or(&"");
                    text(ui, &format!("{mark}{from} → {to}"), 10.0, color);
                    text(ui, &format!("   {}", e.note), 9.0, FAINT);
                }
            });
        }
        let ins = mine.iter().filter(|e| e.to == n.id).count();
        self.section(ui, &n.id, "つながり", &format!("（入る {ins}・出る {}）", mine.len() - ins), false, |ui| {
            for (title, incoming) in [("入ってくる", true), ("出ていく", false)] {
                for e in mine.iter().filter(|e| if incoming { e.to == n.id } else { e.from == n.id }) {
                    let other = if incoming { &e.from } else { &e.to };
                    let kind = if e.blocked.is_empty() { e.status.as_str() } else { "stop" };
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        para(ui, &[("● ", plain(10.0), status_color(kind)), (&format!("{title}: "), plain(10.0), TEXT)]);
                        link(ui, names.get(other.as_str()).copied().unwrap_or(other), Action::Select(other.clone()), &mut out);
                        let lab = if e.label.is_empty() { String::new() } else { format!("（{}）", e.label) };
                        let why = if e.blocked.is_empty() { String::new() } else { format!("　{}", e.blocked) };
                        if !lab.is_empty() || !why.is_empty() {
                            text(ui, &format!("{lab}{why}"), 10.0, TEXT);
                        }
                    });
                }
            }
        });
        if planned {
            self.section(ui, &n.id, "メモ（経緯）", "", false, |ui| {
                if d.notes.is_empty() {
                    text(ui, "メモは無い", 9.0, FAINT);
                }
                for x in &d.notes {
                    let r = x.reference.split_once(':').map_or(x.reference.as_str(), |p| p.1);
                    text(ui, &format!("{r}: {}", x.text), 10.0, TEXT);
                }
            });
        }

        // -- 外へのリンク（中身は出さない） --
        label(ui, "開く");
        let mut any = false;
        ui.horizontal_wrapped(|ui| {
            for r in &n.refs {
                let (project, num) = r.id.split_once(':').unwrap_or(("", &r.id));
                let plan = st.projects.get(project).and_then(|p| p.plan.as_ref()).map(|p| p.path.clone()).unwrap_or_default();
                if let (false, Some(url)) = (plan.is_empty(), gh_url(&gh(project), &format!("blob/main/{plan}"))) {
                    link(ui, &format!("計画書（{num}）"), Action::Open(url), &mut out);
                    any = true;
                    break;
                }
            }
            for (title, paths) in [("コード", &n.paths), ("設計", &n.docs)] {
                for path in paths {
                    if let Some(url) = gh_url(&gh(&n.project), &format!("blob/main/{path}")) {
                        link(ui, &format!("{title}: {}", path.rsplit('/').next().unwrap_or(path)), Action::Open(url), &mut out);
                        any = true;
                    }
                }
            }
        });
        if !any {
            text(ui, "リンクが無い（宣言に paths・docs を足すと、コードと設計へ飛べる）", 9.0, FAINT);
        }
        out
    }

    /// まとめた線の中身（どの部品からどの部品へ）
    pub fn bundle(&mut self, ui: &mut Ui, view: &View, lay: &Layout, bi: usize) -> Option<Action> {
        let b = &lay.bundles[bi];
        let names: HashMap<&str, &str> = view.nodes.iter().map(|x| (x.id.as_str(), x.title.as_str())).collect();
        let zones: HashMap<&str, &str> = view.zones.iter().map(|z| (z.id.as_str(), z.title.as_str())).collect();
        let za = zones.get(b.zones.0.as_str()).unwrap_or(&"");
        let zb = zones.get(b.zones.1.as_str()).unwrap_or(&"");
        ui.label(RichText::new(format!("{za} と {zb}")).font(bold(12.0)).color(TEXT));
        ui.add_space(6.0);
        text(ui, &format!("{} 本の線をまとめている（色は中でいちばん悪い状態）", b.edges.len()), 9.0, FAINT);
        let mut out = None;
        for &i in &b.edges {
            let e = &view.edges[i];
            let kind = if e.blocked.is_empty() { e.status.as_str() } else { "stop" };
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                para(ui, &[("● ", plain(10.0), status_color(kind))]);
                link(ui, names.get(e.from.as_str()).unwrap_or(&""), Action::Select(e.from.clone()), &mut out);
                text(ui, " → ", 10.0, TEXT);
                link(ui, names.get(e.to.as_str()).unwrap_or(&""), Action::Select(e.to.clone()), &mut out);
                if !e.label.is_empty() {
                    text(ui, &format!("（{}）", e.label), 10.0, TEXT);
                }
            });
            if !e.blocked.is_empty() {
                text(ui, &format!("   {}", e.blocked), 10.0, status_color("stop"));
            }
            if !e.note.is_empty() {
                text(ui, &format!("   {}", e.note), 9.0, DIM);
            }
        }
        out
    }
}

/// 右の欄の一覧の 1 行
pub enum Line {
    /// 文字（大きさ・色）
    Text(String, f32, Color32),
    /// 小見出し
    Label(String),
    /// 押すと既定のブラウザで開く
    Link(String, Option<String>),
    Gap,
}

/// 右の欄に一覧を出す（図と現物のずれ・前回からの変化）
pub fn list(ui: &mut Ui, title: &str, lines: &[Line]) -> Option<Action> {
    let mut out = None;
    ui.label(RichText::new(title).font(bold(12.0)).color(TEXT));
    ui.add_space(6.0);
    for l in lines {
        match l {
            Line::Text(s, size, color) => text(ui, s, *size, *color),
            Line::Label(s) => label(ui, s),
            Line::Link(s, url) => link_or_text(ui, s, url.clone(), &mut out),
            Line::Gap => ui.add_space(6.0),
        }
    }
    out
}

/// 図と現物のずれ（宣言漏れ・消えたファイル）。今のタブに出ているプロジェクトだけ
pub fn drift_lines(st: &State, view: &View) -> Vec<Line> {
    let mut lines = vec![Line::Text(
        "見張っている場所にあるのに、どの部品の paths にも無いもの（宣言漏れ）と、paths に書いたのに origin/main に無いもの。\
         部品の paths に足すか、宣言の ignore に足すと消える。"
            .into(),
        9.0,
        FAINT,
    )];
    let Some(a) = &st.arch else { return lines };
    let projects: std::collections::BTreeSet<&str> = view.nodes.iter().map(|n| n.project.as_str()).collect();
    for p in projects {
        let Some(d) = a.drift.get(p) else { continue };
        if d.unclaimed.is_empty() && d.missing.is_empty() {
            continue;
        }
        let gh = st.projects.get(p).map(|x| x.github.clone()).unwrap_or_default();
        lines.push(Line::Gap);
        lines.push(Line::Label(p.to_string()));
        for f in &d.unclaimed {
            lines.push(Line::Link(format!("宣言に無い: {f}"), gh_url(&gh, &format!("tree/main/{f}"))));
        }
        for m in &d.missing {
            let node = m.node.split_once(':').map_or(m.node.as_str(), |x| x.1);
            lines.push(Line::Text(format!("無くなった: {}（{node}）", m.path), 10.0, crate::draw::ERROR));
        }
    }
    lines
}

/// 図と現物のずれの数（今のタブに出ているプロジェクトだけ）
pub fn drift_count(st: &State, view: &View) -> usize {
    let Some(a) = &st.arch else { return 0 };
    let projects: std::collections::BTreeSet<&str> = view.nodes.iter().map(|n| n.project.as_str()).collect();
    projects.iter().filter_map(|p| a.drift.get(*p)).map(|d| d.unclaimed.len() + d.missing.len()).sum()
}

/// 前回（その日の最初の写し）からの変化
pub fn change_lines(st: &State) -> Vec<Line> {
    let Some(a) = &st.arch else { return vec![] };
    let ch = &a.changes;
    let since = if ch.since.is_empty() { "—" } else { ch.since.as_str() };
    let mut lines = vec![Line::Text(format!("{since} の初めから、状態か待ちが変わった部品。"), 9.0, FAINT)];
    for (o, k) in &ch.skeletons {
        lines.push(Line::Text(format!("{o} の骨格: {} 番まで → {} 番まで本番", k.from.unwrap_or(0), k.to), 10.0, TEXT));
    }
    let names: HashMap<&str, &str> = a.nodes.iter().map(|n| (n.id.as_str(), n.title.as_str())).collect();
    let mut ids: Vec<&String> = ch.nodes.keys().collect();
    ids.sort();
    for id in ids {
        let c = &ch.nodes[id];
        let old = c.from.as_ref().map_or("新しく宣言", |w| status_text(&w.status));
        let why = if c.to.blocked.is_empty() { String::new() } else { format!("（{}）", c.to.blocked) };
        let name = names.get(id.as_str()).copied().unwrap_or(id);
        lines.push(Line::Text(format!("{name}: {old} → {}{why}", status_text(&c.to.status)), 10.0, TEXT));
    }
    lines
}
