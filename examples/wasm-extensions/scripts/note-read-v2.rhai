// Input is a note UUID. This query-only script can use a read-only grant.
// The generic command function accepts every typed v2 WIT command.
let reply = command(#{ ReadNote: #{ id: input } });
let current = reply.Ok.value.Note.note;
current.path + ":" + current.text
