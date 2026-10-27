//! Registration of an enabled Kimi configuration; view state lives in conversation_view.
use super::*;
use crate::kimi::KimiConfig;
impl TanglewoodUi {
    pub fn with_kimi(mut self, config: KimiConfig) -> Self {
        self.app
            .register_conversation_provider(crate::model_catalog::ProviderConfig::Kimi(
                config.clone(),
            ));
        if self.conversation.is_none() {
            let record = crate::conversation_runtime::ConversationRecord::new_kimi(config);
            match self.app.insert_conversation(record) {
                Ok(id) => self.conversation = Some(conversation_view::ConversationView::new(id)),
                Err(error) => self.error = Some(error),
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation_runtime::{ConversationMessage, Submission};
    #[test]
    fn submissions_and_open_failures_keep_only_the_current_draft() {
        let config = KimiConfig {
            executable: "kimi".into(),
            cwd: "/tmp/kimi".into(),
            mcp_bridge: "/tmp/bridge".into(),
            socket: "/tmp/socket".into(),
            resume_session: None,
        };
        let mut record = crate::conversation_runtime::ConversationRecord::new_kimi(config);
        record.thread = Some("session".into());
        record.ready = true;
        record.kimi.as_mut().unwrap().confirmed_session = Some("session".into());
        let prepare = |record: &mut crate::conversation_runtime::ConversationRecord,
                       id: &str,
                       text: &str| {
            record.prompt = text.into();
            record.context_data.draft.attachments = vec![crate::conversation::PromptAttachment {
                name: "source".into(),
                text: "snapshot β".into(),
            }];
            record.kimi.as_mut().unwrap().submission = Some(Submission {
                session: "session".into(),
                id: id.into(),
                text: text.into(),
                attachments: record.context_data.draft.attachments.clone(),
            });
            record.messages.push(ConversationMessage {
                role: "You",
                key: Some(("session".into(), id.into(), "user".into())),
                text: text.into(),
                command: None,
                activity: None,
                uncertain: false,
            });
            record.starting = true;
        };
        for (id, text) in [("one", "first"), ("two", "second")] {
            prepare(&mut record, id, text);
            record.active_turn = Some(id.into());
            record.kimi_turn_finished("session".into(), id.into(), id.into(), "end_turn".into());
            assert!(record.prompt.is_empty());
        }
        prepare(&mut record, "three", "unconfirmed");
        record.kimi_turn_failed(
            "session".into(),
            "".into(),
            "stale".into(),
            "ignored".into(),
        );
        assert!(record.starting);
        record.kimi_turn_failed(
            "session".into(),
            "".into(),
            "three".into(),
            "rejected before turn".into(),
        );
        assert!(!record.starting);
        assert_eq!(record.prompt, "unconfirmed");
        assert_eq!(record.context_data.draft.attachments[0].text, "snapshot β");
        assert_eq!(
            record
                .messages
                .iter()
                .map(|m| m.text.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        prepare(&mut record, "four", "cancelled draft");
        record.fresh_thread = true;
        record.active_turn = Some("turn-four".into());
        record.kimi_turn_finished(
            "session".into(),
            "turn-four".into(),
            "four".into(),
            "cancelled".into(),
        );
        assert_eq!(record.prompt, "cancelled draft");
        assert!(!record.fresh_thread);
        assert_eq!(record.context_data.draft.attachments[0].text, "snapshot β");
        assert_eq!(
            record
                .messages
                .iter()
                .map(|m| m.text.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        record.kimi.as_mut().unwrap().confirmed_session = None;
        record.pending_thread = Some(Some("session".into()));
        record.kimi_open_failed("reload failed".into());
        assert!(!record.ready);
        assert_eq!(record.prompt, "cancelled draft");
        record.kimi.as_mut().unwrap().confirmed_session = Some("session".into());
        record.kimi_open_failed("other session failed".into());
        assert!(record.ready);
    }
    #[test]
    fn reconnect_reopens_fresh_session_and_preserves_its_draft() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::{Duration, Instant};
        let directory =
            std::env::temp_dir().join(format!("tw-kimi-reopen-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let script = directory.join("fake-kimi");
        std::fs::write(&script, r##"#!/usr/bin/env python3
import json, os, sys
for line in sys.stdin:
    frame = json.loads(line)
    method = frame.get('method')
    if method == 'initialize':
        result = {'protocolVersion':1,'agentInfo':{'name':'Kimi Code CLI','version':'2.2.0'},'agentCapabilities':{'loadSession':True,'sessionCapabilities':{'list':{}}}}
    elif method == 'session/list':
        result = {'sessions':[{'sessionId':'same','cwd':os.getcwd(),'title':'Existing'}]}
    elif method == 'session/load':
        open('loaded', 'w').close()
        notification = {'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'same','update':{'sessionUpdate':'user_message_chunk','content':{'type':'text','text':'earlier'}}}}
        print(json.dumps(notification), flush=True)
        result = {'configOptions':[]}
    else:
        result = {}
    if 'id' in frame:
        print(json.dumps({'jsonrpc':'2.0','id':frame['id'],'result':result}), flush=True)
"##).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = KimiConfig {
            executable: script,
            cwd: directory.clone(),
            mcp_bridge: "/tmp/bridge".into(),
            socket: "/tmp/socket".into(),
            resume_session: Some("same".into()),
        };
        let mut record = crate::conversation_runtime::ConversationRecord::new_kimi(config);
        record.thread = Some("same".into());
        record.fresh_thread = true;
        record.prompt = "retained prompt".into();
        record
            .context_data
            .draft
            .attachments
            .push(crate::conversation::PromptAttachment {
                name: "source".into(),
                text: "retained snapshot β".into(),
            });
        let mut app = App::new();
        let id = app.insert_conversation(record).unwrap();
        app.connect_conversation(id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.conversation(id).unwrap().ready {
            app.tick();
            if let Some(error) = &app.conversation(id).unwrap().error {
                panic!("reopen failed: {error}");
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let record = app.conversation(id).unwrap();
        assert!(directory.join("loaded").is_file());
        assert_eq!(record.thread.as_deref(), Some("same"));
        assert_eq!(record.prompt, "retained prompt");
        assert_eq!(
            record.context_data.draft.attachments[0].text,
            "retained snapshot β"
        );
        assert_eq!(record.messages[0].text, "earlier");
        assert!(!record.fresh_thread);
        let generation = record.client_generation;
        app.execute_conversation(crate::conversation_runtime::ConversationAction {
            chat_id: id,
            expected_generation: Some(generation),
            kind: crate::conversation_runtime::ConversationActionKind::Disconnect,
        })
        .unwrap();
        while !app.begin_conversation_shutdown() {
            app.tick();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
