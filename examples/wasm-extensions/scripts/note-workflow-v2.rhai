// Input is a new vault-relative Markdown path; offsets count UTF-8 bytes.
let created = create_note(input, "Rhai extension");
let edited = edit_note(created.id, created.revision, 14, 14, " edited");
let current = read_note(edited.id);
current.id + ":" + current.text
