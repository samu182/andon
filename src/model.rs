//! board.py が書く ~/.herdr/kanban-state.json の "arch" を読む。判断は Python（arch.py）側で済んでいて、ここは読むだけ。

use serde::Deserialize;
use std::collections::BTreeMap;
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
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Arch {
    pub zones: Vec<Zone>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub changes: serde_json::Value,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct State {
    pub generated_at: String,
    pub arch: Option<Arch>,
}

pub fn state_path() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    PathBuf::from(home).join(".herdr").join("kanban-state.json")
}

pub fn load_state() -> Result<State, String> {
    let path = state_path();
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{} を読めない: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("state の形が違う: {e}"))
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
