//! VS Code's local Agent Host Protocol (AHP 1.1). Tokens stay in memory and
//! never appear in command arguments, logs, settings, or error messages.
use serde_json::{json, Value};
use std::{
    os::unix::net::UnixStream,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};

const LIMIT: usize = 4 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

fn code(args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = Command::new("code");
    command.args(args);
    let run = |command: &mut Command| {
        prmarmot_core::process::run_bounded(command, Duration::from_secs(15), LIMIT, true)
    };
    let result = run(&mut command);
    #[cfg(target_os = "macos")]
    let result = if result
        .as_ref()
        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
    {
        run(
            Command::new("/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code")
                .args(args),
        )
    } else {
        result
    };
    let output = result
        .map_err(|_| "Cannot run VS Code; install its code command")?
        .ok_or("VS Code command timed out")?;
    if !output.status.success() || output.stdout.len() >= LIMIT {
        return Err("VS Code command failed; a version supporting code --agents and code agent endpoints is required".into());
    }
    Ok(output.stdout)
}

pub struct ReviewAgent {
    socket: WebSocket<UnixStream>,
    next_id: u64,
    provider: String,
    model: String,
}

impl ReviewAgent {
    /// Read-only preflight before Git creates anything. Only local editor hosts
    /// are eligible; never select a remote endpoint or a standalone host.
    pub fn connect(harness: &str, model: &str) -> Result<Self, String> {
        if harness.trim().is_empty() || model.trim().is_empty() {
            return Err(
                "Set Default agent harness and Default agent model in Settings first".into(),
            );
        }
        let mut endpoints = endpoints()?;
        if endpoints.is_empty() {
            code(&["--agents"])?;
            let deadline = Instant::now() + Duration::from_secs(10);
            while endpoints.is_empty() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(250));
                endpoints = self::endpoints()?;
            }
        }
        if endpoints.len() != 1 {
            return Err("Open one local VS Code instance with Agents enabled, then try again (no unique local editor host found)".into());
        }
        let endpoint = &endpoints[0];
        if endpoint["protocolVersion"] != "1.1.0" {
            return Err(
                "Unsupported VS Code Agents protocol; this integration supports AHP 1.1.0".into(),
            );
        }
        let path = endpoint["endpoint"]["path"]
            .as_str()
            .ok_or("VS Code returned no local socket")?;
        let token = endpoint["connectionToken"]
            .as_str()
            .ok_or("VS Code returned no local connection token")?;
        let stream = UnixStream::connect(path)
            .map_err(|_| "Cannot connect to VS Code Agents; reopen its Agents window")?;
        stream
            .set_read_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        let mut url = url::Url::parse("ws://localhost/").unwrap();
        url.query_pairs_mut().append_pair("tkn", token);
        let config = tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(LIMIT))
            .max_frame_size(Some(LIMIT));
        let (socket, _) =
            tungstenite::client::client_with_config(url.as_str(), stream, Some(config))
                .map_err(|_| "VS Code Agents authentication or WebSocket handshake failed")?;
        let mut client = Self {
            socket,
            next_id: 0,
            provider: String::new(),
            model: String::new(),
        };
        let init = client.request(
            "initialize",
            json!({
                "clientId": format!("prmarmot-{}", uuid::Uuid::new_v4()),
                "protocolVersions": ["1.1.0"], "initialSubscriptions": ["ahp-root://"]
            }),
        )?;
        let agents = init["snapshots"]
            .as_array()
            .and_then(|a| a.iter().find(|s| s["resource"] == "ahp-root://"))
            .and_then(|s| s["state"]["agents"].as_array())
            .ok_or("VS Code returned no harness catalog")?;
        (client.provider, client.model) = select_model(agents, harness, model)?;
        Ok(client)
    }

    pub fn start_review(
        mut self,
        path: &Path,
        pr_url: &str,
        prompt: &str,
        permissions: &str,
    ) -> Result<(), String> {
        let id = uuid::Uuid::new_v4().to_string();
        let channel = format!("ahp-session:/{id}");
        let link = format!("vscode://agents/agent-host-session/ahp-session/{id}");
        let directory = url::Url::from_directory_path(path)
            .map_err(|_| "Invalid review worktree path")?
            .to_string();
        let mut config = json!({"isolation":"worktree", "pullRequestUrl":pr_url});
        let resolved = self.request(
            "resolveSessionConfig",
            json!({
                "provider":self.provider, "workingDirectory":directory, "config":config
            }),
        )?;
        if resolved["values"]["isolation"] != "worktree"
            || resolved["values"]["pullRequestUrl"] != pr_url
        {
            return Err("This VS Code harness does not support PR worktree sessions for the selected repository".into());
        }
        if let Some((key, value)) = permission_selection(&self.provider, permissions, &resolved)? {
            config[&key] = json!(value);
            let effective = self.request(
                "resolveSessionConfig",
                json!({
                    "provider":self.provider, "workingDirectory":directory, "config":config
                }),
            )?;
            if effective["values"][&key] != config[&key] {
                return Err(
                    "VS Code did not accept Default agent permissions; no chat was created".into(),
                );
            }
        }
        // Match New Session from Pull Request: the parent repository is the
        // workspace; VS Code owns PR checkout, isolation and worktree identity.
        self.request(
            "createSession",
            json!({"channel": channel, "provider": self.provider,
            "workingDirectories": [directory], "config": config}),
        )
        .map_err(|e| format!("{e}. Parent repository: {}", path.display()))?;
        let result = self
            .send_review(&channel, pr_url, prompt)
            .and_then(|()| self.wait_until_ready(&channel));
        // Reveal even on a send failure so the user can inspect/recover the
        // already-created session. Never retry a potentially accepted prompt.
        let opened = code(&["--agents", "--open-url", &link]);
        result.map_err(|e| {
            format!(
                "{e}. Session: {link}; repository: {}. Check the session before retrying",
                path.display()
            )
        })?;
        opened.map_err(|e| format!("Session created, but {e}. Open {link}"))?;
        Ok(())
    }

    fn send_review(&mut self, channel: &str, pr_url: &str, prompt: &str) -> Result<(), String> {
        let session = self.request("subscribe", json!({"channel": channel}))?;
        let chat = session["snapshot"]["state"]["defaultChat"]
            .as_str()
            .ok_or("VS Code returned no default chat")?
            .to_owned();
        self.request("subscribe", json!({"channel": chat}))?;
        let turn = uuid::Uuid::new_v4().to_string();
        let action = json!({"type":"chat/turnStarted", "turnId":turn,
            "startedAt":chrono::Utc::now().to_rfc3339(),
            "message":initial_message(pr_url, prompt, &self.model)});
        self.send(
            json!({"jsonrpc":"2.0", "method":"dispatchAction", "params":{
                "channel":chat, "clientSeq":1, "action":action
            }}),
        )?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let message = self.receive(deadline)?;
            let envelope = &message["params"];
            if message["method"] == "action"
                && envelope["channel"] == chat
                && envelope["action"]["type"] == action["type"]
                && envelope["action"]["turnId"] == turn
            {
                if let Some(reason) = envelope["rejectionReason"].as_str() {
                    return Err(format!("VS Code rejected the review: {reason}"));
                }
                return Ok(());
            }
        }
    }

    /// A dispatch receipt alone does not mean the SDK/worktree is ready or
    /// that the session is listed. Wait before handing the link to the UI.
    fn wait_until_ready(&mut self, channel: &str) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let snapshot = self.request("subscribe", json!({"channel": channel}))?;
            let state = &snapshot["snapshot"]["state"];
            match state["lifecycle"].as_str() {
                Some("ready") => return Ok(()),
                Some("creationFailed") => {
                    return Err(format!(
                        "VS Code could not initialize the PR session: {}",
                        state["error"]
                    ))
                }
                _ if Instant::now() >= deadline => {
                    return Err(
                        "Timed out waiting for VS Code to prepare the PR worktree and session"
                            .into(),
                    )
                }
                _ => std::thread::sleep(Duration::from_millis(250)),
            }
        }
    }

    fn send(&mut self, value: Value) -> Result<(), String> {
        self.socket
            .send(Message::Text(value.to_string().into()))
            .map_err(|_| "VS Code Agents connection closed".into())
    }

    fn receive(&mut self, deadline: Instant) -> Result<Value, String> {
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("VS Code Agents response timed out")?;
            self.socket
                .get_mut()
                .set_read_timeout(Some(remaining))
                .map_err(|e| e.to_string())?;
            match self
                .socket
                .read()
                .map_err(|_| "VS Code Agents response unavailable or timed out")?
            {
                Message::Text(text) => {
                    return serde_json::from_str(&text)
                        .map_err(|_| "Invalid VS Code Agents response".into())
                }
                Message::Close(_) => return Err("VS Code Agents connection closed".into()),
                _ => self
                    .socket
                    .flush()
                    .map_err(|_| "VS Code Agents connection closed")?,
            }
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}))?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let response = self.receive(deadline)?;
            if response["id"] == id {
                if let Some(error) = response.get("error") {
                    return Err(format!(
                        "VS Code {method}: {}",
                        error["message"].as_str().unwrap_or("request rejected")
                    ));
                }
                return response
                    .get("result")
                    .cloned()
                    .ok_or("Invalid VS Code Agents response".into());
            }
        }
    }
}

/// Permission IDs are harness-specific. Validate against the live schema so
/// policy restrictions and unsupported values cannot silently fall back.
fn permission_selection(
    provider: &str,
    selection: &str,
    resolved: &Value,
) -> Result<Option<(String, String)>, String> {
    let selection = selection.trim();
    if selection.is_empty() {
        return Ok(None);
    }
    let key = match provider {
        "codex" => "codex.permissionsPreset",
        "claude" => "permissionMode",
        "copilotcli" => "autoApprove",
        _ => {
            return Err(format!(
                "Default agent permissions is not supported for {provider}"
            ))
        }
    };
    let property = &resolved["schema"]["properties"][key];
    let allowed = property["enum"]
        .as_array()
        .is_some_and(|values| values.iter().any(|v| v == selection));
    if !allowed || (property["readOnly"] == true && resolved["values"][key] != selection) {
        return Err(format!("Default agent permissions '{selection}' is unavailable for {provider}. Allowed values: {}", property["enum"]));
    }
    Ok(Some((key.into(), selection.into())))
}

fn initial_message(pr_url: &str, prompt: &str, model: &str) -> Value {
    if prompt.trim().is_empty() {
        // Native PR creation also sends a hidden setup turn. Unlike that UI,
        // we have no full PR snapshot, so do not claim one was attached.
        json!({"text":format!("<!-- vscode-hidden-from-transcript -->\nInitialize this session for pull request {pr_url}. Do not inspect or modify files, use tools, fetch pull request data, or take any other action until the user sends a visible follow-up request. Reply only with \"Ready\"."),
            "origin":{"kind":"user"}, "model":{"id":model},
            "_meta":{"vscode.chat.hiddenFromTranscript":true}})
    } else {
        json!({"text":format!("{}\n\nPull request: {pr_url}", prompt.trim()),
            "origin":{"kind":"user"}, "model":{"id":model}})
    }
}

fn endpoints() -> Result<Vec<Value>, String> {
    let bytes = code(&["agent", "endpoints"])?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "VS Code returned invalid endpoint metadata")?;
    Ok(value["endpoints"]
        .as_array()
        .ok_or("VS Code returned no endpoint list")?
        .iter()
        .filter(|e| e["type"] == "editor" && e["endpoint"]["type"] == "socket")
        .cloned()
        .collect())
}

fn select_model(agents: &[Value], harness: &str, model: &str) -> Result<(String, String), String> {
    let harness = harness.trim();
    let matching: Vec<_> = agents
        .iter()
        .filter(|a| {
            ["provider", "displayName"].iter().any(|key| {
                a[key]
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case(harness))
            })
        })
        .collect();
    if matching.len() != 1 {
        return Err(format!(
            "Harness '{harness}' is unavailable. Choose an installed local VS Code harness"
        ));
    }
    let agent = matching[0];
    let models = agent["models"]
        .as_array()
        .ok_or("Harness returned no models")?;
    let model = model.trim();
    // Prefer exact IDs. Friendly names and unqualified Codex IDs are accepted
    // only if unambiguous across the selected harness's model sources.
    let exact = models.iter().find(|m| m["id"] == model);
    let matching: Vec<_> = models
        .iter()
        .filter(|m| {
            m["name"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(model))
                || m["id"]
                    .as_str()
                    .and_then(|s| s.strip_prefix("@provider="))
                    .and_then(|s| s.split_once(':'))
                    .is_some_and(|(_, id)| id == model)
        })
        .collect();
    let selected = exact.or_else(|| (matching.len() == 1).then(|| matching[0]))
        .ok_or_else(|| format!("Model '{model}' is unavailable or ambiguous for {harness}. Use its exact model ID from VS Code"))?;
    Ok((
        agent["provider"]
            .as_str()
            .ok_or("Invalid harness ID")?
            .into(),
        selected["id"].as_str().ok_or("Invalid model ID")?.into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permissions_follow_harness_schema_and_do_not_fall_back() {
        for (provider, key, value) in [
            ("codex", "codex.permissionsPreset", "full-access"),
            ("claude", "permissionMode", "acceptEdits"),
            ("copilotcli", "autoApprove", "autoApprove"),
        ] {
            let mut schema = json!({"schema":{"properties":{}}, "values":{}});
            schema["schema"]["properties"][key] = json!({"enum":["default", value]});
            schema["values"][key] = json!("default");
            assert_eq!(
                permission_selection(provider, value, &schema).unwrap(),
                Some((key.into(), value.into()))
            );
            assert_eq!(permission_selection(provider, "  ", &schema).unwrap(), None);
            assert!(permission_selection(provider, "typo", &schema).is_err());
            schema["schema"]["properties"][key]["readOnly"] = json!(true);
            assert!(permission_selection(provider, value, &schema).is_err());
            assert!(permission_selection(provider, "default", &schema).is_ok());
        }
        assert!(permission_selection("unknown", "full-access", &json!({})).is_err());
    }

    #[test]
    fn blank_prompt_initializes_without_requesting_work() {
        for prompt in ["", "  "] {
            let message = initial_message("https://github.test/a/b/pull/1", prompt, "chosen-model");
            assert_eq!(message["_meta"]["vscode.chat.hiddenFromTranscript"], true);
            assert_eq!(message["model"]["id"], "chosen-model");
            let text = message["text"].as_str().unwrap();
            assert!(text.starts_with("<!-- vscode-hidden-from-transcript -->\n"));
            assert!(text.contains("Do not inspect or modify files, use tools"));
            assert!(text.contains("https://github.test/a/b/pull/1"));
            assert!(!text.contains("attached snapshot"));
        }
    }

    #[test]
    fn ready_check_waits_for_materialization_and_reports_failure() {
        for lifecycle in ["ready", "creationFailed"] {
            let (client, server) = UnixStream::pair().unwrap();
            let worker = std::thread::spawn(move || {
                let mut socket = tungstenite::accept(server).unwrap();
                for state in ["creating", lifecycle] {
                    let request: Value =
                        serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                    assert_eq!(request["method"], "subscribe");
                    socket.send(Message::Text(json!({"jsonrpc":"2.0", "id":request["id"],
                        "result":{"snapshot":{"state":{"lifecycle":state,"error":"test setup failure"}}}}).to_string().into())).unwrap();
                }
            });
            let (socket, _) = tungstenite::client("ws://localhost/", client).unwrap();
            let mut agent = ReviewAgent {
                socket,
                next_id: 0,
                provider: "codex".into(),
                model: "test".into(),
            };
            assert_eq!(
                agent.wait_until_ready("ahp-session:/test").is_ok(),
                lifecycle == "ready"
            );
            worker.join().unwrap();
        }
    }

    #[test]
    fn review_sends_selected_model_and_waits_for_acceptance() {
        for (rejected, prompt) in [false, true].into_iter().flat_map(|r| {
            ["/code-review", "Summarize risks"]
                .into_iter()
                .map(move |p| (r, p))
        }) {
            let (client, server) = UnixStream::pair().unwrap();
            let worker = std::thread::spawn(move || {
                let mut socket = tungstenite::accept(server).unwrap();
                for (channel, state) in [
                    ("ahp-session:/test", json!({"defaultChat":"ahp-chat:/test"})),
                    ("ahp-chat:/test", json!({})),
                ] {
                    let request: Value =
                        serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                    assert_eq!(request["method"], "subscribe");
                    assert_eq!(request["params"]["channel"], channel);
                    socket
                        .send(Message::Text(
                            json!({"jsonrpc":"2.0","id":request["id"],
                        "result":{"snapshot":{"state":state}}})
                            .to_string()
                            .into(),
                        ))
                        .unwrap();
                }
                let request: Value =
                    serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                assert_eq!(request["method"], "dispatchAction");
                assert_eq!(request["params"]["channel"], "ahp-chat:/test");
                let action = &request["params"]["action"];
                assert_eq!(action["type"], "chat/turnStarted");
                assert_eq!(action["message"]["model"]["id"], "exact-model");
                assert_eq!(
                    action["message"]["text"],
                    format!("{prompt}\n\nPull request: https://github.test/a/b/pull/1")
                );
                let mut envelope = request["params"].clone();
                if rejected {
                    envelope["rejectionReason"] = json!("test rejection");
                }
                socket
                    .send(Message::Text(
                        json!({"jsonrpc":"2.0","method":"action","params":envelope})
                            .to_string()
                            .into(),
                    ))
                    .unwrap();
            });
            let (socket, _) = tungstenite::client("ws://localhost/", client).unwrap();
            let mut agent = ReviewAgent {
                socket,
                next_id: 0,
                provider: "codex".into(),
                model: "exact-model".into(),
            };
            let result = agent.send_review(
                "ahp-session:/test",
                "https://github.test/a/b/pull/1",
                prompt,
            );
            assert_eq!(result.is_err(), rejected);
            worker.join().unwrap();
        }
    }

    #[test]
    #[ignore = "read-only check against a running local VS Code Agents host"]
    fn live_local_agent_preflight() {
        let harness = std::env::var("PRMARMOT_TEST_HARNESS").unwrap();
        let model = std::env::var("PRMARMOT_TEST_MODEL").unwrap();
        let agent = ReviewAgent::connect(&harness, &model).unwrap();
        assert!(!agent.model.is_empty());
    }

    #[test]
    fn model_selection_is_scoped_and_unambiguous() {
        let agents = vec![json!({"provider":"codex", "displayName":"Codex", "models":[
            {"id":"@provider=openai:test", "name":"Test"}, {"id":"@provider=other:test", "name":"Test"}]})];
        assert!(select_model(&agents, "claude", "Test").is_err());
        assert!(select_model(&agents, "codex", "test").is_err());
        assert_eq!(
            select_model(&agents, "Codex", "@provider=openai:test")
                .unwrap()
                .1,
            "@provider=openai:test"
        );
    }
}
