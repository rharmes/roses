# roses — vhs demo (TASK-16)
#
# Records an animated GIF of roses for the README.
#
# Prerequisites:
#   * `roses` on your PATH (`cargo install roses`,
#     `brew install rharmes/tap/roses`, or `cargo build --release` + add
#     `target/release` to PATH).
#   * You are ALREADY logged in to Feedbin — run `roses` once and sign in, so the
#     demo never types credentials (none are stored in this script).
#   * vhs (+ ttyd + ffmpeg): https://github.com/charmbracelet/vhs
#
# Generate:  vhs demo/roses.tape      # writes demo/roses.gif
# Validate:  vhs validate demo/roses.tape
#
# Timing/colors are easy to tweak — adjust the Sleeps and Theme to taste.

Output demo/roses.gif

# Bail out early with a clear message if roses isn't installed.
Require roses

Set Shell "bash"
Set FontSize 16
Set Width 1280
Set Height 720
Set Padding 20
Set Theme "Dracula"
Set TypingSpeed 90ms

# --- Launch roses (already authenticated to Feedbin) ---
Type "roses"
Sleep 600ms
Enter
Sleep 3s              # let the newest unread entries load

# --- Browse the Sources column (feeds + unread counts) ---
Down@500ms 2
Sleep 1200ms

# --- Focus the Articles column and skim a few titles ---
Right
Sleep 800ms
Down@450ms 3
Sleep 1200ms

# --- Focus the Reader and scroll the body (reveals the scrollbar) ---
Right
Sleep 800ms
Down@180ms 12
Sleep 1500ms

# --- Mark read, then undo (non-destructive — restores the entry) ---
Left                  # back to the Articles column
Sleep 600ms
Type "m"              # mark the selected article read
Sleep 1500ms
Type "u"              # undo
Sleep 1500ms

# --- Quit back to the shell ---
Type "q"
Sleep 1200ms

# --- Finale: the "all caught up" rose (offline; no account needed) ---
Type "roses preview"
Sleep 400ms
Enter
Sleep 3s
Type "q"
Sleep 1200ms
