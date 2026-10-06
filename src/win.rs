//! Windows と herdr まわり: 2 つ目の窓を開かない・開いている窓に知らせる・窓を前に出す・herdr のペインを聞く。
//! ここに、今の窓（arch_view.py）で踏んだ落とし穴への手当てが集まっている（2026-10-06）。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

/// 2 つ目の窓を開かないための口の番号。Python 版（8766）と並べて使えるよう別の番号にした
pub const PORT: u16 = 8767;

/// 開いている窓への知らせ
pub enum Msg {
    /// 前に出るだけ
    Show,
    /// その範囲のタブを足す（あれば切り替える）。空なら全体
    Open(String),
}

fn addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], PORT))
}

/// 決まった番号の口を握る。握れなければ、もう開いている。
/// そのときは開いている窓に msg（"open:<範囲>" か "show"）を知らせて None を返す
/// （裏で立てた窓は Windows が前に出さず、隠れて見えなかったので、開いている窓に「前に出て」と頼む）
pub fn claim(msg: impl FnOnce() -> String) -> Option<TcpListener> {
    if let Ok(l) = TcpListener::bind(addr()) {
        return Some(l);
    }
    let msg = msg();
    log(&format!("開いている窓へ: {msg}")); // タブが増えないときに、押したのが届いているかを確かめるため
    if let Ok(mut c) = TcpStream::connect_timeout(&addr(), Duration::from_secs(2)) {
        let _ = c.write_all(msg.as_bytes());
    }
    None
}

/// 知らせを待つ糸。届いたら tx に送って、窓を描き直させる
pub fn listen(l: TcpListener, tx: mpsc::Sender<Msg>, ctx: egui::Context) {
    std::thread::spawn(move || {
        for conn in l.incoming().flatten() {
            let mut conn = conn;
            let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = vec![0u8; 512];
            let n = conn.read(&mut buf).unwrap_or(0);
            let text = String::from_utf8_lossy(&buf[..n]).to_string();
            let msg = match text.strip_prefix("open:") {
                Some(scope) => Msg::Open(scope.to_string()),
                None => Msg::Show,
            };
            if tx.send(msg).is_err() {
                break;
            }
            ctx.request_repaint();
        }
    });
}

/// ~/.herdr/open-arch.log に 1 行残す（open-arch.ps1 と同じ所）
pub fn log(line: &str) {
    let home = std::env::var_os("USERPROFILE").unwrap_or_default();
    let path = std::path::PathBuf::from(home).join(".herdr").join("open-arch.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} [rust] {line}", now());
    }
}

/// 今の時刻（この PC の時刻で、2026-10-06T19:30:00 の形）
#[cfg(windows)]
fn now() -> String {
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        ms: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(t: *mut SystemTime);
    }
    let mut t = SystemTime::default();
    unsafe { GetLocalTime(&mut t) };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second)
}

#[cfg(not(windows))]
fn now() -> String {
    String::new()
}

/// herdr のペイン: (フォルダ, 今選んでいるか)。herdr が無い・読めなければ空。
/// herdr はコンソールのプログラムなので、CREATE_NO_WINDOW で呼ばないと窓が開いて入力を奪う
pub fn panes() -> Vec<(String, bool)> {
    let mut cmd = std::process::Command::new("herdr");
    crate::layout::hidden(cmd.args(["pane", "list"]).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()));
    let Ok(out) = cmd.output() else { return vec![] };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else { return vec![] };
    v["result"]["panes"]
        .as_array()
        .map(|ps| {
            ps.iter()
                .map(|p| (p["cwd"].as_str().unwrap_or("").to_string(), p["focused"].as_bool().unwrap_or(false)))
                .collect()
        })
        .unwrap_or_default()
}

/// herdr で今選んでいるペインのフォルダ
pub fn focused_cwd() -> Option<String> {
    panes().into_iter().find(|p| p.1).map(|p| p.0)
}

#[cfg(windows)]
mod sys {
    pub type Hwnd = isize;
    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn EnumWindows(f: extern "system" fn(Hwnd, isize) -> i32, l: isize) -> i32;
        pub fn GetWindowThreadProcessId(h: Hwnd, pid: *mut u32) -> u32;
        pub fn IsWindowVisible(h: Hwnd) -> i32;
        pub fn IsIconic(h: Hwnd) -> i32;
        pub fn ShowWindow(h: Hwnd, cmd: i32) -> i32;
        pub fn GetForegroundWindow() -> Hwnd;
        pub fn AttachThreadInput(a: u32, b: u32, attach: i32) -> i32;
        pub fn BringWindowToTop(h: Hwnd) -> i32;
        pub fn SetForegroundWindow(h: Hwnd) -> i32;
        pub fn SetWindowPos(h: Hwnd, after: Hwnd, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetCurrentProcessId() -> u32;
        pub fn GetCurrentThreadId() -> u32;
    }
}

/// この処理の、見えている窓
#[cfg(windows)]
fn my_windows() -> Vec<sys::Hwnd> {
    use std::sync::Mutex;
    static FOUND: Mutex<Vec<isize>> = Mutex::new(vec![]);
    extern "system" fn each(h: sys::Hwnd, me: isize) -> i32 {
        let mut pid = 0u32;
        unsafe { sys::GetWindowThreadProcessId(h, &mut pid) };
        if pid as isize == me && unsafe { sys::IsWindowVisible(h) } != 0 {
            FOUND.lock().unwrap().push(h);
        }
        1
    }
    FOUND.lock().unwrap().clear();
    unsafe { sys::EnumWindows(each, sys::GetCurrentProcessId() as isize) };
    FOUND.lock().unwrap().clone()
}

/// 窓を一番前に出す。いったん「常に手前」にしてすぐ戻す（Windows は裏から立てた窓を前に出させないため）。
/// Windows は、いま前にいない処理からの「前に出して」を断るので、前にいる窓の入力にいったん相乗りしてから頼む
/// （Alt を一瞬押した形にする回避は、窓の動きが止まることがあったのでやめた・2026-10-06）
#[cfg(windows)]
pub fn bring_front() {
    const SW_RESTORE: i32 = 9;
    const HWND_TOPMOST: isize = -1;
    const HWND_NOTOPMOST: isize = -2;
    const SWP_NOMOVE_NOSIZE: u32 = 0x0002 | 0x0001;
    for h in my_windows() {
        unsafe {
            if sys::IsIconic(h) != 0 {
                sys::ShowWindow(h, SW_RESTORE);
            }
            sys::SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE_NOSIZE);
            sys::SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE_NOSIZE);
            let fg = sys::GetWindowThreadProcessId(sys::GetForegroundWindow(), std::ptr::null_mut());
            let me = sys::GetCurrentThreadId();
            if fg != 0 && fg != me {
                sys::AttachThreadInput(me, fg, 1);
                sys::BringWindowToTop(h);
                sys::SetForegroundWindow(h);
                sys::AttachThreadInput(me, fg, 0);
            } else {
                sys::SetForegroundWindow(h);
            }
        }
    }
}

#[cfg(not(windows))]
pub fn bring_front() {}
