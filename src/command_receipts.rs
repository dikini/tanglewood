//! Shared confirmation of accepted domain work, independent of its adapter.
use crate::{App, app::IoStatus, commands::CommandResult};

pub(crate) enum Confirmation {
    Pending,
    Completed(CommandResult),
    Unknown(String),
}

pub(crate) fn confirm(app: &App, result: &CommandResult) -> Confirmation {
    match result {
        CommandResult::Note(accepted) => match app.note(accepted.note.id) {
            Ok(current) if current.save.persisted == Some(accepted.note.revision) => {
                let mut confirmed = accepted.clone();
                confirmed.save.persisted = Some(accepted.note.revision);
                Confirmation::Completed(CommandResult::Note(confirmed))
            }
            Ok(current) if current.save.error.is_some() => Confirmation::Unknown(format!(
                "accepted but storage failed: {}",
                current.save.error.unwrap_or_default()
            )),
            Ok(current) if current.note.revision != accepted.note.revision => {
                Confirmation::Unknown(
                    "accepted revision was superseded before confirmation; re-read state".into(),
                )
            }
            Err(_) => Confirmation::Unknown("accepted note no longer available".into()),
            _ => Confirmation::Pending,
        },
        CommandResult::Accepted { request_id } => match app.io_status(*request_id) {
            Some(IoStatus::Completed) => Confirmation::Completed(result.clone()),
            Some(IoStatus::Failed(error)) => Confirmation::Unknown(format!(
                "accepted but storage failed: {error}; inspect state before retry"
            )),
            Some(IoStatus::Conflict(id)) => Confirmation::Unknown(format!(
                "accepted request has conflict {id}; inspect state before retry"
            )),
            Some(IoStatus::Pending) | None => Confirmation::Pending,
        },
        _ => Confirmation::Unknown("unsupported mutation acknowledgement".into()),
    }
}
