//! 書き出す。今のタブの図（中身と並べ方）を、ブラウザで操作できる 1 枚の HTML にする。
//! 見た目と操作は assets/export.html（draw.rs・side.rs の写し）。ここは中身の JSON を埋めるだけ。
//! ネットも Graphviz も要らない（並べ方も埋める）。中身は会社の設計なので、置くのは手元（~/.herdr/andon-export）だけ。

use crate::layout::{Layout, NODE_H, NODE_W, ZONE_HEAD};
use crate::model::{State, View, Work};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

const TEMPLATE: &str = include_str!("../assets/export.html");

/// 埋める中身。範囲に出ている部品・プロジェクトの分だけ
pub fn data(st: &State, view: &View, lay: &Layout, scope: &str, working: &HashMap<String, Work>) -> Value {
    let ids: HashSet<&str> = view.nodes.iter().map(|n| n.id.as_str()).collect();
    let names: BTreeSet<&str> = view.nodes.iter().map(|n| n.project.as_str()).collect();
    let arch = st.arch.as_ref();
    let drift: HashMap<&str, Value> = arch
        .map(|a| a.drift.iter().filter(|(p, _)| names.contains(p.as_str())).map(|(p, d)| (p.as_str(), json!(d))).collect())
        .unwrap_or_default();
    let changes = arch.map(|a| {
        let nodes: HashMap<&str, Value> = a.changes.nodes.iter().filter(|(k, _)| ids.contains(k.as_str())).map(|(k, c)| (k.as_str(), json!(c))).collect();
        json!({ "since": a.changes.since, "nodes": nodes, "skeletons": a.changes.skeletons })
    });
    // PR の行き先（github）は部品の外のプロジェクトのこともあるので、全部のプロジェクトの github・色・計画書の場所だけ
    let projects: HashMap<&str, Value> = st
        .projects
        .iter()
        .map(|(k, p)| (k.as_str(), json!({ "github": p.github, "color": p.color, "plan": p.plan.as_ref().map(|x| x.path.clone()) })))
        .collect();
    let working: HashMap<&str, Value> =
        working.iter().filter(|(k, _)| ids.contains(k.as_str())).map(|(k, w)| (k.as_str(), json!({ "branch": w.branch, "focused": w.focused }))).collect();
    json!({
        "scope": scope,
        "generated_at": st.generated_at,
        "zones": view.zones,
        "nodes": view.nodes,
        "edges": view.edges,
        "owner": view.owner,
        "owners": view.owners,
        "layout": lay,
        "changes": changes,
        "drift": drift,
        "projects": projects,
        "working": working,
        "size": { "node_w": NODE_W * 72.0, "node_h": NODE_H * 72.0, "zone_head": ZONE_HEAD },
    })
}

pub fn html(data: &Value) -> String {
    // <script> の中に置くので、</script> で切れないように
    let body = data.to_string().replace("</", "<\\/");
    let title = match data["scope"].as_str() {
        Some("") | None => "全体".to_string(),
        Some(s) => s.to_string(),
    };
    TEMPLATE.replacen("/*DATA*/", &body, 1).replacen("<title>あんどん</title>", &format!("<title>あんどん {title}</title>"), 1)
}

/// 置き場所: ~/.herdr/andon-export/andon-<範囲>-<state の時刻>.html（同じ時刻の state なら上書き）
pub fn default_path(st: &State, scope: &str) -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    let at: String = st.generated_at.chars().take(16).filter(|c| c.is_ascii_digit()).collect(); // 2026-10-14T17:47 → 202610141747
    let name = if scope.is_empty() { "all" } else { scope };
    PathBuf::from(home).join(".herdr").join("andon-export").join(format!("andon-{name}-{at}.html"))
}

pub fn write(path: &std::path::Path, st: &State, view: &View, lay: &Layout, scope: &str, working: &HashMap<String, Work>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{} を作れない: {e}", dir.display()))?;
    }
    std::fs::write(path, html(&data(st, view, lay, scope, working))).map_err(|e| format!("{} に書けない: {e}", path.display()))
}

/// 既定のブラウザで開くための file:// の URL
pub fn file_url(path: &std::path::Path) -> String {
    format!("file:///{}", path.display().to_string().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Arch, Node, Zone};

    #[test]
    fn 埋めた中身は範囲の分だけで_scriptで切れない() {
        let node = |id: &str, project: &str, title: &str| Node { id: id.into(), project: project.into(), zone: "z".into(), title: title.into(), ..Default::default() };
        let mut a = Arch { zones: vec![Zone { id: "z".into(), ..Default::default() }], nodes: vec![node("a:x", "a", "</script>"), node("b:y", "b", "y")], ..Default::default() };
        a.drift.insert("b".into(), Default::default());
        let st = State { generated_at: "2026-10-14T17:47:34+09:00".into(), arch: Some(a.clone()), ..Default::default() };
        let view = crate::model::view_for(&a, "a", None);
        let lay = Layout { pos: Default::default(), frames: vec![], paths: vec![], size: (0.0, 0.0), bundles: vec![] };
        let d = data(&st, &view, &lay, "a", &HashMap::new());
        assert_eq!(d["nodes"].as_array().unwrap().len(), 1);
        assert!(d["drift"].get("b").is_none());
        let out = html(&d);
        assert!(!out.contains("/*DATA*/"));
        assert!(!out.contains("\"</script>\""));
        assert!(out.contains("<title>あんどん a</title>"));
        assert!(default_path(&st, "").ends_with("andon-all-202610141747.html"));
    }
}
