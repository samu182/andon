//! カンバン（上の帯の「図｜カンバン」か b）。列とカードの中身は board.py が決める（state の cards）。ここは並べるだけ。
//! 列は「やること → 自分の番 → 外部待ち」。範囲はタブのプロジェクト（全体なら全部）。
//! カードを押すと、まだ済んでいないやることを開く。部品につながっていれば「図で見る」で図のその部品へ。

use crate::draw::{BONE, DIM, FAINT, LINE, PANEL, TEXT, project_mark};
use crate::model::{COLUMNS, Card, State, View, card_node, column_cards};
use egui::text::LayoutJob;
use egui::{Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Margin, Sense, Stroke, TextFormat, Ui};
use std::collections::HashSet;

/// カンバンで押されたもの
pub enum Action {
    /// 図に切り替えて、その部品を選ぶ
    Show(String),
}

/// 開いているカード（"<project>:<id>"）。窓が開いているあいだ覚えておく
#[derive(Default)]
pub struct Board {
    opened: HashSet<String>,
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

/// カードの題。計画書のカードは番号も（計画書と突き合わせる）。修正は「修正」、作業カードは題だけ（番号は読んでも分からない）
pub fn card_label(c: &Card) -> (String, String) {
    match c.kind.as_str() {
        "plan" | "pr" => (c.id.clone(), c.title.clone()),
        "fix" => ("修正".into(), c.title.clone()),
        _ => (String::new(), c.title.clone()),
    }
}

impl Board {
    /// 3 列を並べる。列ごとに縦に流す
    pub fn show(&mut self, ui: &mut Ui, st: &State, view: &View, scope: &str) -> Option<Action> {
        let mut act = None;
        let gap = 16.0;
        let w = ((ui.available_width() - gap * 2.0) / 3.0).floor();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for col in COLUMNS {
                let cards = column_cards(st, scope, col);
                ui.allocate_ui_with_layout(egui::vec2(w, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(w);
                    // 見どころは「自分の番」だけ（紺）。ほかの列の見出しは灰色
                    let ink = if col == "自分の番" { BONE } else { DIM };
                    para(ui, &[(col, bold(11.0), ink), (&format!("  {}", cards.len()), plain(10.0), FAINT)]);
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().id_salt(("kanban", col)).auto_shrink([false, false]).show(ui, |ui| {
                        ui.set_width(w - 8.0);
                        if cards.is_empty() {
                            para(ui, &[("なし", plain(9.0), FAINT)]);
                        }
                        for c in cards {
                            if let Some(a) = self.card(ui, st, view, c, scope.is_empty()) {
                                act = Some(a);
                            }
                            ui.add_space(8.0);
                        }
                    });
                });
            }
        });
        act
    }

    fn card(&mut self, ui: &mut Ui, st: &State, view: &View, c: &Card, all: bool) -> Option<Action> {
        let key = format!("{}:{}", c.project, c.id);
        let open = self.opened.contains(&key);
        let node = card_node(view, c);
        let mut link: Option<(egui::Rect, String)> = None;
        let frame = Frame::new()
            .fill(PANEL)
            .stroke(Stroke::new(1.0_f32, if open { BONE } else { LINE }))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(12, 10));
        let resp = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (id, title) = card_label(c);
                let mut parts: Vec<(&str, FontId, Color32)> = vec![];
                if c.critical {
                    parts.push(("急ぎ  ", bold(9.0), BONE));
                }
                let id_text = if id.is_empty() { String::new() } else { format!("{id}  ") };
                parts.push((&id_text, plain(9.0), FAINT));
                parts.push((&title, bold(10.0), TEXT));
                para(ui, &parts);
                // 全体のときだけプロジェクトの名前（色があれば枠と同じ色の四角）
                let mut tail: Vec<String> = vec![];
                if !c.progress.is_empty() {
                    tail.push(c.progress.clone());
                }
                tail.extend(c.why.iter().cloned());
                if all || !tail.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if all {
                            if let Some(m) = st.projects.get(&c.project).and_then(|p| project_mark(p.color.as_deref()?)) {
                                let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                                ui.painter().rect(r, 1.0, m, Stroke::new(1.0_f32, LINE), egui::StrokeKind::Inside);
                            }
                            para(ui, &[(&c.project, plain(9.0), DIM)]);
                        }
                        if !tail.is_empty() {
                            para(ui, &[(&tail.join(" / "), plain(9.0), FAINT)]);
                        }
                    });
                }
                if open {
                    ui.add_space(6.0);
                    let rest: Vec<&str> = c.items.iter().filter(|i| !i.done).map(|i| i.text.as_str()).collect();
                    if rest.is_empty() {
                        para(ui, &[("残っているやることは無い", plain(9.0), FAINT)]);
                    }
                    for t in rest {
                        para(ui, &[("・", plain(9.0), FAINT), (t, plain(9.0), DIM)]);
                    }
                }
                if let Some(n) = node {
                    ui.add_space(4.0);
                    // 押したかはカード全体の当たりで見る（重ねた当たりは後のものが勝つので、ここに当たりを置いても取られる）
                    link = Some((ui.label(egui::RichText::new(format!("図で見る › {}", n.title)).font(plain(9.0)).color(BONE)).rect, n.id.clone()));
                }
            })
            .response;
        let r = ui.interact(resp.rect, ui.id().with(("card", &key)), Sense::click());
        if r.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if !r.clicked() {
            return None;
        }
        let at = r.interact_pointer_pos();
        match link {
            Some((rect, id)) if at.is_some_and(|p| rect.expand(3.0).contains(p)) => return Some(Action::Show(id)),
            _ => {}
        }
        if !self.opened.remove(&key) {
            self.opened.insert(key);
        }
        None
    }
}
