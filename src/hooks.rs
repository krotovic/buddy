//! The local server Claude Code hooks post to.
//!
//! Listens on 127.0.0.1 only. Every request must carry the token from the
//! settings in the `X-Buddy-Token` header, so other local programs cannot
//! post fake events. The body is the hook input JSON Claude Code sends.

use std::io::Read;

use serde::Deserialize;

use crate::pending::Event;

pub const PATH: &str = "/buddy/hook";
/// Notification types that mean Claude is waiting for you.
pub const NEEDS_YOU_NOTIFICATIONS: &str =
    "permission_prompt|elicitation_dialog|elicitation_url_dialog|agent_needs_input";
pub const TOKEN_HEADER: &str = "X-Buddy-Token";
const MAX_BODY: u64 = 1 << 20;

#[derive(Debug, Deserialize)]
struct HookInput {
    hook_event_name: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    notification_type: Option<String>,
}

/// Turn a hook input into an event for the pending list. None for events
/// the cube does not react to.
pub fn parse_event(body: &str) -> Option<Event> {
    let input: HookInput = serde_json::from_str(body).ok()?;
    let session = input.session_id;
    match input.hook_event_name.as_str() {
        "Notification" => match input.notification_type.as_deref() {
            // The installed matcher already limits this; the check keeps
            // other hook setups from turning every message into Needs you.
            // idle_prompt comes a minute after Stop and would turn every
            // Done into Needs you.
            None => Some(Event::NeedsYou(session)),
            Some(t) if NEEDS_YOU_NOTIFICATIONS.split('|').any(|n| n == t) => {
                Some(Event::NeedsYou(session))
            }
            Some(_) => None,
        },
        "Stop" => Some(Event::Done(session)),
        "UserPromptSubmit" | "SessionEnd" => Some(Event::Clear(session)),
        _ => None,
    }
}

/// A random token for a new install.
pub fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    // RandomState is seeded from the OS; two of them give 128 bits.
    (0..2)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i);
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            );
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// Answer for one request: status code and the event, if any.
pub fn handle(
    method: &str,
    url: &str,
    token: Option<&str>,
    expected: &str,
    body: &str,
) -> (u16, Option<Event>) {
    if url.split('?').next() != Some(PATH) {
        return (404, None);
    }
    if method != "POST" {
        return (405, None);
    }
    if expected.is_empty() || token != Some(expected) {
        return (401, None);
    }
    (200, parse_event(body))
}

/// Start the server on a background thread. Events go to `on_event`.
pub fn spawn(
    port: u16,
    token: String,
    on_event: impl Fn(Event) + Send + 'static,
) -> anyhow::Result<()> {
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow::anyhow!("hook server on 127.0.0.1:{port}: {e}"))?;
    log::info!("hook server on http://127.0.0.1:{port}{PATH}");
    std::thread::Builder::new()
        .name("hook-server".into())
        .spawn(move || {
            for mut req in server.incoming_requests() {
                let mut body = String::new();
                let read = req.as_reader().take(MAX_BODY).read_to_string(&mut body);
                let sent = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv(TOKEN_HEADER))
                    .map(|h| h.value.as_str().to_owned());
                let (status, event) = match read {
                    Ok(_) => handle(
                        req.method().as_str(),
                        req.url(),
                        sent.as_deref(),
                        &token,
                        &body,
                    ),
                    Err(_) => (400, None),
                };
                if status != 200 {
                    log::warn!("hook request {} {}: {status}", req.method(), req.url());
                }
                if let Some(event) = event {
                    log::info!("hook event {event:?}");
                    on_event(event);
                }
                let response =
                    tiny_http::Response::from_string(if status == 200 { "{}" } else { "" })
                        .with_status_code(status)
                        .with_header(
                            "Content-Type: application/json"
                                .parse::<tiny_http::Header>()
                                .expect("static header"),
                        );
                let _ = req.respond(response);
            }
        })?;
    Ok(())
}

/// The hooks Buddy adds to Claude Code: event, matcher.
const HOOKS: [(&str, Option<&str>); 4] = [
    ("Notification", Some(NEEDS_YOU_NOTIFICATIONS)),
    ("Stop", None),
    ("UserPromptSubmit", None),
    ("SessionEnd", None),
];

/// `~/.claude/settings.json`, the user-level Claude Code settings.
pub fn claude_settings_path() -> Option<std::path::PathBuf> {
    directories::BaseDirs::new().map(|d| d.home_dir().join(".claude").join("settings.json"))
}

fn is_ours(handler: &serde_json::Value) -> bool {
    handler
        .get("url")
        .and_then(|u| u.as_str())
        .is_some_and(|u| u.starts_with("http://127.0.0.1:") && u.ends_with(PATH))
}

/// Remove Buddy's hooks from Claude Code settings JSON, leaving every
/// other hook as it was. Groups left empty are dropped.
pub fn remove_hooks(settings: &mut serde_json::Value) {
    let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return;
    };
    for groups in hooks.values_mut() {
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        for group in groups.iter_mut() {
            if let Some(list) = group.get_mut("hooks").and_then(|l| l.as_array_mut()) {
                list.retain(|h| !is_ours(h));
            }
        }
        groups.retain(|g| {
            g.get("hooks")
                .and_then(|l| l.as_array())
                .is_none_or(|l| !l.is_empty())
        });
    }
    hooks.retain(|_, groups| groups.as_array().is_none_or(|g| !g.is_empty()));
}

/// Add Buddy's hooks (replacing older ones, for example with another port
/// or token) to Claude Code settings JSON.
pub fn add_hooks(settings: &mut serde_json::Value, port: u16, token: &str) -> anyhow::Result<()> {
    use serde_json::{Value, json};
    if !settings.is_object() {
        *settings = json!({});
    }
    remove_hooks(settings);
    let root = settings.as_object_mut().expect("object");
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("\"hooks\" in Claude Code settings is not an object"))?;
    for (event, matcher) in HOOKS {
        let mut group = json!({
            "hooks": [{
                "type": "http",
                "url": format!("http://127.0.0.1:{port}{PATH}"),
                "headers": { TOKEN_HEADER: token },
                // Fail fast when Buddy is not running.
                "timeout": 5
            }]
        });
        if let Some(m) = matcher {
            group["matcher"] = Value::String(m.into());
        }
        let groups = hooks.entry(event).or_insert_with(|| json!([]));
        groups
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("hooks.{event} is not a list"))?
            .push(group);
    }
    Ok(())
}

/// Install (or with `install == false`, remove) Buddy's hooks in the
/// Claude Code settings file at `path`. The previous file is kept next to
/// it as `settings.json.buddy-backup`.
pub fn update_settings_file(
    path: &std::path::Path,
    install: bool,
    port: u16,
    token: &str,
) -> anyhow::Result<()> {
    let mut settings: serde_json::Value = match std::fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => serde_json::from_str(&text).map_err(|e| {
            anyhow::anyhow!("{} is not valid JSON, not touching it: {e}", path.display())
        })?,
        Ok(_) => serde_json::json!({}),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(e) => return Err(e.into()),
    };
    if path.exists() {
        std::fs::copy(path, path.with_extension("json.buddy-backup"))?;
    }
    if install {
        add_hooks(&mut settings, port, token)?;
    } else {
        remove_hooks(&mut settings);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.buddy-tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&settings)? + "\n")?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Whether the settings at `path` hold Buddy's hooks for this port and
/// token.
pub fn installed(path: &std::path::Path, port: u16, token: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(settings) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    let url = format!("http://127.0.0.1:{port}{PATH}");
    HOOKS.iter().all(|(event, _)| {
        settings["hooks"][event].as_array().is_some_and(|groups| {
            groups.iter().any(|g| {
                g["hooks"].as_array().is_some_and(|l| {
                    l.iter()
                        .any(|h| h["url"] == url.as_str() && h["headers"][TOKEN_HEADER] == token)
                })
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(event: &str, extra: &str) -> String {
        format!(
            r#"{{"session_id":"s1","transcript_path":"/t.jsonl","cwd":"/p","hook_event_name":"{event}"{extra}}}"#
        )
    }

    #[test]
    fn maps_hook_events() {
        let n = body(
            "Notification",
            r#","message":"Claude needs your permission","notification_type":"permission_prompt""#,
        );
        assert_eq!(parse_event(&n), Some(Event::NeedsYou("s1".into())));
        assert_eq!(
            parse_event(&body("Stop", r#","stop_hook_active":false"#)),
            Some(Event::Done("s1".into()))
        );
        assert_eq!(
            parse_event(&body("UserPromptSubmit", r#","prompt":"hi""#)),
            Some(Event::Clear("s1".into()))
        );
        assert_eq!(
            parse_event(&body("SessionEnd", r#","reason":"exit""#)),
            Some(Event::Clear("s1".into()))
        );
        assert_eq!(parse_event(&body("PreToolUse", "")), None);
        assert_eq!(
            parse_event(&body(
                "Notification",
                r#","notification_type":"auth_success""#
            )),
            None
        );
        assert_eq!(
            parse_event(&body(
                "Notification",
                r#","notification_type":"idle_prompt""#
            )),
            None
        );
        assert_eq!(
            parse_event(&body("Notification", "")),
            Some(Event::NeedsYou("s1".into()))
        );
        assert_eq!(parse_event("not json"), None);
    }

    #[test]
    fn requests_need_the_token() {
        let b = body("Stop", "");
        assert_eq!(
            handle("POST", PATH, Some("t"), "t", &b),
            (200, Some(Event::Done("s1".into())))
        );
        assert_eq!(handle("POST", PATH, Some("x"), "t", &b).0, 401);
        assert_eq!(handle("POST", PATH, None, "t", &b).0, 401);
        assert_eq!(handle("POST", PATH, Some(""), "", &b).0, 401);
        assert_eq!(handle("GET", PATH, Some("t"), "t", &b).0, 405);
        assert_eq!(handle("POST", "/other", Some("t"), "t", &b).0, 404);
    }

    #[test]
    fn tokens_are_long_and_different() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn server_round_trip() {
        use std::io::Write;
        let port = 47000 + (std::process::id() % 1000) as u16;
        let (tx, rx) = std::sync::mpsc::channel();
        spawn(port, "tok".into(), move |e| tx.send(e).unwrap()).unwrap();
        let b = body("Notification", "");
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            s,
            "POST {PATH} HTTP/1.1\r\nHost: x\r\n{TOKEN_HEADER}: tok\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
        )
        .unwrap();
        let mut reply = String::new();
        s.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap(),
            Event::NeedsYou("s1".into())
        );
    }

    #[test]
    fn install_keeps_other_hooks_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("buddy-hooks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let theirs = serde_json::json!({
            "model": "opus",
            "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "say done" } ] } ] }
        });
        std::fs::write(&path, theirs.to_string()).unwrap();

        update_settings_file(&path, true, 47821, "old").unwrap();
        update_settings_file(&path, true, 47821, "new").unwrap();
        assert!(installed(&path, 47821, "new"));
        assert!(!installed(&path, 47821, "old"));
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["model"], "opus");
        let stop = v["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2, "their hook plus ours once: {stop:?}");
        assert_eq!(
            v["hooks"]["Notification"][0]["matcher"],
            NEEDS_YOU_NOTIFICATIONS
        );
        assert!(dir.join("settings.json.buddy-backup").exists());

        update_settings_file(&path, false, 47821, "new").unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v, theirs);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn broken_settings_are_left_alone() {
        let dir = std::env::temp_dir().join(format!("buddy-hooks-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ oops").unwrap();
        assert!(update_settings_file(&path, true, 1, "t").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ oops");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
