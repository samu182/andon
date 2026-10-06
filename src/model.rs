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
    /// プロジェクトごとのリポジトリの場所（開いたときの範囲を、herdr のペインのフォルダから決める）
    pub repos: HashMap<String, String>,
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
    /// 骨格を持っているプロジェクト（全体では、帯の名前を押して切り替える）
    pub owners: Vec<String>,
}

/// 全体の view（--dump-layout と、Python との突き合わせで使う）
pub fn view_all(a: &Arch, picked: Option<&str>) -> View {
    view_for(a, "", picked)
}

/// 表示する部品の id（arch.py の scope の写し）。scope が空なら全部。あれば、そのプロジェクトの部品と、線 1 本でつながる部品
pub fn scope_ids(a: &Arch, scope: &str) -> std::collections::HashSet<String> {
    let ids: std::collections::HashSet<String> = a.nodes.iter().map(|n| n.id.clone()).collect();
    if scope.is_empty() {
        return ids;
    }
    let own: std::collections::HashSet<String> = a.nodes.iter().filter(|n| n.project == scope).map(|n| n.id.clone()).collect();
    let mut out = own.clone();
    for e in &a.edges {
        if own.contains(&e.from) && ids.contains(&e.to) {
            out.insert(e.to.clone());
        }
        if own.contains(&e.to) && ids.contains(&e.from) {
            out.insert(e.from.clone());
        }
    }
    out
}

/// タブの範囲（scope: プロジェクト名、空なら全体）の view（arch_view.py の rebuild の写し）。
/// どのプロジェクトの骨格を出すか: プロジェクトの範囲ではそのプロジェクトの骨格だけ（無ければ出さない）。
/// 全体では picked（帯の名前を押して選んだもの）、無ければ最初のもの。
/// 部品は写しにして、出す骨格の番号だけを step に入れる（タブごとに出す骨格が違っても混ざらない）
pub fn view_for(a: &Arch, scope: &str, picked: Option<&str>) -> View {
    let keep = scope_ids(a, scope);
    let all: Vec<String> = a
        .nodes
        .iter()
        .filter(|n| keep.contains(&n.id))
        .flat_map(|n| n.steps.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let (owner, owners) = if scope.is_empty() {
        let owner = picked.filter(|p| all.iter().any(|o| o == p)).map(str::to_string).or_else(|| all.first().cloned());
        (owner, all)
    } else {
        let owner = all.iter().find(|o| *o == scope).cloned();
        let owners = owner.iter().cloned().collect();
        (owner, owners)
    };
    let nodes = a
        .nodes
        .iter()
        .filter(|n| keep.contains(&n.id))
        .map(|n| Node { step: owner.as_ref().and_then(|o| n.steps.get(o).copied()), ..n.clone() })
        .collect();
    let edges = a.edges.iter().filter(|e| keep.contains(&e.from) && keep.contains(&e.to)).cloned().collect();
    View { zones: a.zones.clone(), nodes, edges, owner, owners }
}

/// フォルダの形をそろえる（Windows は大文字小文字・区切りの向きを区別しない）
fn norm_path(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// フォルダから、どのプロジェクトの範囲を見せるか（arch.py の project_of の写し）。
/// リポジトリ（と横の <repo>-wt-*）ならそのプロジェクト、リポジトリを並べたフォルダ（develop など）なら全部（""）、
/// 関係ないフォルダなら None（今の範囲のまま）
pub fn project_of(cwd: &str, repos: &HashMap<String, String>) -> Option<String> {
    if cwd.is_empty() {
        return None;
    }
    let here = norm_path(cwd);
    let mut chain = vec![here.clone()];
    let mut cur = here.as_str();
    while let Some(i) = cur.rfind('/') {
        cur = &cur[..i];
        if cur.is_empty() {
            break;
        }
        chain.push(cur.to_string());
    }
    let split = |p: &str| -> (String, String) {
        match p.rfind('/') {
            Some(i) => (p[..i].to_string(), p[i + 1..].to_string()),
            None => (String::new(), p.to_string()),
        }
    };
    let mut parents = vec![];
    let mut projects: Vec<(&String, &String)> = repos.iter().filter(|(_, r)| !r.is_empty()).collect();
    projects.sort(); // 決まった順に調べる
    for (project, repo) in projects {
        let root = norm_path(repo);
        let (root_parent, root_name) = split(&root);
        parents.push(root_parent.clone());
        let wt = format!("{root_name}-wt-");
        if chain.iter().any(|p| *p == root || {
            let (pp, name) = split(p);
            pp == root_parent && name.starts_with(&wt)
        }) {
            return Some(project.clone());
        }
    }
    parents.contains(&here).then(String::new)
}

pub struct Through {
    pub through: u32,
    pub next: Option<String>,
}

/// 骨格の部品を番号順に
pub fn bones(nodes: &[Node]) -> Vec<&Node> {
    let mut b: Vec<&Node> = nodes.iter().filter(|n| n.step.is_some()).collect();
    b.sort_by_key(|n| n.step);
    b
}

/// arch.py の through の写し。骨格が端から何番まで切れ目なく本番で通っているか。
pub fn through(nodes: &[Node]) -> Through {
    let bones = bones(nodes);
    let mut k = 0;
    for n in &bones {
        if n.status != "live" || !n.blocked.is_empty() {
            break;
        }
        k = n.step.unwrap();
    }
    let next = bones.iter().find(|n| n.step.unwrap() > k).map(|n| n.id.clone());
    Through { through: k, next }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repos() -> HashMap<String, String> {
        [("shop", "C:/dev/shop"), ("api", "C:/dev/api"), ("none", "")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn フォルダからプロジェクト() {
        assert_eq!(project_of("C:/dev/shop", &repos()).as_deref(), Some("shop"));
        assert_eq!(project_of("C:/dev/shop/src/x", &repos()).as_deref(), Some("shop"));
        assert_eq!(project_of(r"C:\Dev\Shop\src", &repos()).as_deref(), Some("shop")); // 区切りと大文字小文字は問わない
        assert_eq!(project_of("C:/dev/api-wt-fix", &repos()).as_deref(), Some("api")); // 横の worktree
        assert_eq!(project_of("C:/dev/api-wt-fix/src", &repos()).as_deref(), Some("api"));
        assert_eq!(project_of("C:/dev", &repos()).as_deref(), Some("")); // リポジトリを並べたフォルダ: 全体
        assert_eq!(project_of("C:/elsewhere", &repos()), None); // 関係ないフォルダ
        assert_eq!(project_of("", &repos()), None);
    }

    #[test]
    fn 範囲はそのプロジェクトと線1本でつながる部品() {
        let node = |id: &str, project: &str| Node { id: id.into(), project: project.into(), ..Default::default() };
        let edge = |a: &str, b: &str| Edge { from: a.into(), to: b.into(), ..Default::default() };
        let a = Arch {
            nodes: vec![node("shop:cart", "shop"), node("shop:mail", "shop"), node("api:stock", "api"), node("api:admin", "api"), node("x:far", "x"), node("x:far2", "x")],
            edges: vec![edge("shop:cart", "api:stock"), edge("api:admin", "shop:mail"), edge("x:far", "x:far2"), edge("api:stock", "api:admin")],
            ..Default::default()
        };
        let mut got: Vec<String> = scope_ids(&a, "api").into_iter().collect();
        got.sort();
        assert_eq!(got, ["api:admin", "api:stock", "shop:cart", "shop:mail"]);
        assert_eq!(scope_ids(&a, "").len(), 6);
    }
}
