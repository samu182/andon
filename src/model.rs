//! board.py が書く ~/.herdr/kanban-state.json の "arch" を読む。判断は Python（arch.py）側で済んでいて、ここは読むだけ。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Zone {
    pub id: String,
    pub title: String,
    pub sub: String,
    pub kind: String,
    pub project: String,
    /// 構想（完成図）だけの枠
    #[serde(skip_serializing_if = "is_false")]
    pub future: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub why: String,
    pub project: String,
    pub branch: String,
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Item {
    pub text: String,
    pub done: bool,
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Note {
    #[serde(rename = "ref")]
    pub reference: String,
    pub text: String,
}

/// 部品の詳しい中身（arch.py の detail が決める。ここは並べるだけ）
#[derive(Deserialize, Serialize, Clone, Default)]
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
#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Ref {
    pub id: String,
    pub critical: bool,
}

#[derive(Deserialize, Serialize, Clone, Default)]
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
    /// 構想（完成図だけに出す。作るかは未定）。状態・骨格・待ちを持たない
    #[serde(skip_serializing_if = "is_false")]
    pub future: bool,
    /// 構想の出所（"<パス>#<目印>"）と、宣言に書いたメモ
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// 構想の知らせ（出所が main に無い・変わった / 作り始めた）。ずれには数えない
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source_note: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub promote: String,
    /// 今出している骨格の番号（view を作るときに steps から入れる）
    #[serde(skip)]
    pub step: Option<u32>,
}

#[derive(Deserialize, Serialize, Clone, Default)]
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
    /// 構想の線（完成図だけに出す）
    #[serde(skip_serializing_if = "is_false")]
    pub future: bool,
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Was {
    pub status: String,
    pub blocked: String,
}

/// 前回（その日の最初の写し）からの変化
#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Change {
    pub from: Option<Was>,
    pub to: Was,
}

/// 骨格の、端から切れ目なく本番の所の変化
#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct SkeletonChange {
    pub from: Option<u32>,
    pub to: u32,
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Changes {
    pub since: String,
    pub nodes: HashMap<String, Change>,
    pub skeletons: BTreeMap<String, SkeletonChange>,
}

/// paths に書いたのに origin/main に無いもの
#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Missing {
    pub path: String,
    pub node: String,
}

/// 図と現物のずれ（プロジェクトごと）
#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Drift {
    /// 見張っている場所にあるのに、どの部品の paths にも無いもの（宣言漏れ）
    pub unclaimed: Vec<String>,
    pub missing: Vec<Missing>,
    /// 計画書の項目のうち、どの部品・線の plan にも無いもの（図に描かれていない作業）
    pub unplanned: Vec<Unplanned>,
    /// 部品の docs の設計書が、宣言（arch.json）の保存より後に変わったもの（設計が変わった）
    pub stale_docs: Vec<StaleDoc>,
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct StaleDoc {
    pub doc: String,
    pub nodes: Vec<String>,
    pub hash: String,
    pub subject: String,
    pub date: String,
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Unplanned {
    pub id: String,
    pub title: String,
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Arch {
    pub zones: Vec<Zone>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub changes: Changes,
    pub drift: HashMap<String, Drift>,
    /// プロジェクトごとのリポジトリの場所（開いたときの範囲を、herdr のペインのフォルダから決める）
    pub repos: HashMap<String, String>,
    /// 構想（arch-future.json）。完成図のときだけ図に足す
    pub future: Future,
}

/// 構想（完成図）。arch.py の future が決める。今の図の判断には入らない
#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Future {
    pub zones: Vec<Zone>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// 落とした宣言の知らせ（分からない枠・部品）
    pub notes: Vec<String>,
}

impl Future {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty()
    }
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Plan {
    pub path: String,
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Project {
    pub github: String,
    /// 画面でのプロジェクトの色（kanban.json の "color"。カンバンの画面と同じ色の名前）
    pub color: Option<String>,
    /// タブの並び（kanban.json の "order"。小さいほど左。無ければ名前の順で後ろ）
    pub order: Option<i64>,
    pub plan: Option<Plan>,
    /// 番号（文字）ごとの PR
    pub prs: HashMap<String, Pr>,
}

/// カンバンのカード（作業中の判定に使うところだけ）
#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct Card {
    pub project: String,
    pub id: String,
    pub prs: Vec<u64>,
    /// PR がまだ無いあいだ、このカードの作業をしているブランチ（plan カードの id はブランチ名とそろわないため）
    pub branches: Vec<String>,
    /// plan（計画書の項目）・fix（修正）・work（作業カード）
    pub kind: String,
    pub title: String,
    pub critical: bool,
    /// やること・自分の番・外部待ち（board.py が決める）
    pub column: String,
    /// 待っているもの・「N 日動きなし」
    pub why: Vec<String>,
    /// やることの済んだ数（「3/8」。無ければ空）
    pub progress: String,
    pub order: i64,
    pub items: Vec<Item>,
}

/// カンバンの列の順
pub const COLUMNS: [&str; 3] = ["やること", "自分の番", "外部待ち"];

/// その列のカード（範囲が空なら全部のプロジェクト）。プロジェクトのタブの並び → カードの order の順
pub fn column_cards<'a>(st: &'a State, scope: &str, column: &str) -> Vec<&'a Card> {
    let rank = |p: &str| st.projects.get(p).and_then(|x| x.order).unwrap_or(i64::MAX);
    let mut out: Vec<&Card> = st.cards.iter().filter(|c| c.column == column && (scope.is_empty() || c.project == scope)).collect();
    out.sort_by(|a, b| (rank(&a.project), &a.project, a.order).cmp(&(rank(&b.project), &b.project, b.order)));
    out
}

/// カードの部品（plan に "<project>:<カードの id>" がある部品）。無ければ None
pub fn card_node<'a>(view: &'a View, c: &Card) -> Option<&'a Node> {
    let key = format!("{}:{}", c.project, c.id);
    view.nodes.iter().find(|n| !n.future && n.plan.contains(&key))
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

/// 窓に出すタブの範囲。左から全体（空）、構成図に部品があるプロジェクトを order の順（無ければ名前の順で後ろ）
pub fn tab_scopes(st: &State) -> Vec<String> {
    let mut names: Vec<&str> = st.arch.as_ref().map_or(vec![], |a| a.nodes.iter().map(|n| n.project.as_str()).filter(|p| !p.is_empty()).collect());
    names.sort();
    names.dedup();
    names.sort_by_key(|n| st.projects.get(*n).and_then(|p| p.order).unwrap_or(i64::MAX));
    std::iter::once(String::new()).chain(names.into_iter().map(String::from)).collect()
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
    /// 完成図（構想を足した図）か
    pub full: bool,
}

/// 全体の view（--dump-layout と、Python との突き合わせで使う）
pub fn view_all(a: &Arch, picked: Option<&str>) -> View {
    view_for(a, "", picked)
}

/// 表示する部品の id（arch.py の scope の写し）。scope が空なら全部。あれば、そのプロジェクトの部品と、線 1 本でつながる部品。
/// ほかのプロジェクトの部品でも、そのプロジェクトの骨格に番号を持つものは入れる（骨格の番号が抜けて見えないように）
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
    out.extend(a.nodes.iter().filter(|n| n.steps.contains_key(scope)).map(|n| n.id.clone()));
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
    View { zones: a.zones.clone(), nodes, edges, owner, owners, full: false }
}

/// 今か完成図かで view を作る。full でなければ view_for と同じ（構想は読まない）。
/// full なら、今の view の後ろに構想を足す（並びは 今の枠・部品・線 → 構想の枠・部品・線。書き出しはこの並びを前提にする）。
/// 範囲がプロジェクトなら、そのプロジェクトの構想と、範囲の部品に線 1 本でつながる構想を足す。
/// 今の部品は足さない（ずれ・変化・骨格の数を今の図とそろえる）。両端が図に無い線は出さない
pub fn view_mode(a: &Arch, scope: &str, picked: Option<&str>, full: bool) -> View {
    let mut v = view_for(a, scope, picked);
    v.full = full;
    if !full || a.future.is_empty() {
        return v;
    }
    let f = &a.future;
    let zones: std::collections::HashSet<&str> = a.zones.iter().chain(&f.zones).map(|z| z.id.as_str()).collect();
    let mut keep: std::collections::HashSet<String> = v.nodes.iter().map(|n| n.id.clone()).collect();
    let own: std::collections::HashSet<&str> =
        f.nodes.iter().filter(|n| scope.is_empty() || n.project == scope).map(|n| n.id.as_str()).collect();
    let touches = |id: &str, other: &dyn Fn(&str) -> bool| f.edges.iter().any(|e| (e.from == id && other(&e.to)) || (e.to == id && other(&e.from)));
    let futures: Vec<&Node> = f
        .nodes
        .iter()
        .filter(|n| zones.contains(n.zone.as_str()))
        .filter(|n| own.contains(n.id.as_str()) || touches(&n.id, &|x| keep.contains(x) || own.contains(x)))
        .collect();
    keep.extend(futures.iter().map(|n| n.id.clone()));
    v.nodes.extend(futures.into_iter().map(|n| Node { future: true, step: None, ..n.clone() }));
    v.edges.extend(f.edges.iter().filter(|e| keep.contains(&e.from) && keep.contains(&e.to)).map(|e| Edge { future: true, ..e.clone() }));
    v.zones.extend(f.zones.iter().map(|z| Zone { future: true, ..z.clone() }));
    v
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

    /// 今の部品 2 つ（shop・api）と、構想 2 つ（shop の構想だけの枠に 1 つ・api の今の枠に 1 つ）
    fn future_arch() -> Arch {
        let state = serde_json::json!({ "arch": {
            "zones": [{ "id": "shop:z" }, { "id": "api:z" }],
            "nodes": [{ "id": "shop:cart", "project": "shop", "zone": "shop:z", "steps": { "shop": 1 } }, { "id": "api:stock", "project": "api", "zone": "api:z" }],
            "edges": [],
            "future": {
                "zones": [{ "id": "shop:later", "title": "あとで", "future": true }],
                "nodes": [
                    { "id": "shop:points", "project": "shop", "zone": "shop:later", "title": "ポイント", "status": "future", "future": true,
                      "source": "docs/構想.md#ポイント", "source_note": "出所が変わった（見直す）", "promote": "" },
                    { "id": "api:bulk", "project": "api", "zone": "api:z", "future": true }
                ],
                "edges": [{ "from": "shop:points", "to": "api:stock", "future": true, "status": "future" }],
                "notes": ["線 a → b: b が見つからない（落とした）"]
            }
        }});
        serde_json::from_value::<State>(state).unwrap().arch.unwrap()
    }

    #[test]
    fn 構想を読む() {
        let a = future_arch();
        assert_eq!(a.future.nodes[0].source, "docs/構想.md#ポイント");
        assert_eq!(a.future.nodes[0].source_note, "出所が変わった（見直す）");
        assert!(a.future.nodes[0].future && a.future.zones[0].future && a.future.edges[0].future);
        assert_eq!(a.future.notes.len(), 1);
        let none: State = serde_json::from_value(serde_json::json!({ "arch": { "nodes": [] } })).unwrap();
        assert!(none.arch.unwrap().future.is_empty()); // 構想の無い state も読める
    }

    #[test]
    fn 構想は完成図のときだけ図に入る() {
        let a = future_arch();
        let now = view_mode(&a, "", None, false);
        assert_eq!(now.nodes.len(), 2);
        assert!(now.edges.is_empty() && now.zones.len() == 2);
        let full = view_mode(&a, "", None, true);
        let ids: Vec<&str> = full.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["shop:cart", "api:stock", "shop:points", "api:bulk"]); // 今の部品が先、構想は後ろ
        assert_eq!(full.edges.len(), 1);
        assert_eq!(full.zones.len(), 3);
        assert!(full.nodes[2].future && full.nodes[2].step.is_none());
        assert_eq!(full.nodes[0].step, Some(1)); // 今の部品の骨格はそのまま
        assert_eq!(bones(&full.nodes).len(), 1); // 構想は骨格に入らない
    }

    #[test]
    fn 範囲では_そのプロジェクトの構想と_範囲につながる構想を足す() {
        let a = future_arch();
        let shop = view_mode(&a, "shop", None, true);
        let mut ids: Vec<&str> = shop.nodes.iter().map(|n| n.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["shop:cart", "shop:points"]); // 範囲の外の今の部品（api:stock）は足さないので、その線も出さない
        assert!(shop.edges.is_empty());
        let api = view_mode(&a, "api", None, true);
        let mut ids: Vec<&str> = api.nodes.iter().map(|n| n.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["api:bulk", "api:stock", "shop:points"]); // 範囲の部品に線 1 本でつながる構想
    }

    #[test]
    fn 構想の欄は空なら書き出さない() {
        let v = serde_json::to_value(Node { id: "a".into(), ..Default::default() }).unwrap();
        for k in ["future", "source", "note", "source_note", "promote"] {
            assert!(v.get(k).is_none(), "{k} が出ている");
        }
        assert!(serde_json::to_value(Edge::default()).unwrap().get("future").is_none());
        assert!(serde_json::to_value(Zone::default()).unwrap().get("future").is_none());
    }

    #[test]
    fn 範囲にはそのプロジェクトの骨格の部品も入る() {
        let bone = |id: &str, project: &str, owner: &str| Node {
            id: id.into(),
            project: project.into(),
            steps: [(owner.to_string(), 1)].into_iter().collect(),
            ..Default::default()
        };
        let a = Arch {
            nodes: vec![Node { id: "api:stock".into(), project: "api".into(), ..Default::default() }, bone("x:relay", "x", "api"), bone("x:other", "x", "x")],
            ..Default::default()
        };
        let mut got: Vec<String> = scope_ids(&a, "api").into_iter().collect();
        got.sort();
        assert_eq!(got, ["api:stock", "x:relay"]);
    }

    #[test]
    fn カンバンの列は範囲で絞り_タブの並びとカードの順に並べ_部品は計画書の項目でたどる() {
        let st: State = serde_json::from_value(serde_json::json!({
            "projects": { "a": { "github": "", "order": 2 }, "b": { "github": "", "order": 1 } },
            "cards": [
                { "project": "a", "id": "C-2", "column": "やること", "order": 2 },
                { "project": "a", "id": "C-1", "column": "やること", "order": 1 },
                { "project": "b", "id": "fix-x", "column": "やること", "order": 9 },
                { "project": "a", "id": "C-3", "column": "外部待ち", "order": 3 }
            ],
            "arch": { "zones": [{ "id": "z" }], "nodes": [
                { "id": "a:n", "zone": "z", "project": "a", "plan": ["a:C-1"] },
                { "id": "a:f", "zone": "z", "project": "a", "plan": ["a:C-2"], "future": true }
            ], "edges": [] }
        }))
        .unwrap();
        let ids = |scope: &str, col: &str| column_cards(&st, scope, col).iter().map(|c| c.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids("", "やること"), ["fix-x", "C-1", "C-2"]); // b（order 1）が先
        assert_eq!(ids("a", "やること"), ["C-1", "C-2"]);
        assert_eq!(ids("a", "自分の番"), Vec::<String>::new());
        let view = view_for(st.arch.as_ref().unwrap(), "", None);
        let card = |id: &str| st.cards.iter().find(|c| c.id == id).unwrap();
        assert_eq!(card_node(&view, card("C-1")).map(|n| n.id.as_str()), Some("a:n"));
        assert!(card_node(&view, card("C-2")).is_none()); // 構想の部品へは飛ばない
        assert!(card_node(&view, card("fix-x")).is_none());
    }
}
