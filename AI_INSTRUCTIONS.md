# AI instructions: install and configure rexflash

This file is for AI agents (Claude Code, Codex and others) that use the Rex skill (`/rex`) and the
`rex` command. Follow the steps in order. Do not skip a check. When all checks pass, rexflash is
installed and configured with no errors.

rexflash is a Rex extension. It has two parts:

- the `rexflash` binary (Rust), installed by Homebrew
- `rexflash.lua`, a Lua file that `init.lua` loads. It defines the Rex actions `rexflash_hint` and
  `rexflash_flash`, and it binds keys to them.

## Rules

- Work only on macOS. rexflash uses `pbcopy` and `open`. On another system, stop and tell the user.
- Change only what this file says. Do not edit other keys, modes or actions in `init.lua`.
- Do not run `rexflash` yourself in a terminal. It opens `/dev/tty` and waits for a key, even when
  you give it a wrong argument. Only the Lua extension starts it.
- Do not run `rexflash_hint` or `rexflash_flash` in your own pane. The overlay covers the pane and
  waits for a key. Use a scratch session (step 6).
- If a step fails, read [Troubleshooting](#troubleshooting). Do not retry the same command blindly.
- At the end, tell the user what you changed: the installed package, the lines you added to
  `init.lua`, and the keys you chose.

## 1. Check the system

```sh
uname -s            # must print Darwin
rex whoami --json   # must connect to a Rex server
command -v brew     # must print a path
```

If `rex whoami` fails, the Rex server is not running. Ask the user to start Rex. If `brew` is
missing, use the [other install methods](README.md#install) in the README.

## 2. Install

```sh
brew install g4rcez/tap/rexflash
```

Then check the two files and save the extension path. Do not write the path by hand. The Homebrew
prefix is `/opt/homebrew` on Apple Silicon and `/usr/local` on Intel.

```sh
REXFLASH_LUA="$(brew --prefix)/opt/rexflash/share/rexflash/rexflash.lua"
test -x "$(brew --prefix)/opt/rexflash/bin/rexflash" && test -f "$REXFLASH_LUA" && echo "$REXFLASH_LUA"
```

The command must print the path. If rexflash is already installed, `brew upgrade g4rcez/tap/rexflash`
is enough. The `opt` path does not change when the version changes.

## 3. Find `init.lua`

Rex reads one file, in this order:

1. the file in the variable `REX_CONFIG`, if it is set
2. `$XDG_CONFIG_HOME/rex/init.lua`, or `~/.config/rex/init.lua` when `XDG_CONFIG_HOME` is not set

```sh
INIT="${REX_CONFIG:-${XDG_CONFIG_HOME:-$HOME/.config}/rex/init.lua}"
INIT="$(realpath "$INIT" 2>/dev/null || echo "$INIT")"   # follow a symlink, as with a dotfiles repository
echo "$INIT"
```

Edit the real file that `realpath` prints. If the file does not exist, create it. Then check whether
rexflash is already set up:

```sh
grep -n rexflash "$INIT"
```

If a `dofile(...rexflash.lua)` line exists, do not add a second one. Update its path and options, then
go to step 5.

## 4. Choose the keys and edit `init.lua`

Default keys: `ctrl+p>y` for hint mode and `ctrl+p>/` for flash mode. A `>` means a key sequence: press
`ctrl+p`, release it, then press `y`.

Check that the keys are free:

```sh
rex keymap | grep -E 'ctrl\+p +(y|/)( |$)'
```

- No output: the keys are free. Use the default keys.
- A line for `rexflash_hint` or `rexflash_flash`: they are already bound. Do nothing.
- A line for another action: the key is used. Do not replace it. Choose two free keys, for example
  `ctrl+p>h` and `ctrl+p>f`, and check them the same way. Tell the user which keys you chose.

If `init.lua` already defines a prefix variable (for example `local PREFIX = "ctrl+p>"`), use it in
the same way (`hint_key = PREFIX .. "y"`).

Add this at the top level of `init.lua`, not inside a function. Use the value of `$REXFLASH_LUA` from
step 2 as a literal absolute path. Lua does not expand `~` or shell variables.

```lua
dofile("/opt/homebrew/opt/rexflash/share/rexflash/rexflash.lua") {
    hint_key = "ctrl+p>y",
    flash_key = "ctrl+p>/",
}
```

Keep the call in this form: `dofile("PATH")` directly followed by `{ ... }`. Other options (actions,
patterns, alphabet, scrollback, theme) are in the [README](README.md#options). Add them only when the
user asks. A key that rexflash does not know is an error that shows in the overlay when you use it.

## 5. Validate

Run these three commands. Each must succeed before the next.

```sh
rex config check
```

Expected: no line that starts with `error`, and the exit status is 0. The counts for `bindings` and
`actions` include the two rexflash bindings and two rexflash actions.

```sh
rex config reload
```

Expected: exit status 0. A failed reload keeps the last working keys, so the old setup stays active
and rexflash is not loaded. Fix the error and run `rex config reload` again.

```sh
rex actions | grep rexflash       # rexflash_hint and rexflash_flash
rex keymap | grep rexflash        # your two keys, with the file name of rexflash.lua
```

Both must print two lines.

## 6. Smoke test (optional, no effect on the user's panes)

Use a new session so the overlay never covers your own pane. Use `-C` to name the Rex app client when
more than one client is connected (`rex client ls`).

```sh
created=$(rex new rexflash-check --window check --json -- sh -c 'echo "see https://example.com and src/main.rs:42"; exec sleep 600') || exit
session=$(printf '%s' "$created" | jq -er '.session_id')
block=$(printf '%s' "$created" | jq -er '.initial_windows[0].block_ids[0]')
sleep 1
rex do rexflash_hint -s "$session" -b "$block"
sleep 2
rex block ls -s "$session"        # a block with the label "rexflash" must be there
```

Read the overlay with a capture. Each label replaces the first letter of its match, so the text
shows `sttps://example.com` and `arc/main.rs:42`:

```sh
float=$(rex block ls -s "$session" --json | jq -r '.. | objects | select(.label? == "rexflash") | .block_id' | head -1)
rex capture -s "$session" -b "$float" --trim
```

Then close the overlay and remove the session. Always do this, also when a step fails:

```sh
rex send-key -s "$session" -b "$float" escape
rex session kill "$session"
```

If the `rex block ls` output shows no `rexflash` block, or the overlay shows an error, read
[Troubleshooting](#troubleshooting).

## 7. Report

Tell the user:

- the installed version (`brew list --versions rexflash`)
- the `init.lua` file you changed and the lines you added
- the two keys, and that they work after the reload
- what to do: press the hint key in a pane, press a label to copy, Shift plus a label to paste, Ctrl
  plus a label to open. Press the same key again or Esc to close.

## Troubleshooting

| Symptom | Cause | Fix |
| ------- | ----- | --- |
| `rex config check` prints `error: open ...rexflash.lua: no such file or directory` | The path in `dofile` is wrong, or the package is not installed | Run step 2 again and copy the path that it prints. |
| `error ... key sequence "..." has an empty key` | A malformed key, for example `ctrl+p>>y` | Use the form `ctrl+p>y`: one `>` between keys. |
| `rex config reload` fails | An error in `init.lua` | Run `rex config check`, fix the first error, reload again. The last working config stays active. |
| `rex actions` shows no `rexflash` line | The reload failed, or the `dofile` line is not at the top level | Fix `init.lua` and reload. Put the call outside any function. |
| A key does nothing | Another binding takes the key | Run `rex keymap` and choose free keys. |
| Overlay shows `rexflash: config: unknown field ...` | An option name that rexflash does not know | Remove it or correct the name. Options are in the README. |
| Overlay shows `rexflash: config: ...` with a value | An invalid value (for example `hint_position`, `alphabet`, `scrollback`, a style with a letter) | Use a value that the README allows. |
| Overlay shows `pbcopy` or `open` errors | The system is not macOS, or `pbcopy` is not in the PATH | rexflash supports macOS only. |
| Overlay shows an error about `rex` | The binary cannot find the `rex` command | Set `REX_BIN` to the full path of `rex` in the Rex environment. |
| The overlay shows a command-not-found or permission error | The binary path is wrong, or the file lost its execute permission | `ls -l "$(brew --prefix)/opt/rexflash/bin/rexflash"`. Run `brew reinstall g4rcez/tap/rexflash`. To use another binary, set `bin = "/full/path/rexflash"`. |
| Overlay shows `press any key` | rexflash found an error and waits so that you can read it | Read the message above it. Press a key to close. |
| Overlay has the wrong colors | Rex does not give a global theme to a floating terminal | Pass `theme = { foreground = "#...", background = "#..." }` in the options. |

## Remove

1. Delete the `dofile(...rexflash.lua) { ... }` call from `init.lua`.
2. Run `rex config reload`.
3. Run `brew uninstall g4rcez/tap/rexflash`.
