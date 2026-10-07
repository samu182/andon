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
    pub branch: String,
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
    /// つながっているカード（"<project>:<カードの id>"）
    pub plan: Vec<String>,
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

/// 骨格の、端から切れ目なく本番の所の変化
#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct SkeletonChange {
    pub from: Option<u32>,
    pub to: u32,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Changes {
    pub since: String,
    pub nodes: HashMap<String, Change>,
    pub skeletons: BTreeMap<String, SkeletonChange>,
}

/// paths に書いたのに origin/main に無いもの
#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Missing {
    pub path: String,
    pub node: String,
}

/// 図と現物のずれ（プロジェクトごと）
#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Drift {
    /// 見張っている場所にあるのに、どの部品の paths にも無いもの（宣言漏れ）
    pub unclaimed: Vec<String>,
    pub missing: Vec<Missing>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Arch {
    pub zones: Vec<Zone>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub changes: Changes,
    pub drift: HashMap<String, Drift>,
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
    /// 画面でのプロジェクトの色（kanban.json の "color"。カンバンの画面と同じ色の名前）
    pub color: Option<String>,
    pub plan: Option<Plan>,
    /// 番号（文字）ごとの PR
    pub prs: HashMap<String, Pr>,
}

/// カンバンのカード（作業中の判定に使うところだけ）
#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Card {
    pub project: String,
    pub id: String,
    pub prs: Vec<u64>,
    /// PR がまだ無いあいだ、このカードの作業をしているブランチ（plan カードの id はブランチ名とそろわないため）
    pub branches: Vec<String>,
}

/// 作業中の部品: どのブランチで、今選んでいるペインか
#[derive(Clone, PartialEq, Debug)]
pub struct Work {
    pub branch: String,
    pub focused: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct State {
    pub generated_at: String,
    pub arch: Option<Arch>,
    pub projects: HashMap<String, Project>,
    pub cards: Vec<Card>,
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

/// ブランチ名とカードの id を比べるための形（feat/・fix/ などの前置きと記号を落とす）
fn norm(name: &str) -> String {
    let low = name.to_lowercase();
    let mut rest = low.as_str();
    for p in ["feat", "fix", "docs", "chore", "refactor", "test"] {
        if let Some(r) = low.strip_prefix(p).and_then(|r| r.strip_prefix('/').or_else(|| r.strip_prefix('-'))) {
            rest = r;
            break;
        }
    }
    rest.chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect()
}

/// 今どこを作業しているか（arch.py の working の写し）。
/// herdr のペインのブランチ → そのブランチの開いている PR → その PR をつないだカード → 部品。
/// PR がまだ無いブランチは、カードの branches に書いてあればそのカード、無ければ名前がカードの id と同じなら
/// （fix/tap-button と fix-tap-button）そのカード。
/// branches: {ブランチ名: 今選んでいるペインか}
pub fn working(nodes: &[Node], cards: &[Card], projects: &HashMap<String, Project>, branches: &BTreeMap<String, bool>) -> HashMap<String, Work> {
    let mut card_of_pr: HashMap<(String, u64), String> = HashMap::new();
    let mut card_by_name: HashMap<String, String> = HashMap::new();
    for c in cards {
        let key = format!("{}:{}", c.project, c.id);
        card_by_name.insert(norm(&c.id), key.clone());
        for b in &c.branches {
            card_by_name.insert(norm(b), key.clone());
        }
        for &num in &c.prs {
            card_of_pr.insert((c.project.clone(), num), key.clone());
        }
    }
    let mut keys: HashMap<String, Work> = HashMap::new();
    for (branch, &focused) in branches {
        let mut hit: Vec<String> = vec![];
        for (proj, p) in projects {
            for pr in p.prs.values() {
                if pr.branch == *branch && pr.state == "OPEN" {
                    if let Some(k) = card_of_pr.get(&(proj.clone(), pr.number)) {
                        hit.push(k.clone());
                    }
                }
            }
        }
        if hit.is_empty() {
            if let Some(k) = card_by_name.get(&norm(branch)) {
                hit.push(k.clone());
            }
        }
        for k in hit {
            let was = keys.get(&k).is_some_and(|w| w.focused);
            keys.insert(k, Work { branch: branch.clone(), focused: focused || was });
        }
    }
    let mut out = HashMap::new();
    for n in nodes {
        for r in &n.plan {
            if let Some(w) = keys.get(r) {
                out.insert(n.id.clone(), w.clone());
            }
        }
    }
    out
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
    fn ブランチから_pr_カード_部品へ_名前でも合わせる() {
        let node = |id: &str, plan: &[&str]| Node { id: id.into(), plan: plan.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        let nodes = vec![node("p:mail", &["p:C-8"]), node("p:tap", &["p:fix-tap-button"]), node("p:db", &[])];
        let cards = vec![
            Card { project: "p".into(), id: "C-8".into(), prs: vec![71], ..Default::default() },
            Card { project: "p".into(), id: "fix-tap-button".into(), ..Default::default() },
        ];
        let pr = Pr { number: 71, state: "OPEN".into(), branch: "feat/ses-delivery".into(), ..Default::default() };
        let projects: HashMap<String, Project> =
            [("p".to_string(), Project { prs: [("71".to_string(), pr)].into_iter().collect(), ..Default::default() })].into_iter().collect();
        let branches: BTreeMap<String, bool> =
            [("feat/ses-delivery", true), ("fix/tap-button", false), ("main", false)].iter().map(|(k, v)| (k.to_string(), *v)).collect();
        let got = working(&nodes, &cards, &projects, &branches);
        assert_eq!(got.len(), 2);
        assert_eq!(got["p:mail"], Work { branch: "feat/ses-delivery".into(), focused: true });
        assert_eq!(got["p:tap"], Work { branch: "fix/tap-button".into(), focused: false });
    }

    #[test]
    fn pr_の無いブランチはカードに書いたもので合わせる() {
        let nodes = vec![Node { id: "p:monthly".into(), plan: vec!["p:C-15".into()], ..Default::default() }];
        let cards = vec![Card { project: "p".into(), id: "C-15".into(), branches: vec!["feat/monthly-report".into()], ..Default::default() }];
        let branches: BTreeMap<String, bool> = [("feat/monthly-report".to_string(), false)].into_iter().collect();
        let got = working(&nodes, &cards, &HashMap::new(), &branches);
        assert_eq!(got["p:monthly"], Work { branch: "feat/monthly-report".into(), focused: false });
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
