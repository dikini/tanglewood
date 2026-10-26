//! Android Editable is an input mirror. Only the focused Rust field accepts its state.
use eframe::egui::{self, text::CCursor, text::CCursorRange};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
struct Snapshot {
    widget: u64,
    epoch: u64,
    seq: u64,
    text: String,
    selection: [i32; 2],
    composing: [i32; 2],
}
struct Session {
    id: egui::Id,
    state: Snapshot,
    pending: Option<Snapshot>,
}
struct Recovery {
    widget: u64,
    base: String,
    text: String,
}
#[derive(Clone, Copy)]
struct Action {
    widget: u64,
    epoch: u64,
    kind: i32,
}
struct Route {
    owner: i64,
    session: Option<Session>,
    publish: Option<Snapshot>,
    show: bool,
    action: Option<Action>,
    edit_target: Option<(u64, u64)>,
    next_epoch: u64,
    interruptions: Vec<Recovery>,
    clipboard: Option<String>,
    error: Option<String>,
    recovery: Option<String>,
    wake: Arc<dyn Fn() + Send + Sync>,
}
fn edit_protects(route: &Route, widget: u64, epoch: u64, popup_open: bool) -> bool {
    route.edit_target == Some((widget, epoch))
        && (popup_open
            || route
                .action
                .is_some_and(|action| (action.widget, action.epoch) == (widget, epoch)))
}
fn allocate_epoch(route: &mut Route) -> Option<u64> {
    match route.next_epoch.checked_add(1) {
        Some(epoch) => {
            route.next_epoch = epoch;
            Some(epoch)
        }
        None => {
            route.error =
                Some("Input generation exhausted; restart input before continuing.".into());
            None
        }
    }
}
fn retire_session(route: &mut Route) -> bool {
    let needs_recovery = route
        .session
        .as_ref()
        .is_some_and(|session| session.pending.is_some() || session.state.composing != [-1, -1]);
    if needs_recovery && route.interruptions.len() >= 8 {
        route.error = Some(
            "Interrupted-input recovery is full; copy retained input before continuing.".into(),
        );
        return false;
    }
    if let Some(mut session) = route.session.take() {
        if needs_recovery {
            let text = session
                .pending
                .take()
                .map_or_else(|| session.state.text.clone(), |pending| pending.text);
            route.recovery = Some(text.clone());
            route.interruptions.push(Recovery {
                widget: session.state.widget,
                base: session.state.text,
                text,
            });
            route.error = Some(
                "Input interrupted; the original field's draft is retained for recovery.".into(),
            );
        }
    }
    true
}
static ROUTE: Mutex<Option<Route>> = Mutex::new(None);

pub fn end_owner(owner: i64) {
    if let Ok(mut route) = ROUTE.lock() {
        if route.as_ref().is_some_and(|route| route.owner == owner) {
            *route = None;
        }
    }
}
/// Return scalar offset only at an exact UTF-16 boundary (never inside a surrogate pair).
fn scalar_offset(text: &str, offset: i32) -> Result<usize, &'static str> {
    let target = usize::try_from(offset).map_err(|_| "negative UTF-16 position")?;
    let mut units = 0;
    for (scalars, character) in text.chars().enumerate() {
        if units == target {
            return Ok(scalars);
        }
        units += character.len_utf16();
        if units > target {
            return Err("UTF-16 position splits a surrogate pair");
        }
    }
    (units == target)
        .then(|| text.chars().count())
        .ok_or("UTF-16 position exceeds input")
}
fn utf16_offset(text: &str, scalar: usize) -> Option<i32> {
    text.chars()
        .take(scalar)
        .map(char::len_utf16)
        .sum::<usize>()
        .try_into()
        .ok()
}
fn validate(update: &Snapshot) -> Result<(), &'static str> {
    scalar_offset(&update.text, update.selection[0])?;
    scalar_offset(&update.text, update.selection[1])?;
    if update.composing != [-1, -1] {
        if update.composing[0] > update.composing[1] {
            return Err("invalid composing range");
        }
        scalar_offset(&update.text, update.composing[0])?;
        scalar_offset(&update.text, update.composing[1])?;
    }
    Ok(())
}
fn accept(session: &mut Session, update: Snapshot) -> Result<(), Snapshot> {
    if update.widget != session.state.widget
        || update.epoch != session.state.epoch
        || update.seq <= session.state.seq
        || session
            .pending
            .as_ref()
            .is_some_and(|pending| update.seq <= pending.seq)
        || validate(&update).is_err()
    {
        return Err(update);
    }
    // Latest complete state includes every previous edit in this epoch, including commit.
    session.pending = Some(update);
    Ok(())
}

#[derive(Default)]
pub struct EditOutcome {
    pub active: bool,
    pub changed: bool,
    pub composing: bool,
    pub finished: bool,
    pub conflict: bool,
}
/// Apply before TextEdit renders, preserving its undo state and exact scalar selection.
pub fn before(ctx: &egui::Context, id: egui::Id, text: &mut String) -> EditOutcome {
    let mut outcome = EditOutcome::default();
    let focused = ctx.memory(|memory| memory.focused());
    let popup_open = egui::Popup::is_any_open(ctx);
    let update = {
        let Ok(mut route) = ROUTE.lock() else {
            return outcome;
        };
        let Some(route) = route.as_mut() else {
            return outcome;
        };
        if let Some(index) = route
            .interruptions
            .iter()
            .position(|recovery| recovery.widget == id.value())
        {
            let recovery = route.interruptions.remove(index);
            outcome.active = true;
            outcome.conflict = true;
            outcome.composing = true;
            if *text == recovery.base {
                outcome.changed = *text != recovery.text;
                *text = recovery.text;
            }
            return outcome;
        }
        let protected_edit = route.session.as_ref().is_some_and(|session| {
            edit_protects(route, session.state.widget, session.state.epoch, popup_open)
        });
        let Some(session) = route
            .session
            .as_mut()
            .filter(|session| session.state.widget == id.value())
        else {
            return outcome;
        };
        outcome.active = true;
        outcome.composing = session.state.composing != [-1, -1];
        let Some(update) = session.pending.take() else {
            return outcome;
        };
        if *text != session.state.text || (focused != Some(id) && !protected_edit) {
            outcome.conflict = true;
            route.error =
                Some("Input changed elsewhere; native input retained for recovery.".into());
            route.recovery = Some(update.text);
            return outcome;
        }
        outcome.changed = *text != update.text;
        outcome.composing = update.composing != [-1, -1];
        outcome.finished = session.state.composing != [-1, -1] && !outcome.composing;
        session.state = update.clone();
        route.publish = Some(update.clone());
        update
    };
    // No egui operation runs while the callback route is locked.
    let primary = scalar_offset(&update.text, update.selection[1]).expect("validated callback");
    let secondary = scalar_offset(&update.text, update.selection[0]).expect("validated callback");
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        let old_range = state.cursor.char_range().unwrap_or_default();
        let mut undo = state.undoer();
        undo.add_undo(&(old_range, text.clone()));
        state.set_undoer(undo);
        state.cursor.set_char_range(Some(CCursorRange {
            primary: CCursor::new(primary),
            secondary: CCursor::new(secondary),
            h_pos: None,
        }));
        state.store(ctx, id);
    }
    *text = update.text;
    outcome
}
/// Publish only the focused field after egui's own pointer/physical-key processing.
pub fn after(ctx: &egui::Context, output: &egui::text_edit::TextEditOutput, text: &str) {
    if !output.response.has_focus() {
        return;
    }
    let range = output.cursor_range.unwrap_or_default();
    let (Some(anchor), Some(cursor)) = (
        utf16_offset(text, range.secondary.index),
        utf16_offset(text, range.primary.index),
    ) else {
        if let Ok(mut route) = ROUTE.lock() {
            if let Some(route) = route.as_mut() {
                route.error =
                    Some("Input exceeds Android's UTF-16 index range; source unchanged.".into());
            }
        }
        return;
    };
    let selection = [anchor, cursor];
    let Ok(mut route) = ROUTE.lock() else {
        return;
    };
    let Some(route) = route.as_mut() else {
        return;
    };
    let widget = output.response.id.value();
    let reset = route.session.as_ref().is_none_or(|session| {
        session.state.widget != widget
            || session.state.text != text
            || session.state.selection != selection
    });
    if reset {
        let Some(epoch) = allocate_epoch(route) else {
            return;
        };
        if !retire_session(route) {
            return;
        }
        let state = Snapshot {
            widget,
            epoch,
            seq: 0,
            text: text.into(),
            selection,
            composing: [-1, -1],
        };
        route.session = Some(Session {
            id: output.response.id,
            state: state.clone(),
            pending: None,
        });
        route.publish = Some(state);
    }
    if reset || output.response.clicked() {
        route.show = true;
        if let Some(session) = &route.session {
            route.publish = Some(session.state.clone());
        }
    }
    let _ = ctx;
}
pub fn menu(ui: &mut egui::Ui) {
    let mut selected = None;
    let menu = ui.menu_button("Edit", |ui| {
        for (label, kind) in [("Select all", 1), ("Copy", 2), ("Cut", 3), ("Paste", 4)] {
            if ui.button(label).clicked() {
                selected = Some(kind);
                ui.close();
            }
        }
    });
    if let Ok(mut route) = ROUTE.lock() {
        if let Some(route) = route.as_mut() {
            if menu.inner.is_some() && route.edit_target.is_none() {
                route.edit_target = route
                    .session
                    .as_ref()
                    .map(|session| (session.state.widget, session.state.epoch));
            }
            if let Some(kind) = selected {
                if let Some((widget, epoch)) = route.edit_target {
                    route.action = Some(Action {
                        widget,
                        epoch,
                        kind,
                    });
                }
            } else if menu.inner.is_none() {
                route.edit_target = None;
            }
        }
    }
}
pub fn notice(ui: &mut egui::Ui) {
    let notice = ROUTE.lock().ok().and_then(|route| {
        route
            .as_ref()
            .map(|route| (route.error.clone(), route.recovery.clone()))
    });
    if let Some((Some(error), recovery)) = notice {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
        if let Some(recovery) = recovery {
            ui.collapsing("Retained native input", |ui| {
                ui.label(recovery);
            });
        }
    }
}
pub fn copy_text(text: &str) {
    if let Ok(mut route) = ROUTE.lock() {
        if let Some(route) = route.as_mut() {
            route.clipboard = Some(text.into());
        }
    }
}

#[cfg(target_os = "android")]
mod android {
    use super::*;
    use jni::{
        EnvUnowned, JValue, JavaVM, jni_sig, jni_str,
        objects::{Global, JClass, JObject, JString},
    };
    pub struct AndroidInput {
        owner: i64,
        vm: JavaVM,
        activity: Global<JObject<'static>>,
    }
    impl AndroidInput {
        /// # Safety
        /// AndroidApp must own the live VM/global Activity handles throughout this call.
        pub unsafe fn new(
            vm: *mut std::ffi::c_void,
            activity: *mut std::ffi::c_void,
            owner: i64,
            wake: impl Fn() + Send + Sync + 'static,
        ) -> Result<Self, String> {
            let vm = unsafe { JavaVM::from_raw(vm.cast()) };
            let activity = vm
                .attach_current_thread(|env| -> jni::errors::Result<_> {
                    let raw = env.get_raw();
                    let local = unsafe { ((**raw).v1_2.NewLocalRef)(raw, activity.cast()) };
                    if local.is_null() {
                        return Err(jni::errors::Error::NullPtr("Activity reference"));
                    }
                    env.new_global_ref(unsafe { JObject::from_raw(env, local) })
                })
                .map_err(|error| error.to_string())?;
            *ROUTE.lock().map_err(|_| "Input route poisoned")? = Some(Route {
                owner,
                session: None,
                publish: None,
                show: false,
                action: None,
                edit_target: None,
                next_epoch: 0,
                interruptions: Vec::new(),
                clipboard: None,
                error: None,
                recovery: None,
                wake: Arc::new(wake),
            });
            Ok(Self {
                owner,
                vm,
                activity,
            })
        }
        pub fn flush(&mut self, ctx: &egui::Context) {
            let focused = ctx.memory(|memory| memory.focused()).map(|id| id.value());
            let popup_open = egui::Popup::is_any_open(ctx);
            let Some((snapshot, show, action, clipboard, hide, refocus)) =
                ROUTE.lock().ok().and_then(|mut route| {
                    let route = route.as_mut().filter(|route| route.owner == self.owner)?;

                    let mut hide = false;
                    let refocus = route.action.and_then(|action| {
                        route
                            .session
                            .as_ref()
                            .filter(|session| {
                                (session.state.widget, session.state.epoch)
                                    == (action.widget, action.epoch)
                            })
                            .map(|session| session.id)
                    });
                    if route
                        .session
                        .as_ref()
                        .is_some_and(|session| Some(session.state.widget) != focused)
                        && route.action.is_none()
                        && !(popup_open
                            && route.edit_target.is_some_and(|target| {
                                route.session.as_ref().is_some_and(|session| {
                                    target == (session.state.widget, session.state.epoch)
                                })
                            }))
                    {
                        if !retire_session(route) {
                            return None;
                        }
                        route.publish = None;
                        hide = true;
                    }
                    Some((
                        route.publish.take(),
                        std::mem::take(&mut route.show),
                        route.action.take(),
                        route.clipboard.take(),
                        hide,
                        refocus,
                    ))
                })
            else {
                return;
            };
            if let Some(id) = refocus {
                ctx.memory_mut(|memory| memory.request_focus(id));
            }
            let result = self
                .vm
                .attach_current_thread(|env| -> jni::errors::Result<()> {
                    if hide {
                        env.call_method(
                            &self.activity,
                            jni_str!("endInput"),
                            jni_sig!("(J)V"),
                            &[JValue::Long(self.owner)],
                        )?;
                    }
                    if let Some(snapshot) = snapshot {
                        let text = env.new_string(&snapshot.text)?;
                        env.call_method(
                            &self.activity,
                            jni_str!("updateInput"),
                            jni_sig!("(JJJJLjava/lang/String;IIIIZ)V"),
                            &[
                                JValue::Long(self.owner),
                                JValue::Long(snapshot.widget as i64),
                                JValue::Long(snapshot.epoch as i64),
                                JValue::Long(snapshot.seq as i64),
                                JValue::Object(&text),
                                JValue::Int(snapshot.selection[0]),
                                JValue::Int(snapshot.selection[1]),
                                JValue::Int(snapshot.composing[0]),
                                JValue::Int(snapshot.composing[1]),
                                JValue::Bool(show),
                            ],
                        )?;
                    }
                    if let Some(action) = action {
                        env.call_method(
                            &self.activity,
                            jni_str!("inputAction"),
                            jni_sig!("(JJJI)V"),
                            &[
                                JValue::Long(self.owner),
                                JValue::Long(action.widget as i64),
                                JValue::Long(action.epoch as i64),
                                JValue::Int(action.kind),
                            ],
                        )?;
                    }
                    if let Some(text) = clipboard {
                        let text = env.new_string(text)?;
                        env.call_method(
                            &self.activity,
                            jni_str!("copyInputText"),
                            jni_sig!("(JLjava/lang/String;)V"),
                            &[JValue::Long(self.owner), JValue::Object(&text)],
                        )?;
                    }
                    Ok(())
                });
            if let Err(error) = result {
                if let Ok(mut route) = ROUTE.lock() {
                    if let Some(route) = route.as_mut().filter(|route| route.owner == self.owner) {
                        route.error = Some(format!("Android input unavailable: {error}"));
                    }
                }
            }
        }
    }
    impl Drop for AndroidInput {
        fn drop(&mut self) {
            if let Ok(mut route) = ROUTE.lock() {
                if route
                    .as_ref()
                    .is_some_and(|route| route.owner == self.owner)
                {
                    *route = None;
                }
            }
        }
    }
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_org_tanglewood_TanglewoodActivity_nativeInputFailure<'local>(
        mut env: EnvUnowned<'local>,
        _class: JClass<'local>,
        owner: i64,
        error: JString<'local>,
    ) {
        env.with_env(|env| -> jni::errors::Result<()> {
            let error = error
                .try_to_string(env)
                .unwrap_or_else(|_| "Android input unavailable; draft retained.".into());
            let wake = ROUTE.lock().ok().and_then(|mut route| {
                route
                    .as_mut()
                    .filter(|route| route.owner == owner)
                    .map(|route| {
                        route.error = Some(error);
                        route.wake.clone()
                    })
            });
            if let Some(wake) = wake {
                wake();
            }
            Ok(())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>();
    }
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_org_tanglewood_TanglewoodActivity_nativeInput<'local>(
        mut env: EnvUnowned<'local>,
        _class: JClass<'local>,
        owner: i64,
        widget: i64,
        epoch: i64,
        seq: i64,
        text: JString<'local>,
        start: i32,
        end: i32,
        compose_start: i32,
        compose_end: i32,
    ) -> bool {
        env.with_env(|env| -> jni::errors::Result<bool> {
            let text = match text.try_to_string(env) {
                Ok(text)=>text,
                Err(_)=>{
                    let wake=ROUTE.lock().ok().and_then(|mut route|route.as_mut().filter(|route|route.owner==owner).map(|route|{
                        route.error=Some("Invalid UTF-16 input rejected; text retained in Android input mirror.".into());route.wake.clone()
                    }));
                    if let Some(wake)=wake{wake();}return Ok(false);
                }
            };
            let update = Snapshot {
                widget: widget as u64,
                epoch: epoch as u64,
                seq: seq as u64,
                text,
                selection: [start, end],
                composing: [compose_start, compose_end],
            };
            let (accepted, wake) = if let Ok(mut route) = ROUTE.lock() {
                if let Some(route) = route.as_mut().filter(|route| route.owner == owner) {
                    let result = route
                        .session
                        .as_mut()
                        .map(|session| accept(session, update.clone()));
                    let accepted = matches!(result, Some(Ok(())));
                    if !accepted {
                        route.error =
                            Some("Stale or invalid native input retained for recovery.".into());
                        route.recovery = Some(update.text);
                    }
                    (accepted, Some(route.wake.clone()))
                } else {
                    (false, None)
                }
            } else {
                (false, None)
            };
            if let Some(wake) = wake {
                wake();
            }
            Ok(accepted)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }
}
#[cfg(target_os = "android")]
pub use android::AndroidInput;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_boundaries_and_epoch_updates_preserve_source() {
        assert_eq!(scalar_offset("a🌲é", 3), Ok(2));
        assert!(scalar_offset("a🌲é", 2).is_err());
        let state = Snapshot {
            widget: 1,
            epoch: 7,
            seq: 0,
            text: "a🌲".into(),
            selection: [3, 3],
            composing: [-1, -1],
        };
        let mut session = Session {
            id: egui::Id::new("fixture"),
            state: state.clone(),
            pending: None,
        };
        let mut update = state.clone();
        update.seq = 1;
        update.text = "a🌲é".into();
        update.selection = [4, 4];
        update.composing = [3, 4];
        assert!(accept(&mut session, update.clone()).is_ok());
        update.seq = 2;
        update.composing = [-1, -1];
        assert!(accept(&mut session, update.clone()).is_ok());
        assert_eq!(session.pending.as_ref().unwrap().text, "a🌲é");
        assert_eq!(session.state.text, "a🌲");
        update.epoch = 6;
        update.seq = 3;
        assert!(accept(&mut session, update).is_err());
        assert_eq!(session.pending.as_ref().unwrap().seq, 2);
        let mut route = Route {
            owner: 1,
            session: Some(session),
            publish: None,
            show: false,
            action: None,
            edit_target: None,
            next_epoch: 7,
            interruptions: Vec::new(),
            clipboard: None,
            error: None,
            recovery: None,
            wake: Arc::new(|| {}),
        };
        assert!(retire_session(&mut route));
        assert_eq!(route.interruptions[0].widget, 1);
        assert_eq!(route.interruptions[0].text, "a🌲é");
        assert_eq!(route.interruptions[0].base, "a🌲");
        assert_eq!(allocate_epoch(&mut route), Some(8)); // No session remains, but old epoch is never reused.
        assert!(!edit_protects(&route, 1, 7, true)); // Arbitrary popups do not authorize input.
        route.edit_target = Some((1, 7));
        assert!(edit_protects(&route, 1, 7, true));
        assert!(!edit_protects(&route, 2, 7, true));
        assert!(!edit_protects(&route, 1, 8, true));
        route.action = Some(Action {
            widget: 1,
            epoch: 7,
            kind: 2,
        });
        assert!(edit_protects(&route, 1, 7, false)); // Exact captured action protects the closing frame.
        assert!(!edit_protects(&route, 2, 7, false));
        route.action = Some(Action {
            widget: 1,
            epoch: 8,
            kind: 2,
        });
        assert!(!edit_protects(&route, 1, 7, false));
        route.next_epoch = u64::MAX;
        assert_eq!(allocate_epoch(&mut route), None);
        assert_eq!(route.interruptions[0].text, "a🌲é");
    }
}
