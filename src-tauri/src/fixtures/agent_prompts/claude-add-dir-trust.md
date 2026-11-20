# Claude "Adding a new directory" trust dialog

Source: Claude Code 2.1.328 (`claude --add-dir=<directory>`, the grant TUIC
passes for a session's attachment directory) in an owned Python PTY, recorded
on 2026-11-20 at 41 rows by 80 columns. The directory was
`/Users/Shared/tuic-fixture/attachments/pty-add-dir-fixture`, an empty
directory created for the recording and removed afterwards; it was not
trusted in the recording profile, and the working directory was an already
trusted repository, so only this dialog rendered. No prompt was submitted and
no production TUIC session was used.

The capture holds the dialog ("Adding a new directory:", the path, "Is this a
directory you created or one you trust?", default `No, exit` above `Yes, add
it`, footer `Enter to confirm · Esc to cancel`), the recorded answer
Down+Enter, and whatever Claude drew next. The recorder then sent SIGTERM to
its own child. The records are TUICCAP2, written by a script from the PTY
reads (read boundaries preserved, timestamps are wall-clock offsets).

SHA-256: `877ea6759861f95b123c7c11544259ecda01fc91111155438bb9371d91c8211a`.

The managed first-run tests replay the output records through the production
chunk processor: a managed Claude child answers it only when the path is its
own session's attachment directory.
