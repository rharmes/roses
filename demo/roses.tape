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
Sleep 2000ms
Enter
Sleep 3s              # let the newest unread entries load

# --- Browse the Sources column (feeds + unread counts) ---
Down@400ms 4
Sleep 1500ms

# --- Focus the Articles column and skim a few titles ---
Right
Sleep 1000ms
Down@800ms 5
Sleep 600ms

# --- Focus the Reader and scroll the body (reveals the scrollbar) ---
Right
Sleep 600ms
Down@100ms 13
Sleep 1500ms

# --- Back in the Articles column: scroll further down the list ---
Left                  # back to the Articles column
Sleep 800ms
Down@500ms 6
Sleep 1600ms

# --- Switch feeds: ← to Sources, ↑ to another feed, → into its articles and reader ---
Left
Sleep 800ms
Up@400ms 3
Sleep 800ms
Right
Sleep 800ms
Down@500ms 2
Sleep 800ms
Right
Sleep 800ms
Down@300ms 5
Sleep 1600ms

# --- Quit back to the shell ---
Type "q"