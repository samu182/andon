//! 書き出す。今のタブの図（中身と並べ方）を、ブラウザで操作できる 1 枚の HTML にする。
//! 見た目と操作は assets/export.html（draw.rs・side.rs の写し）。ここは中身の JSON を埋めるだけ。
//! ネットも Graphviz も要らない（並べ方も埋める）。中身は会社の設計なので、置くのは手元（ダウンロードフォルダ）だけ。
//! 共有用（share）は人に渡す前提で、AI に渡す文・作業中のブランチ（PR のブランチ名も）・図と現物のずれ・
//! 確かめた結果（Vercel の環境変数の名前・コードの確かめ）を抜く。図・状態・なぜ・次の一手・PR・リンクは残す。
//! 構想（arch-future.json）があれば完成図の中身と並べ方も入れ、HTML の側で「今｜完成図」を切り替えられるようにする
//! （構想の出所は共有用でも残す）。

use crate::layout::{Layout, NODE_H, NODE_W, ZONE_HEAD};
use crate::model::{State, View, Work};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

const TEMPLATE: &str = include_str!("../assets/export.html");

/// 埋める中身。範囲に出ている部品・プロジェクトの分だけ。
/// full は完成図の中身と並べ方（構想が無ければ None）。部品のうち今の図にもあるものは id だけにする（同じ中身を 2 度入れない）。
/// start_full なら、開いたときに完成図を出す（窓で完成図を見ていたとき）
#[allow(clippy::too_many_arguments)]
pub fn data(
    st: &State,
    view: &View,
    lay: &Layout,
    full: Option<(&View, &Layout)>,
    scope: &str,
    working: &HashMap<String, Work>,
    share: bool,
    start_full: bool,
) -> Value {
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
    let mut d = json!({
        "share": share,
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
    });
    if let Some((fv, fl)) = full {
        let nodes: Vec<Value> = fv.nodes.iter().map(|n| if !n.future && ids.contains(n.id.as_str()) { json!(n.id) } else { json!(n) }).collect();
        d["future"] = json!({ "zones": fv.zones, "nodes": nodes, "edges": fv.edges, "owner": fv.owner, "owners": fv.owners, "layout": fl });
        d["start_full"] = json!(start_full);
    }
    if share {
        strip_private(&mut d);
    }
    d
}

/// 共有用に、人に渡さないものを抜く（完成図の中身からも。構想の出所は残す）
fn strip_private(d: &mut Value) {
    d["working"] = json!({});
    d["drift"] = json!({});
    strip_parts(d);
    if let Some(f) = d.get_mut("future") {
        strip_parts(f);
    }
}

/// 部品と線から、人に渡さないものを抜く。完成図の部品のうち、今の図にもあるものは id（文字）だけなので触らない
fn strip_parts(d: &mut Value) {
    for n in d["nodes"].as_array_mut().into_iter().flatten().filter(|n| n.is_object()) {
        n["switch_note"] = json!("");
        let det = &mut n["detail"];
        det["ai"] = json!("");
        if det["main_pr"].is_object() {
            det["main_pr"]["branch"] = json!("");
        }
        for pr in det["prs"].as_array_mut().into_iter().flatten() {
            pr["branch"] = json!("");
        }
    }
    for e in d["edges"].as_array_mut().into_iter().flatten() {
        e["note"] = json!("");
    }
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

/// 置き場所: ダウンロードフォルダ（~/Downloads）の andon-<範囲>-<state の時刻>[-share].html（同じ時刻の state なら上書き）。
/// 人に渡すときにすぐ見つかる所に置く（2026-10-14 志賀さんの希望）
pub fn default_path(st: &State, scope: &str, share: bool) -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    let at: String = st.generated_at.chars().take(16).filter(|c| c.is_ascii_digit()).collect(); // 2026-10-14T17:47 → 202610141747
    let name = if scope.is_empty() { "all" } else { scope };
    PathBuf::from(home).join("Downloads").join(format!("andon-{name}-{at}{}.html", if share { "-share" } else { "" }))
}

/// now は今の図、full は完成図（構想が無ければ None）の、中身と並べ方
#[allow(clippy::too_many_arguments)]
pub fn write(
    path: &std::path::Path,
    st: &State,
    now: (&View, &Layout),
    full: Option<(&View, &Layout)>,
    scope: &str,
    working: &HashMap<String, Work>,
    share: bool,
    start_full: bool,
) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{} を作れない: {e}", dir.display()))?;
    }
    let d = data(st, now.0, now.1, full, scope, working, share, start_full);
    std::fs::write(path, html(&d)).map_err(|e| format!("{} に書けない: {e}", path.display()))
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
        let lay = Layout { pos: Default::default(), frames: vec![], paths: vec![], size: (0.0, 0.0), bundles: vec![], keep: None };
        let d = data(&st, &view, &lay, None, "a", &HashMap::new(), false, false);
        assert!(d.get("future").is_none()); // 構想が無ければ、前と同じ中身
        assert_eq!(d["nodes"].as_array().unwrap().len(), 1);
        assert!(d["drift"].get("b").is_none());
        let out = html(&d);
        assert!(!out.contains("/*DATA*/"));
        assert!(!out.contains("\"</script>\""));
        assert!(out.contains("<title>あんどん a</title>"));
        assert!(default_path(&st, "", false).ends_with("andon-all-202610141747.html"));
    }

    #[test]
    fn 共有用はai_ブランチ_ずれ_確かめた結果を抜く() {
        use crate::model::{Detail, Edge, Pr, Work};
        let pr = Pr { number: 7, branch: "feat/secret".into(), ..Default::default() };
        let n = Node {
            id: "a:x".into(),
            project: "a".into(),
            zone: "z".into(),
            switch_note: "本番に SECRET_KEY が無い".into(),
            detail: Detail { ai: "秘密の依頼文".into(), reason: "なぜ".into(), main_pr: Some(pr.clone()), prs: vec![pr], ..Default::default() },
            ..Default::default()
        };
        let e = Edge { from: "a:x".into(), to: "a:x".into(), note: "線の秘密メモ".into(), ..Default::default() };
        let mut a = Arch { zones: vec![Zone { id: "z".into(), ..Default::default() }], nodes: vec![n], edges: vec![e], ..Default::default() };
        a.drift.insert("a".into(), crate::model::Drift { unclaimed: vec!["src/leak.ts".into()], ..Default::default() });
        let st = State { generated_at: "2026-10-14T17:47:34+09:00".into(), arch: Some(a.clone()), ..Default::default() };
        let view = crate::model::view_for(&a, "a", None);
        let lay = Layout { pos: Default::default(), frames: vec![], paths: vec![], size: (0.0, 0.0), bundles: vec![], keep: None };
        let working: HashMap<String, Work> = [("a:x".to_string(), Work { branch: "feat/secret".into(), focused: true })].into_iter().collect();
        let out = html(&data(&st, &view, &lay, None, "a", &working, true, false));
        for gone in ["秘密の依頼文", "feat/secret", "SECRET_KEY", "線の秘密メモ", "src/leak.ts"] {
            assert!(!out.contains(gone), "{gone} が残っている");
        }
        assert!(out.contains("なぜ"));
        assert!(default_path(&st, "a", true).ends_with("andon-a-202610141747-share.html"));
    }

    #[test]
    fn 完成図は今の部品を_id_だけにして入れ_共有用でも構想の出所は残す() {
        use crate::model::{Detail, Future};
        let n = Node { id: "a:x".into(), project: "a".into(), zone: "z".into(), detail: Detail { ai: "秘密の依頼文".into(), ..Default::default() }, ..Default::default() };
        let f = Node { id: "a:later".into(), project: "a".into(), zone: "z".into(), future: true, source: "docs/構想.md#あとで".into(), ..Default::default() };
        let a = Arch {
            zones: vec![Zone { id: "z".into(), ..Default::default() }],
            nodes: vec![n],
            future: Future { nodes: vec![f], ..Default::default() },
            ..Default::default()
        };
        let st = State { arch: Some(a.clone()), ..Default::default() };
        let (now, full) = (crate::model::view_mode(&a, "a", None, false), crate::model::view_mode(&a, "a", None, true));
        let lay = Layout { pos: Default::default(), frames: vec![], paths: vec![], size: (0.0, 0.0), bundles: vec![], keep: None };
        let d = data(&st, &now, &lay, Some((&full, &lay)), "a", &HashMap::new(), false, true);
        assert_eq!(d["future"]["nodes"][0], json!("a:x"));
        assert_eq!(d["future"]["nodes"][1]["source"], json!("docs/構想.md#あとで"));
        assert_eq!(d["start_full"], json!(true));
        let out = html(&data(&st, &now, &lay, Some((&full, &lay)), "a", &HashMap::new(), true, false));
        assert!(!out.contains("秘密の依頼文"));
        assert!(out.contains("docs/構想.md#あとで"));
    }
}
