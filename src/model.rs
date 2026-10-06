//! board.py が書く ~/.herdr/kanban-state.json の "arch" を読む。判断は Python（arch.py）側で済んでいて、ここは読むだけ。

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Zone {
    pub id: String,
    pub title: String,
    pub sub: String,
    pub kind: String,
    pub project: String,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub why: String,
    pub project: String,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Item {
    pub text: String,
    pub done: bool,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Note {
    #[serde(rename = "ref")]
    pub reference: String,
    pub text: String,
}

/// 部品の詳しい中身（arch.py の detail が決める。ここは並べるだけ）
#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Detail {
    pub reason: String,
    pub next: String,
    pub main_pr: Option<Pr>,
    pub items: Vec<Item>,
    pub progress: String,
    pub prs: Vec<Pr>,
    pub notes: Vec<Note>,
    pub ai: String,
}

/// 計画書の項目（"<project>:<番号>"）
#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Ref {
    pub id: String,
    pub critical: bool,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Node {
    pub id: String,
    pub zone: String,
    pub title: String,
    pub sub: String,
    pub status: String,
    pub project: String,
    /// 待ちの理由。待っていなければ空
    pub blocked: String,
    pub skeleton: bool,
    /// プロジェクトごとの骨格の番号
    pub steps: BTreeMap<String, u32>,
    pub detail: Detail,
    pub refs: Vec<Ref>,
    pub paths: Vec<String>,
    pub docs: Vec<String>,
    /// 本番のスイッチ（Vercel の環境変数の名前）を確かめた結果
    pub switch_note: String,
    /// 今出している骨格の番号（view を作るときに steps から入れる）
    #[serde(skip)]
    pub step: Option<u32>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: String,
    pub status: String,
    pub blocked: String,
    pub skeleton: bool,
    pub project: String,
    /// コードで確かめた結果（check のある線だけ）
    pub note: String,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Was {
    pub status: String,
    pub blocked: String,
}

/// 前回（その日の最初の写し）からの変化
#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Change {
    pub from: Option<Was>,
    pub to: Was,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Changes {
    pub since: String,
    pub nodes: HashMap<String, Change>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Arch {
    pub zones: Vec<Zone>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub changes: Changes,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Plan {
    pub path: String,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Project {
    pub github: String,
    pub plan: Option<Plan>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct State {
    pub generated_at: String,
    pub arch: Option<Arch>,
    pub projects: HashMap<String, Project>,
}

/// 読む state。環境変数 ARCH_STATE で差し替えられる（写しを書き換えて、追いつくかを確かめるため）
pub fn state_path() -> PathBuf {
    if let Some(p) = std::env::var_os("ARCH_STATE") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    PathBuf::from(home).join(".herdr").join("kanban-state.json")
}

pub fn load_state() -> Result<State, String> {
    let path = state_path();
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{} を読めない: {e}", path.display()))?;
    let mut value: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("state が JSON でない: {e}"))?;
    drop_nulls(&mut value);
    serde_json::from_value(value).map_err(|e| format!("state の形が違う: {e}"))
}

/// null の欄を消す（無いのと同じに扱う。null が 1 つあるだけで state 全体が読めなくなるのを防ぐ）
fn drop_nulls(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(m) => {
            m.retain(|_, x| !x.is_null());
            m.values_mut().for_each(drop_nulls);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(drop_nulls),
        _ => {}
    }
}

/// 描く範囲。zones は全部（並べるときに使う枠だけ拾う）、部品と線は範囲の中だけ。
#[derive(Clone, Default)]
pub struct View {
    pub zones: Vec<Zone>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// 出している骨格のプロジェクト
    pub owner: Option<String>,
}

/// 全体の view。骨格は最初のプロジェクトのものを出す（arch_view.py の rebuild で scope が全体のとき）。
pub fn view_all(a: &Arch) -> View {
    let owners: std::collections::BTreeSet<&String> = a.nodes.iter().flat_map(|n| n.steps.keys()).collect();
    let owner = owners.iter().next().map(|s| s.to_string());
    let nodes = a
        .nodes
        .iter()
        .map(|n| Node { step: owner.as_ref().and_then(|o| n.steps.get(o).copied()), ..n.clone() })
        .collect();
    View { zones: a.zones.clone(), nodes, edges: a.edges.clone(), owner }
}

pub struct Through {
    pub through: u32,
    pub total: usize,
    pub next: Option<String>,
}

/// arch.py の through の写し。骨格が端から何番まで切れ目なく本番で通っているか。
pub fn through(nodes: &[Node]) -> Through {
    let mut bones: Vec<&Node> = nodes.iter().filter(|n| n.step.is_some()).collect();
    bones.sort_by_key(|n| n.step);
    let mut k = 0;
    for n in &bones {
        if n.status != "live" || !n.blocked.is_empty() {
            break;
        }
        k = n.step.unwrap();
    }
    let next = bones.iter().find(|n| n.step.unwrap() > k).map(|n| n.id.clone());
    Through { through: k, total: bones.len(), next }
}
